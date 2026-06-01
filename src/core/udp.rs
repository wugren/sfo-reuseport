use std::collections::HashMap;
#[cfg(all(
    feature = "quinn",
    any(feature = "runtime-async-std", feature = "runtime-tokio")
))]
use std::future::Future;
use std::io;
#[cfg(feature = "quinn")]
use std::io::IoSliceMut;
use std::net::{Shutdown, SocketAddr, UdpSocket as StdUdpSocket};
#[cfg(feature = "quinn")]
use std::task::{Context, Poll};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::core::{
    ConcurrencyPermit, Error, HandlerFuture, HandlerFutureBox, ServerRuntime, UdpServiceConfig,
    WorkerConcurrencyLimit, linux_reuseport_select,
};
use crate::platform;
use crate::runtime;

pub(crate) type UdpHandler =
    Arc<dyn Fn(UdpSocket, PacketMeta, Vec<u8>) -> HandlerFutureBox + Send + Sync>;
pub(crate) type SocketCallback = Arc<dyn Fn(UdpSocket, usize) -> HandlerFutureBox + Send + Sync>;

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
type RoutedPacketSender = tokio::sync::mpsc::Sender<RoutedPacket>;
#[cfg(feature = "runtime-tokio")]
type RoutedPacketReceiver = tokio::sync::Mutex<tokio::sync::mpsc::Receiver<RoutedPacket>>;

#[cfg(feature = "runtime-tokio-uring")]
type RoutedPacketSender = std::sync::mpsc::SyncSender<RoutedPacket>;
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

    #[cfg(feature = "quinn")]
    pub fn try_send_to(&self, buffer: &[u8], target: SocketAddr) -> io::Result<usize> {
        match &self.inner {
            UdpSocketInner::Runtime(socket) => runtime::udp_try_send_to(socket, buffer, target),
            UdpSocketInner::Routed(socket) => socket.sender.send_to(buffer, target),
        }
    }

    #[cfg(feature = "quinn")]
    pub fn poll_send_ready(&self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &self.inner {
            UdpSocketInner::Runtime(socket) => runtime::udp_poll_send_ready(socket, cx),
            UdpSocketInner::Routed(_socket) => Poll::Ready(Ok(())),
        }
    }

    #[cfg(feature = "quinn")]
    pub fn poll_recv_from(
        &self,
        cx: &mut Context<'_>,
        buffer: &mut [u8],
    ) -> Poll<io::Result<(usize, SocketAddr)>> {
        match &self.inner {
            UdpSocketInner::Runtime(socket) => {
                runtime::udp_poll_recv_from_slice(socket, cx, buffer)
            }
            UdpSocketInner::Routed(socket) => socket.poll_recv_from_slice(cx, buffer),
        }
    }

    #[cfg(feature = "quinn")]
    pub fn poll_recv_from_vectored(
        &self,
        cx: &mut Context<'_>,
        buffers: &mut [IoSliceMut<'_>],
    ) -> Poll<io::Result<(usize, SocketAddr)>> {
        match &self.inner {
            UdpSocketInner::Runtime(socket) => {
                runtime::udp_poll_recv_from_vectored(socket, cx, buffers)
            }
            UdpSocketInner::Routed(socket) => socket.poll_recv_from_vectored(cx, buffers),
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

    #[cfg(feature = "quinn")]
    fn poll_recv_from_slice(
        &self,
        cx: &mut Context<'_>,
        buffer: &mut [u8],
    ) -> Poll<io::Result<(usize, SocketAddr)>> {
        let packet = match self.poll_recv_packet(cx) {
            Poll::Ready(result) => result,
            Poll::Pending => return Poll::Pending,
        };
        match packet {
            Ok(packet) => {
                let len = packet.payload.len().min(buffer.len());
                buffer[..len].copy_from_slice(&packet.payload[..len]);
                Poll::Ready(Ok((len, packet.peer_addr)))
            }
            Err(error) => Poll::Ready(Err(error)),
        }
    }

    #[cfg(feature = "quinn")]
    fn poll_recv_from_vectored(
        &self,
        cx: &mut Context<'_>,
        buffers: &mut [IoSliceMut<'_>],
    ) -> Poll<io::Result<(usize, SocketAddr)>> {
        let packet = match self.poll_recv_packet(cx) {
            Poll::Ready(result) => result,
            Poll::Pending => return Poll::Pending,
        };
        match packet {
            Ok(packet) => {
                scatter_datagram(&packet.payload, buffers);
                Poll::Ready(Ok((packet.payload.len(), packet.peer_addr)))
            }
            Err(error) => Poll::Ready(Err(error)),
        }
    }

    #[cfg(feature = "quinn")]
    fn poll_recv_packet(&self, cx: &mut Context<'_>) -> Poll<io::Result<RoutedPacket>> {
        #[cfg(feature = "runtime-async-std")]
        {
            let mut future = Box::pin(self.receiver.recv());
            return match future.as_mut().poll(cx) {
                Poll::Ready(Ok(packet)) => Poll::Ready(Ok(packet)),
                Poll::Ready(Err(_)) => Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "routed UDP socket closed",
                ))),
                Poll::Pending => Poll::Pending,
            };
        }
        #[cfg(feature = "runtime-tokio")]
        {
            let mut lock = Box::pin(self.receiver.lock());
            let mut receiver = match lock.as_mut().poll(cx) {
                Poll::Ready(receiver) => receiver,
                Poll::Pending => return Poll::Pending,
            };
            let mut recv = Box::pin(receiver.recv());
            return match recv.as_mut().poll(cx) {
                Poll::Ready(Some(packet)) => Poll::Ready(Ok(packet)),
                Poll::Ready(None) => Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "routed UDP socket closed",
                ))),
                Poll::Pending => Poll::Pending,
            };
        }
        #[cfg(feature = "runtime-tokio-uring")]
        {
            let _ = cx;
            let receiver = self
                .receiver
                .lock()
                .map_err(|_| io::Error::other("routed UDP receiver lock poisoned"))?;
            return match receiver.try_recv() {
                Ok(packet) => Poll::Ready(Ok(packet)),
                Err(std::sync::mpsc::TryRecvError::Empty) => Poll::Pending,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Poll::Ready(Err(
                    io::Error::new(io::ErrorKind::UnexpectedEof, "routed UDP socket closed"),
                )),
            };
        }
    }
}

#[cfg(feature = "quinn")]
fn scatter_datagram(payload: &[u8], buffers: &mut [IoSliceMut<'_>]) {
    let mut offset = 0;
    for buffer in buffers {
        if offset >= payload.len() {
            break;
        }
        let copy_len = (payload.len() - offset).min(buffer.len());
        buffer[..copy_len].copy_from_slice(&payload[offset..offset + copy_len]);
        offset += copy_len;
    }
}

#[derive(Clone)]
pub struct UdpServer {
    state: Arc<UdpServerState>,
}

pub(crate) struct UdpServerState {
    active: Arc<AtomicBool>,
    tasks: Mutex<Vec<runtime::TaskHandle>>,
    callback_tasks: Mutex<Vec<runtime::TaskHandle>>,
    sockets: Mutex<Vec<StdUdpSocket>>,
    close_sockets: Mutex<Vec<StdUdpSocket>>,
    thread_sockets: Mutex<HashMap<thread::ThreadId, StdUdpSocket>>,
    next_socket: AtomicUsize,
}

impl UdpServer {
    pub fn serve<F, Fut>(
        runtime: &ServerRuntime,
        config: UdpServiceConfig,
        handler: F,
    ) -> Result<Self, Error>
    where
        F: Fn(UdpSocket, PacketMeta, Vec<u8>) -> Fut + Clone + Send + Sync + 'static,
        Fut: HandlerFuture,
    {
        config.validate()?;
        config.validate_routed_packet_channel_capacity()?;
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
        config: UdpServiceConfig,
        callback: F,
    ) -> Result<Self, Error>
    where
        F: Fn(UdpSocket, usize) -> Fut + Clone + Send + Sync + 'static,
        Fut: HandlerFuture,
    {
        config.validate()?;
        config.validate_routed_packet_channel_capacity()?;
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
            callback_tasks: Mutex::new(Vec::new()),
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
        if let Ok(mut tasks) = self.callback_tasks.lock() {
            tasks.clear();
        }
    }

    pub(crate) fn register_task(&self, task: runtime::TaskHandle) -> Result<(), Error> {
        self.tasks
            .lock()
            .map_err(|_| Error::Runtime("udp task registry lock poisoned".to_string()))?
            .push(task);
        Ok(())
    }

    pub(crate) fn register_callback_task(&self, task: runtime::TaskHandle) -> Result<(), Error> {
        self.callback_tasks
            .lock()
            .map_err(|_| Error::Runtime("udp callback task registry lock poisoned".to_string()))?
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
    service_config: UdpServiceConfig,
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
    let max_concurrency = service_config.max_concurrency_per_worker;

    let handler = udp_handler(handler);
    for (worker_id, socket) in sockets.into_iter().enumerate() {
        let runtime_active = Arc::clone(&runtime_active);
        let server_active = state.active_flag();
        let task_state = Arc::clone(&state);
        let handler = Arc::clone(&handler);
        let limit = WorkerConcurrencyLimit::new(max_concurrency);
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
            udp_listener_loop(socket, runtime_active, server_active, handler, limit).await;
        })?;
        state.register_task(task)?;
    }

    Ok(())
}

fn add_socket_callback_reuse_port_listener<F, Fut>(
    runtime: &ServerRuntime,
    service_config: UdpServiceConfig,
    callback: F,
    state: Arc<UdpServerState>,
) -> Result<(), Error>
where
    F: Fn(UdpSocket, usize) -> Fut + Clone + Send + Sync + 'static,
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
            let _ = callback(socket, worker_id).await;
        })?;
        state.register_callback_task(task)?;
    }

    Ok(())
}

fn add_simulated_listener<F, Fut>(
    runtime: &ServerRuntime,
    service_config: UdpServiceConfig,
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
    let limits = worker_limits(worker_count, service_config.max_concurrency_per_worker);
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
            limits,
        )
        .await;
    })?;
    state.register_task(task)?;

    Ok(())
}

pub(crate) fn add_socket_callback_simulated_listener<F, Fut, R>(
    runtime: &ServerRuntime,
    service_config: UdpServiceConfig,
    callback: F,
    state: Arc<UdpServerState>,
    route_packet: R,
) -> Result<(), Error>
where
    F: Fn(UdpSocket, usize) -> Fut + Clone + Send + Sync + 'static,
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

        let routed_packet_channel_capacity = service_config.routed_packet_channel_capacity();
        let mut senders = Vec::with_capacity(worker_count);
        let callback = socket_callback(callback);
        for worker_id in 0..worker_count {
            let (sender, receiver) = routed_packet_channel(routed_packet_channel_capacity);
            senders.push(sender);
            let routed_socket = UdpSocket::from_routed(RoutedUdpSocket {
                sender: socket.try_clone().map_err(Error::from)?,
                receiver,
                local_addr,
            });
            let callback = Arc::clone(&callback);
            let task = runtime.submit_to_worker(worker_id, move || async move {
                let _ = callback(routed_socket, worker_id).await;
            })?;
            state.register_callback_task(task)?;
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
    F: Fn(UdpSocket, usize) -> Fut + Clone + Send + Sync + 'static,
    Fut: HandlerFuture,
{
    Arc::new(move |socket, worker_id| {
        let future = callback.clone()(socket, worker_id);
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
fn routed_packet_channel(capacity: usize) -> (RoutedPacketSender, RoutedPacketReceiver) {
    async_std::channel::bounded(capacity)
}

#[cfg(feature = "runtime-tokio")]
fn routed_packet_channel(capacity: usize) -> (RoutedPacketSender, RoutedPacketReceiver) {
    let (sender, receiver) = tokio::sync::mpsc::channel(capacity);
    (sender, tokio::sync::Mutex::new(receiver))
}

#[cfg(feature = "runtime-tokio-uring")]
#[allow(dead_code)]
fn routed_packet_channel(capacity: usize) -> (RoutedPacketSender, RoutedPacketReceiver) {
    let (sender, receiver) = std::sync::mpsc::sync_channel(capacity);
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
    sender.send(packet).await.map_err(|_| ())
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
    limit: WorkerConcurrencyLimit,
) {
    let mut buffer = vec![0_u8; 65_536];
    while is_active(&runtime_active, &server_active) {
        let permit = limit.acquire().await;
        if !is_active(&runtime_active, &server_active) {
            break;
        }
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
        if spawn_udp_handler(
            socket.clone(),
            meta,
            buffer[..len].to_vec(),
            Arc::clone(&handler),
            permit,
        )
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
    limits: Vec<WorkerConcurrencyLimit>,
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
        let Some(limit) = limits.get(worker_id) else {
            break;
        };
        let Some(permit) = limit.try_acquire() else {
            continue;
        };
        let submit_result = submit_udp_handler(executor, socket, meta, payload, handler, permit).await;
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
    permit: ConcurrencyPermit,
) -> Result<(), Error> {
    #[cfg(any(feature = "runtime-tokio", feature = "runtime-async-std"))]
    {
        runtime::submit_or_run_local(
            executor,
            move || async move {
                let _permit = permit;
                let _ = handler(socket, meta, payload).await;
            },
            async {},
        )
        .await
        .map_err(Error::from)
    }

    #[cfg(feature = "runtime-tokio-uring")]
    {
        let _ = executor;
        let _permit = permit;
        let _ = handler(socket, meta, payload).await;
        Ok(())
    }
}

fn worker_limits(worker_count: usize, max: Option<usize>) -> Vec<WorkerConcurrencyLimit> {
    (0..worker_count)
        .map(|_| WorkerConcurrencyLimit::new(max))
        .collect()
}

pub(crate) fn spawn_udp_handler(
    socket: UdpSocket,
    meta: PacketMeta,
    payload: Vec<u8>,
    handler: UdpHandler,
    permit: ConcurrencyPermit,
) -> Result<(), Error> {
    runtime::spawn_local(async move {
        let _permit = permit;
        let _ = handler(socket, meta, payload).await;
    })
    .map(|_| ())
    .map_err(Error::from)
}

#[cfg(all(test, feature = "quinn", feature = "runtime-tokio"))]
mod quinn_tests {
    use super::*;
    use std::future::poll_fn;

    #[tokio::test]
    async fn routed_socket_quinn_poll_recv_reads_routed_packet() {
        let (sender, receiver) =
            routed_packet_channel(crate::core::DEFAULT_ROUTED_PACKET_CHANNEL_CAPACITY);
        let sender_socket = StdUdpSocket::bind("127.0.0.1:0").unwrap();
        let local_addr = sender_socket.local_addr().unwrap();
        let peer_addr: SocketAddr = "127.0.0.1:12345".parse().unwrap();
        let socket = UdpSocket::from_routed(RoutedUdpSocket {
            sender: sender_socket,
            receiver,
            local_addr,
        });

        send_routed_packet(
            &sender,
            RoutedPacket {
                payload: b"routed-quinn".to_vec(),
                peer_addr,
            },
        )
        .await
        .unwrap();

        let mut buffer = [0_u8; 32];
        let (len, source) = poll_fn(|cx| socket.poll_recv_from(cx, &mut buffer))
            .await
            .unwrap();

        assert_eq!(&buffer[..len], b"routed-quinn");
        assert_eq!(source, peer_addr);
    }

    #[tokio::test]
    async fn routed_socket_quinn_poll_recv_vectored_scatters_datagram() {
        let (sender, receiver) =
            routed_packet_channel(crate::core::DEFAULT_ROUTED_PACKET_CHANNEL_CAPACITY);
        let sender_socket = StdUdpSocket::bind("127.0.0.1:0").unwrap();
        let local_addr = sender_socket.local_addr().unwrap();
        let peer_addr: SocketAddr = "127.0.0.1:12346".parse().unwrap();
        let socket = UdpSocket::from_routed(RoutedUdpSocket {
            sender: sender_socket,
            receiver,
            local_addr,
        });

        send_routed_packet(
            &sender,
            RoutedPacket {
                payload: b"abcdef".to_vec(),
                peer_addr,
            },
        )
        .await
        .unwrap();

        let mut first = [0_u8; 2];
        let mut second = [0_u8; 4];
        let mut buffers = [IoSliceMut::new(&mut first), IoSliceMut::new(&mut second)];
        let (len, source) = poll_fn(|cx| socket.poll_recv_from_vectored(cx, &mut buffers))
            .await
            .unwrap();

        assert_eq!(len, 6);
        assert_eq!(source, peer_addr);
        assert_eq!(&first, b"ab");
        assert_eq!(&second, b"cdef");
    }
}
