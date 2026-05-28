use std::collections::HashMap;
use std::io;
use std::net::{Shutdown, SocketAddr, UdpSocket as StdUdpSocket};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::core::{
    Error, HandlerFuture, HandlerFutureBox, ServerRuntime, ServiceConfig, linux_reuseport_select,
};
use crate::platform;
use crate::runtime;

pub(crate) type UdpHandler =
    Arc<dyn Fn(UdpSocket, PacketMeta, Vec<u8>) -> HandlerFutureBox + Send + Sync>;
pub(crate) type SocketCallback = Arc<dyn Fn(UdpSocket) -> HandlerFutureBox + Send + Sync>;

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct PacketMeta {
    pub peer_addr: Option<SocketAddr>,
    pub local_addr: Option<SocketAddr>,
}

#[derive(Clone)]
pub struct UdpSocket {
    inner: UdpSocketInner,
}

#[derive(Clone)]
enum UdpSocketInner {
    Runtime(runtime::UdpSocket),
    #[allow(dead_code)]
    Routed(Arc<RoutedUdpSocket>),
}

struct RoutedUdpSocket {
    sender: StdUdpSocket,
    receiver: RoutedPacketReceiver,
    local_addr: SocketAddr,
}

struct RoutedPacket {
    payload: Vec<u8>,
    peer_addr: SocketAddr,
}

#[cfg(feature = "runtime-async-std")]
type RoutedPacketSender = async_std::channel::Sender<RoutedPacket>;
#[cfg(feature = "runtime-async-std")]
type RoutedPacketReceiver = async_std::channel::Receiver<RoutedPacket>;

#[cfg(feature = "runtime-tokio")]
type RoutedPacketSender = tokio::sync::mpsc::UnboundedSender<RoutedPacket>;
#[cfg(feature = "runtime-tokio")]
type RoutedPacketReceiver = tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<RoutedPacket>>;

#[cfg(feature = "runtime-tokio-uring")]
type RoutedPacketSender = std::sync::mpsc::Sender<RoutedPacket>;
#[cfg(feature = "runtime-tokio-uring")]
type RoutedPacketReceiver = Mutex<std::sync::mpsc::Receiver<RoutedPacket>>;

impl UdpSocket {
    pub async fn recv_from(&self, buffer: &mut [u8]) -> Result<(usize, SocketAddr), Error> {
        match &self.inner {
            UdpSocketInner::Runtime(socket) => runtime::udp_recv_from_slice(socket, buffer)
                .await
                .map_err(Error::from),
            UdpSocketInner::Routed(socket) => socket.recv_from_slice(buffer).await.map_err(Error::from),
        }
    }

    pub async fn send_to(&self, buffer: &[u8], target: SocketAddr) -> Result<usize, Error> {
        match &self.inner {
            UdpSocketInner::Runtime(socket) => runtime::udp_send_to(socket, buffer, target)
                .await
                .map_err(Error::from),
            UdpSocketInner::Routed(socket) => socket.sender.send_to(buffer, target).map_err(Error::from),
        }
    }

    pub fn local_addr(&self) -> Result<SocketAddr, Error> {
        match &self.inner {
            UdpSocketInner::Runtime(socket) => socket.local_addr().map_err(Error::from),
            UdpSocketInner::Routed(socket) => Ok(socket.local_addr),
        }
    }

    pub(crate) fn from_runtime(inner: runtime::UdpSocket) -> Self {
        Self {
            inner: UdpSocketInner::Runtime(inner),
        }
    }

    #[allow(dead_code)]
    fn from_routed(inner: RoutedUdpSocket) -> Self {
        Self {
            inner: UdpSocketInner::Routed(Arc::new(inner)),
        }
    }

    /// Receives into an owned buffer without copying on runtimes that can
    /// take ownership of the buffer, such as tokio-uring.
    pub async fn recv_from_vec(&self, buffer: Vec<u8>) -> Result<(usize, SocketAddr, Vec<u8>), Error> {
        match &self.inner {
            UdpSocketInner::Runtime(socket) => runtime::udp_recv_from(socket, buffer)
                .await
                .map_err(Error::from),
            UdpSocketInner::Routed(socket) => socket.recv_from_vec(buffer).await.map_err(Error::from),
        }
    }
}

impl RoutedUdpSocket {
    async fn recv_from_slice(&self, buffer: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        let packet = self.recv_packet().await?;
        let len = packet.payload.len().min(buffer.len());
        buffer[..len].copy_from_slice(&packet.payload[..len]);
        Ok((len, packet.peer_addr))
    }

    async fn recv_from_vec(&self, mut buffer: Vec<u8>) -> io::Result<(usize, SocketAddr, Vec<u8>)> {
        let packet = self.recv_packet().await?;
        let len = packet.payload.len().min(buffer.len());
        buffer[..len].copy_from_slice(&packet.payload[..len]);
        Ok((len, packet.peer_addr, buffer))
    }

    async fn recv_packet(&self) -> io::Result<RoutedPacket> {
        #[cfg(feature = "runtime-async-std")]
        {
            self.receiver.recv().await.map_err(|_| {
                io::Error::new(io::ErrorKind::UnexpectedEof, "routed UDP socket closed")
            })
        }
        #[cfg(feature = "runtime-tokio")]
        {
            self.receiver.lock().await.recv().await.ok_or_else(|| {
                io::Error::new(io::ErrorKind::UnexpectedEof, "routed UDP socket closed")
            })
        }
        #[cfg(feature = "runtime-tokio-uring")]
        {
            self.receiver
                .lock()
                .map_err(|_| io::Error::other("routed UDP receiver lock poisoned"))?
                .recv()
                .map_err(|_| io::Error::new(io::ErrorKind::UnexpectedEof, "routed UDP socket closed"))
        }
    }
}

#[derive(Clone)]
pub struct UdpServer {
    state: Arc<UdpServerState>,
}

pub(crate) struct UdpServerState {
    active: Arc<AtomicBool>,
    tasks: Mutex<Vec<runtime::TaskHandle>>,
    sockets: Mutex<Vec<StdUdpSocket>>,
    close_sockets: Mutex<Vec<StdUdpSocket>>,
    thread_sockets: Mutex<HashMap<thread::ThreadId, StdUdpSocket>>,
    next_socket: AtomicUsize,
}

impl UdpServer {
    pub fn serve<F, Fut>(
        runtime: &ServerRuntime,
        config: ServiceConfig,
        handler: F,
    ) -> Result<Self, Error>
    where
        F: Fn(UdpSocket, PacketMeta, Vec<u8>) -> Fut + Clone + Send + Sync + 'static,
        Fut: HandlerFuture,
    {
        config.validate()?;
        let server = Self {
            state: Arc::new(UdpServerState::new()),
        };
        if !platform::supports_reuse_port_balancing() {
            add_simulated_listener(runtime, config, handler, Arc::clone(&server.state))?;
        } else {
            add_reuse_port_listener(runtime, config, handler, Arc::clone(&server.state))?;
        }
        Ok(server)
    }

    pub fn close(&self) -> Result<(), Error> {
        self.state.close();
        Ok(())
    }

    pub fn listener_socket(&self) -> Result<UdpSocket, Error> {
        self.state.listener_socket()
    }

    pub fn serve_socket<F, Fut>(
        runtime: &ServerRuntime,
        config: ServiceConfig,
        callback: F,
    ) -> Result<Self, Error>
    where
        F: Fn(UdpSocket) -> Fut + Clone + Send + Sync + 'static,
        Fut: HandlerFuture,
    {
        config.validate()?;
        let server = Self {
            state: Arc::new(UdpServerState::new()),
        };
        if !platform::supports_reuse_port_balancing() {
            add_socket_callback_simulated_listener(
                runtime,
                config,
                callback,
                Arc::clone(&server.state),
                |_packet, meta, worker_count| linux_reuseport_select(meta, worker_count).ok(),
            )?;
        } else {
            add_socket_callback_reuse_port_listener(
                runtime,
                config,
                callback,
                Arc::clone(&server.state),
            )?;
        }
        Ok(server)
    }
}

impl UdpServerState {
    pub(crate) fn new() -> Self {
        Self {
            active: Arc::new(AtomicBool::new(true)),
            tasks: Mutex::new(Vec::new()),
            sockets: Mutex::new(Vec::new()),
            close_sockets: Mutex::new(Vec::new()),
            thread_sockets: Mutex::new(HashMap::new()),
            next_socket: AtomicUsize::new(0),
        }
    }

    pub(crate) fn close(&self) {
        self.active.store(false, Ordering::SeqCst);
        if let Ok(mut sockets) = self.sockets.lock() {
            for socket in sockets.drain(..) {
                let _ = socket2::SockRef::from(&socket).shutdown(Shutdown::Both);
            }
        }
        if let Ok(mut sockets) = self.close_sockets.lock() {
            for socket in sockets.drain(..) {
                let _ = socket2::SockRef::from(&socket).shutdown(Shutdown::Both);
            }
        }
        if let Ok(mut thread_sockets) = self.thread_sockets.lock() {
            thread_sockets.clear();
        }
        if let Ok(mut tasks) = self.tasks.lock() {
            for task in tasks.drain(..) {
                task.cancel();
            }
        }
    }

    pub(crate) fn register_task(&self, task: runtime::TaskHandle) -> Result<(), Error> {
        self.tasks
            .lock()
            .map_err(|_| Error::Runtime("udp task registry lock poisoned".to_string()))?
            .push(task);
        Ok(())
    }

    pub(crate) fn active_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.active)
    }

    pub(crate) fn register_listener_socket(&self, socket: &StdUdpSocket) -> Result<(), Error> {
        let thread_socket = socket.try_clone().map_err(Error::from)?;
        let pool_socket = socket.try_clone().map_err(Error::from)?;
        self.thread_sockets
            .lock()
            .map_err(|_| Error::Runtime("server socket registry lock poisoned".to_string()))?
            .insert(thread::current().id(), thread_socket);
        self.sockets
            .lock()
            .map_err(|_| Error::Runtime("server socket registry lock poisoned".to_string()))?
            .push(pool_socket);
        Ok(())
    }

    #[allow(dead_code)]
    fn register_close_socket(&self, socket: &StdUdpSocket) -> Result<(), Error> {
        self.close_sockets
            .lock()
            .map_err(|_| Error::Runtime("server socket registry lock poisoned".to_string()))?
            .push(socket.try_clone().map_err(Error::from)?);
        Ok(())
    }

    pub(crate) fn listener_socket(&self) -> Result<UdpSocket, Error> {
        if !self.active.load(Ordering::SeqCst) {
            return Err(Error::Runtime("server is closed".to_string()));
        }
        if let Some(socket) = self
            .thread_sockets
            .lock()
            .map_err(|_| Error::Runtime("server socket registry lock poisoned".to_string()))?
            .get(&thread::current().id())
            .map(|socket| socket.try_clone())
            .transpose()
            .map_err(Error::from)?
        {
            return runtime::udp_socket_from_std(socket)
                .map(UdpSocket::from_runtime)
                .map_err(Error::from);
        }

        let sockets = self
            .sockets
            .lock()
            .map_err(|_| Error::Runtime("server socket registry lock poisoned".to_string()))?;
        if sockets.is_empty() {
            return Err(Error::Runtime("server has no listener socket".to_string()));
        }
        let seed = self.next_socket.fetch_add(1, Ordering::SeqCst)
            ^ current_time_seed();
        let index = seed % sockets.len();
        let socket = sockets[index].try_clone().map_err(Error::from)?;
        runtime::udp_socket_from_std(socket)
            .map(UdpSocket::from_runtime)
            .map_err(Error::from)
    }
}

fn add_reuse_port_listener<F, Fut>(
    runtime: &ServerRuntime,
    service_config: ServiceConfig,
    handler: F,
    state: Arc<UdpServerState>,
) -> Result<(), Error>
where
    F: Fn(UdpSocket, PacketMeta, Vec<u8>) -> Fut + Clone + Send + Sync + 'static,
    Fut: HandlerFuture,
{
    let sockets = platform::bind_udp_workers(&service_config, runtime.worker_count())?;
    if sockets.is_empty() {
        return Err(Error::InvalidConfig(
            "worker count must be greater than zero".to_string(),
        ));
    }
    let runtime_active = runtime.active_flag();

    let handler = udp_handler(handler);
    for (worker_id, socket) in sockets.into_iter().enumerate() {
        let runtime_active = Arc::clone(&runtime_active);
        let server_active = state.active_flag();
        let task_state = Arc::clone(&state);
        let handler = Arc::clone(&handler);
        let task = runtime.submit_to_worker(worker_id, move || async move {
            if task_state.register_listener_socket(&socket).is_err() {
                return;
            }
            let Ok(socket) = runtime::udp_socket_from_std(socket)
                .map(UdpSocket::from_runtime)
                .map_err(Error::from)
            else {
                return;
            };
            udp_listener_loop(socket, runtime_active, server_active, handler).await;
        })?;
        state.register_task(task)?;
    }

    Ok(())
}

fn add_socket_callback_reuse_port_listener<F, Fut>(
    runtime: &ServerRuntime,
    service_config: ServiceConfig,
    callback: F,
    state: Arc<UdpServerState>,
) -> Result<(), Error>
where
    F: Fn(UdpSocket) -> Fut + Clone + Send + Sync + 'static,
    Fut: HandlerFuture,
{
    let sockets = platform::bind_udp_workers(&service_config, runtime.worker_count())?;
    if sockets.is_empty() {
        return Err(Error::InvalidConfig(
            "worker count must be greater than zero".to_string(),
        ));
    }

    let callback = socket_callback(callback);
    for (worker_id, socket) in sockets.into_iter().enumerate() {
        let task_state = Arc::clone(&state);
        let callback = Arc::clone(&callback);
        let task = runtime.submit_to_worker(worker_id, move || async move {
            if task_state.register_listener_socket(&socket).is_err() {
                return;
            }
            let Ok(socket) = runtime::udp_socket_from_std(socket)
                .map(UdpSocket::from_runtime)
                .map_err(Error::from)
            else {
                return;
            };
            let _ = callback(socket).await;
        })?;
        state.register_task(task)?;
    }

    Ok(())
}

fn add_simulated_listener<F, Fut>(
    runtime: &ServerRuntime,
    service_config: ServiceConfig,
    handler: F,
    state: Arc<UdpServerState>,
) -> Result<(), Error>
where
    F: Fn(UdpSocket, PacketMeta, Vec<u8>) -> Fut + Clone + Send + Sync + 'static,
    Fut: HandlerFuture,
{
    if !runtime::SUPPORTS_USERSPACE_REUSEPORT_SIMULATION {
        return Err(Error::UnsupportedPlatformOption(
            "selected runtime requires native reuse-port worker sockets".to_string(),
        ));
    }

    let socket = platform::bind_udp(&service_config)?;
    let runtime_active = runtime.active_flag();
    let server_active = state.active_flag();

    let worker_executors = runtime.worker_executors();
    let worker_count = worker_executors.len();
    let handler = udp_handler(handler);
    let task_state = Arc::clone(&state);
    let task = runtime.submit_to_worker(0, move || async move {
        if task_state.register_listener_socket(&socket).is_err() {
            return;
        }
        let Ok(socket) = runtime::udp_socket_from_std(socket)
            .map(UdpSocket::from_runtime)
            .map_err(Error::from)
        else {
            return;
        };
        simulated_udp_listener_loop(
            socket,
            runtime_active,
            server_active,
            worker_executors,
            worker_count,
            handler,
        )
        .await;
    })?;
    state.register_task(task)?;

    Ok(())
}

pub(crate) fn add_socket_callback_simulated_listener<F, Fut, R>(
    runtime: &ServerRuntime,
    service_config: ServiceConfig,
    callback: F,
    state: Arc<UdpServerState>,
    route_packet: R,
) -> Result<(), Error>
where
    F: Fn(UdpSocket) -> Fut + Clone + Send + Sync + 'static,
    Fut: HandlerFuture,
    R: Fn(&[u8], PacketMeta, usize) -> Option<usize> + Clone + Send + Sync + 'static,
{
    #[cfg(feature = "runtime-tokio-uring")]
    {
        let _ = (runtime, service_config, callback, state, route_packet);
        return Err(Error::UnsupportedPlatformOption(
            "selected runtime requires native reuse-port worker sockets".to_string(),
        ));
    }

    #[cfg(any(feature = "runtime-tokio", feature = "runtime-async-std"))]
    {
        if !runtime::SUPPORTS_USERSPACE_REUSEPORT_SIMULATION {
            return Err(Error::UnsupportedPlatformOption(
                "selected runtime requires native reuse-port worker sockets".to_string(),
            ));
        }

        let socket = platform::bind_udp(&service_config)?;
        state.register_close_socket(&socket)?;

        let local_addr = socket.local_addr().map_err(Error::from)?;
        let worker_count = runtime.worker_count();
        if worker_count == 0 {
            return Err(Error::InvalidConfig(
                "worker count must be greater than zero".to_string(),
            ));
        }

        let mut senders = Vec::with_capacity(worker_count);
        let callback = socket_callback(callback);
        for worker_id in 0..worker_count {
            let (sender, receiver) = routed_packet_channel();
            senders.push(sender);
            let routed_socket = UdpSocket::from_routed(RoutedUdpSocket {
                sender: socket.try_clone().map_err(Error::from)?,
                receiver,
                local_addr,
            });
            let callback = Arc::clone(&callback);
            let task = runtime.submit_to_worker(worker_id, move || async move {
                let _ = callback(routed_socket).await;
            })?;
            state.register_task(task)?;
        }

        let active = state.active_flag();
        let task = runtime.submit_to_worker(0, move || async move {
            let Ok(socket) = runtime::udp_socket_from_std(socket)
                .map(UdpSocket::from_runtime)
                .map_err(Error::from)
            else {
                return;
            };
            dispatch_socket_only_packets(
                socket,
                local_addr,
                worker_count,
                route_packet,
                active,
                senders,
            )
            .await;
        })?;
        state.register_task(task)?;

        Ok(())
    }
}

pub(crate) fn udp_handler<F, Fut>(handler: F) -> UdpHandler
where
    F: Fn(UdpSocket, PacketMeta, Vec<u8>) -> Fut + Clone + Send + Sync + 'static,
    Fut: HandlerFuture,
{
    Arc::new(move |socket, meta, payload| {
        let future = handler.clone()(socket, meta, payload);
        Box::pin(future) as HandlerFutureBox
    })
}

pub(crate) fn socket_callback<F, Fut>(callback: F) -> SocketCallback
where
    F: Fn(UdpSocket) -> Fut + Clone + Send + Sync + 'static,
    Fut: HandlerFuture,
{
    Arc::new(move |socket| {
        let future = callback.clone()(socket);
        Box::pin(future) as HandlerFutureBox
    })
}

#[allow(dead_code)]
async fn dispatch_socket_only_packets<R>(
    socket: UdpSocket,
    local_addr: SocketAddr,
    worker_count: usize,
    route_packet: R,
    active: Arc<AtomicBool>,
    senders: Vec<RoutedPacketSender>,
) where
    R: Fn(&[u8], PacketMeta, usize) -> Option<usize>,
{
    let mut buffer = vec![0_u8; 65_536];
    while active.load(Ordering::SeqCst) {
        let (len, peer_addr, returned_buffer) = match socket.recv_from_vec(buffer).await {
            Ok(result) => result,
            Err(_) => {
                if active.load(Ordering::SeqCst) {
                    buffer = vec![0_u8; 65_536];
                    continue;
                }
                break;
            }
        };
        buffer = returned_buffer;
        let meta = PacketMeta {
            peer_addr: Some(peer_addr),
            local_addr: Some(local_addr),
        };
        let Some(worker_id) = route_packet(&buffer[..len], meta, worker_count) else {
            continue;
        };
        let Some(sender) = senders.get(worker_id) else {
            break;
        };
        if send_routed_packet(
            sender,
            RoutedPacket {
                payload: buffer[..len].to_vec(),
                peer_addr,
            },
        )
        .await
        .is_err()
        {
            break;
        }
    }
}

#[cfg(feature = "runtime-async-std")]
fn routed_packet_channel() -> (RoutedPacketSender, RoutedPacketReceiver) {
    async_std::channel::unbounded()
}

#[cfg(feature = "runtime-tokio")]
fn routed_packet_channel() -> (RoutedPacketSender, RoutedPacketReceiver) {
    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
    (sender, tokio::sync::Mutex::new(receiver))
}

#[cfg(feature = "runtime-tokio-uring")]
#[allow(dead_code)]
fn routed_packet_channel() -> (RoutedPacketSender, RoutedPacketReceiver) {
    let (sender, receiver) = std::sync::mpsc::channel();
    (sender, Mutex::new(receiver))
}

#[cfg(feature = "runtime-async-std")]
async fn send_routed_packet(
    sender: &RoutedPacketSender,
    packet: RoutedPacket,
) -> Result<(), ()> {
    sender.send(packet).await.map_err(|_| ())
}

#[cfg(feature = "runtime-tokio")]
async fn send_routed_packet(
    sender: &RoutedPacketSender,
    packet: RoutedPacket,
) -> Result<(), ()> {
    sender.send(packet).map_err(|_| ())
}

#[cfg(feature = "runtime-tokio-uring")]
async fn send_routed_packet(
    sender: &RoutedPacketSender,
    packet: RoutedPacket,
) -> Result<(), ()> {
    sender.send(packet).map_err(|_| ())
}

async fn udp_listener_loop(
    socket: UdpSocket,
    runtime_active: Arc<AtomicBool>,
    server_active: Arc<AtomicBool>,
    handler: UdpHandler,
) {
    let mut buffer = vec![0_u8; 65_536];
    while is_active(&runtime_active, &server_active) {
        let (len, peer_addr, returned_buffer) = match socket.recv_from_vec(buffer).await {
            Ok(result) => result,
            Err(_) => {
                if is_active(&runtime_active, &server_active) {
                    buffer = vec![0_u8; 65_536];
                    continue;
                }
                break;
            }
        };
        buffer = returned_buffer;
        if !is_active(&runtime_active, &server_active) {
            break;
        }
        let meta = PacketMeta {
            peer_addr: Some(peer_addr),
            local_addr: socket.local_addr().ok(),
        };
        if handler(socket.clone(), meta, buffer[..len].to_vec())
            .await
            .is_err()
        {
            break;
        }
    }
}

async fn simulated_udp_listener_loop(
    socket: UdpSocket,
    runtime_active: Arc<AtomicBool>,
    server_active: Arc<AtomicBool>,
    worker_executors: Vec<runtime::ExecutorHandle>,
    worker_count: usize,
    handler: UdpHandler,
) {
    let mut buffer = vec![0_u8; 65_536];
    while is_active(&runtime_active, &server_active) {
        let (len, peer_addr, returned_buffer) = match socket.recv_from_vec(buffer).await {
            Ok(result) => result,
            Err(_) => {
                if is_active(&runtime_active, &server_active) {
                    buffer = vec![0_u8; 65_536];
                    continue;
                }
                break;
            }
        };
        buffer = returned_buffer;
        if !is_active(&runtime_active, &server_active) {
            break;
        }
        let meta = PacketMeta {
            peer_addr: Some(peer_addr),
            local_addr: socket.local_addr().ok(),
        };
        let Ok(worker_id) = linux_reuseport_select(meta, worker_count) else {
            break;
        };
        let handler = Arc::clone(&handler);
        let socket = socket.clone();
        let payload = buffer[..len].to_vec();
        let Some(executor) = worker_executors.get(worker_id) else {
            break;
        };
        let submit_result = submit_udp_handler(executor, socket, meta, payload, handler).await;
        if submit_result.is_err() {
            break;
        }
    }
}

pub(crate) fn is_active(runtime_active: &AtomicBool, server_active: &AtomicBool) -> bool {
    runtime_active.load(Ordering::SeqCst) && server_active.load(Ordering::SeqCst)
}

fn current_time_seed() -> usize {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.subsec_nanos() as usize)
        .unwrap_or(0)
}

pub(crate) async fn submit_udp_handler(
    executor: &runtime::ExecutorHandle,
    socket: UdpSocket,
    meta: PacketMeta,
    payload: Vec<u8>,
    handler: UdpHandler,
) -> Result<(), Error> {
    let task_socket = socket.clone();
    let task_payload = payload.clone();
    let task_handler = Arc::clone(&handler);
    runtime::submit_or_run_local(
        executor,
        move || async move {
            let _ = task_handler(task_socket, meta, task_payload).await;
        },
        async move {
            let _ = handler(socket, meta, payload).await;
        },
    )
    .await
    .map_err(Error::from)
}
