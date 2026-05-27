use std::collections::HashMap;
use std::net::{Shutdown, SocketAddr, UdpSocket as StdUdpSocket};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::core::{
    Error, HandlerFuture, HandlerFutureBox, ServerRuntime, ServiceConfig, linux_reuseport_select,
};
use crate::platform;
use crate::runtime::{self, UdpSocket};

type UdpHandler = Arc<dyn Fn(UdpSocket, PacketMeta, Vec<u8>) -> HandlerFutureBox + Send + Sync>;

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct PacketMeta {
    pub peer_addr: Option<SocketAddr>,
    pub local_addr: Option<SocketAddr>,
}

#[derive(Clone)]
pub struct UdpServer {
    state: Arc<UdpServerState>,
}

struct UdpServerState {
    active: Arc<AtomicBool>,
    tasks: Mutex<Vec<runtime::TaskHandle>>,
    sockets: Mutex<Vec<StdUdpSocket>>,
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
}

impl UdpServerState {
    fn new() -> Self {
        Self {
            active: Arc::new(AtomicBool::new(true)),
            tasks: Mutex::new(Vec::new()),
            sockets: Mutex::new(Vec::new()),
            thread_sockets: Mutex::new(HashMap::new()),
            next_socket: AtomicUsize::new(0),
        }
    }

    fn close(&self) {
        self.active.store(false, Ordering::SeqCst);
        if let Ok(mut sockets) = self.sockets.lock() {
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

    fn register_task(&self, task: runtime::TaskHandle) -> Result<(), Error> {
        self.tasks
            .lock()
            .map_err(|_| Error::Runtime("udp task registry lock poisoned".to_string()))?
            .push(task);
        Ok(())
    }

    fn active_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.active)
    }

    fn register_listener_socket(&self, socket: &StdUdpSocket) -> Result<(), Error> {
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

    fn listener_socket(&self) -> Result<UdpSocket, Error> {
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
            return runtime::udp_socket_from_std(socket).map_err(Error::from);
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
        runtime::udp_socket_from_std(socket).map_err(Error::from)
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
            let Ok(socket) = runtime::udp_socket_from_std(socket).map_err(Error::from) else {
                return;
            };
            udp_listener_loop(socket, runtime_active, server_active, handler).await;
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
        let Ok(socket) = runtime::udp_socket_from_std(socket).map_err(Error::from) else {
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

#[derive(Clone)]
pub struct QuicServer {
    state: Arc<UdpServerState>,
}

impl QuicServer {
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
        add_quic_routed_listener(runtime, config, handler, Arc::clone(&server.state))?;
        Ok(server)
    }

    pub fn close(&self) -> Result<(), Error> {
        self.state.close();
        Ok(())
    }

    pub fn listener_socket(&self) -> Result<UdpSocket, Error> {
        self.state.listener_socket()
    }
}

fn add_quic_routed_listener<F, Fut>(
    runtime: &ServerRuntime,
    service_config: ServiceConfig,
    handler: F,
    state: Arc<UdpServerState>,
) -> Result<(), Error>
where
    F: Fn(UdpSocket, PacketMeta, Vec<u8>) -> Fut + Clone + Send + Sync + 'static,
    Fut: HandlerFuture,
{
    if platform::supports_reuse_port_balancing() {
        return add_quic_reuseport_listener(runtime, service_config, handler, state);
    }

    add_quic_simulated_listener(runtime, service_config, handler, state)
}

fn add_quic_reuseport_listener<F, Fut>(
    runtime: &ServerRuntime,
    service_config: ServiceConfig,
    handler: F,
    state: Arc<UdpServerState>,
) -> Result<(), Error>
where
    F: Fn(UdpSocket, PacketMeta, Vec<u8>) -> Fut + Clone + Send + Sync + 'static,
    Fut: HandlerFuture,
{
    if add_quic_reuseport_bpf_listener(
        runtime,
        &service_config,
        handler.clone(),
        Arc::clone(&state),
    )? {
        return Ok(());
    }

    add_quic_simulated_listener(runtime, service_config, handler, state)
}

fn add_quic_simulated_listener<F, Fut>(
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
        let Ok(socket) = runtime::udp_socket_from_std(socket).map_err(Error::from) else {
            return;
        };
        quic_routed_udp_listener_loop(
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

fn add_quic_reuseport_bpf_listener<F, Fut>(
    runtime: &ServerRuntime,
    service_config: &ServiceConfig,
    handler: F,
    state: Arc<UdpServerState>,
) -> Result<bool, Error>
where
    F: Fn(UdpSocket, PacketMeta, Vec<u8>) -> Fut + Clone + Send + Sync + 'static,
    Fut: HandlerFuture,
{
    let Some(sockets) =
        platform::bind_quic_udp_reuseport_workers(service_config, runtime.worker_count())?
    else {
        return Ok(false);
    };
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
        let worker_count = runtime.worker_count();
        let task = runtime.submit_to_worker(worker_id, move || async move {
            if task_state.register_listener_socket(&socket).is_err() {
                return;
            }
            let Ok(socket) = runtime::udp_socket_from_std(socket).map_err(Error::from) else {
                return;
            };
            quic_reuseport_bpf_listener_loop(
                socket,
                runtime_active,
                server_active,
                worker_count,
                handler,
            )
            .await;
        })?;
        state.register_task(task)?;
    }

    Ok(true)
}

fn udp_handler<F, Fut>(handler: F) -> UdpHandler
where
    F: Fn(UdpSocket, PacketMeta, Vec<u8>) -> Fut + Clone + Send + Sync + 'static,
    Fut: HandlerFuture,
{
    Arc::new(move |socket, meta, payload| {
        let future = handler.clone()(socket, meta, payload);
        Box::pin(future) as HandlerFutureBox
    })
}

async fn quic_routed_udp_listener_loop(
    socket: UdpSocket,
    runtime_active: Arc<AtomicBool>,
    server_active: Arc<AtomicBool>,
    worker_executors: Vec<runtime::ExecutorHandle>,
    worker_count: usize,
    handler: UdpHandler,
) {
    let mut buffer = vec![0_u8; 65_536];
    while is_active(&runtime_active, &server_active) {
        let Ok((len, peer_addr)) = runtime::udp_recv_from(&socket, &mut buffer).await else {
            if is_active(&runtime_active, &server_active) {
                continue;
            }
            break;
        };
        if !is_active(&runtime_active, &server_active) {
            break;
        }
        let payload = &buffer[..len];
        let Some(worker_id) = quic_worker_index(payload, worker_count) else {
            continue;
        };
        let meta = PacketMeta {
            peer_addr: Some(peer_addr),
            local_addr: socket.local_addr().ok(),
        };
        let handler = Arc::clone(&handler);
        let socket = socket.clone();
        let payload = payload.to_vec();
        let Some(executor) = worker_executors.get(worker_id) else {
            break;
        };
        let submit_result = submit_udp_handler(executor, socket, meta, payload, handler).await;
        if submit_result.is_err() {
            break;
        }
    }
}

async fn quic_reuseport_bpf_listener_loop(
    socket: UdpSocket,
    runtime_active: Arc<AtomicBool>,
    server_active: Arc<AtomicBool>,
    worker_count: usize,
    handler: UdpHandler,
) {
    let mut buffer = vec![0_u8; 65_536];
    while is_active(&runtime_active, &server_active) {
        let Ok((len, peer_addr)) = runtime::udp_recv_from(&socket, &mut buffer).await else {
            if is_active(&runtime_active, &server_active) {
                continue;
            }
            break;
        };
        if !is_active(&runtime_active, &server_active) {
            break;
        }
        let payload = &buffer[..len];
        if !quic_reuseport_bpf_accepts_packet(payload, worker_count) {
            continue;
        }
        let meta = PacketMeta {
            peer_addr: Some(peer_addr),
            local_addr: socket.local_addr().ok(),
        };
        if handler(socket.clone(), meta, payload.to_vec()).await.is_err() {
            break;
        }
    }
}

fn quic_worker_index(packet: &[u8], workers: usize) -> Option<usize> {
    if workers == 0 || packet.is_empty() {
        return None;
    }

    let shard = if packet[0] & 0x80 != 0 {
        quic_worker_shard(quic_long_header_dcid(packet)?)?
    } else {
        quic_worker_shard(packet.get(1..)?)?
    };

    Some(usize::from(shard) % workers)
}

fn quic_reuseport_bpf_accepts_packet(packet: &[u8], workers: usize) -> bool {
    quic_worker_index(packet, workers).is_some()
}

fn quic_long_header_dcid(packet: &[u8]) -> Option<&[u8]> {
    let dcid_len = usize::from(*packet.get(5)?);
    if dcid_len == 0 {
        return None;
    }
    let start = 6;
    let end = start + dcid_len;
    if end > packet.len() {
        return None;
    }
    Some(&packet[start..end])
}

fn quic_worker_shard(bytes: &[u8]) -> Option<u16> {
    let shard = bytes.get(..2)?;
    Some(u16::from_be_bytes([shard[0], shard[1]]))
}

async fn udp_listener_loop(
    socket: UdpSocket,
    runtime_active: Arc<AtomicBool>,
    server_active: Arc<AtomicBool>,
    handler: UdpHandler,
) {
    let mut buffer = vec![0_u8; 65_536];
    while is_active(&runtime_active, &server_active) {
        let Ok((len, peer_addr)) = runtime::udp_recv_from(&socket, &mut buffer).await else {
            if is_active(&runtime_active, &server_active) {
                continue;
            }
            break;
        };
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
        let Ok((len, peer_addr)) = runtime::udp_recv_from(&socket, &mut buffer).await else {
            if is_active(&runtime_active, &server_active) {
                continue;
            }
            break;
        };
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

fn is_active(runtime_active: &AtomicBool, server_active: &AtomicBool) -> bool {
    runtime_active.load(Ordering::SeqCst) && server_active.load(Ordering::SeqCst)
}

fn current_time_seed() -> usize {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.subsec_nanos() as usize)
        .unwrap_or(0)
}

async fn submit_udp_handler(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quic_long_header_uses_first_two_dcid_bytes_as_worker_shard() {
        let packet = [0xc0, 0, 0, 0, 1, 4, 0x01, 0x02, 9, 9];
        assert_eq!(quic_worker_index(&packet, 4), Some(2));
    }

    #[test]
    fn quic_short_header_uses_first_two_bytes_after_header_as_worker_shard() {
        let packet = [0x40, 0x01, 0x03, 2, 3];
        assert_eq!(quic_worker_index(&packet, 4), Some(3));
    }

    #[test]
    fn quic_route_key_rejects_empty_or_truncated_dcid() {
        assert_eq!(quic_worker_index(&[], 4), None);
        assert_eq!(quic_worker_index(&[0xc0, 0, 0, 0, 1, 0], 4), None);
        assert_eq!(quic_worker_index(&[0xc0, 0, 0, 0, 1, 1, 1], 4), None);
        assert_eq!(quic_worker_index(&[0xc0, 0, 0, 0, 1, 4, 1], 4), None);
        assert_eq!(quic_worker_index(&[0x40, 1], 4), None);
    }

    #[test]
    fn quic_reuseport_bpf_path_trusts_kernel_worker_selection() {
        let packet = [0xc0, 0, 0, 0, 1, 4, 0, 2, 9, 9];

        assert_eq!(quic_worker_index(&packet, 4), Some(2));
        assert!(quic_reuseport_bpf_accepts_packet(&packet, 4));
    }
}
