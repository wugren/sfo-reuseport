use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::core::{
    Error, HandlerFuture, HandlerFutureBox, ListenerConfig, ListenerId, ListenerProtocol,
    ServerRuntime, ServiceConfig, linux_reuseport_select,
};
use crate::platform;
use crate::runtime::{self, UdpSocket};

type UdpHandler = Arc<dyn Fn(UdpSocket, PacketMeta, Vec<u8>) -> HandlerFutureBox + Send + Sync>;

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct PacketMeta {
    pub peer_addr: Option<SocketAddr>,
    pub local_addr: Option<SocketAddr>,
}

pub struct UdpServer;

impl UdpServer {
    pub fn serve<F, Fut>(
        runtime: &ServerRuntime,
        config: ServiceConfig,
        handler: F,
    ) -> Result<(), Error>
    where
        F: Fn(UdpSocket, PacketMeta, Vec<u8>) -> Fut + Clone + Send + Sync + 'static,
        Fut: HandlerFuture,
    {
        config.validate()?;
        if !platform::supports_reuse_port_balancing() {
            add_simulated_listener(runtime, config, handler)?;
        } else {
            add_reuse_port_listener(runtime, config, handler)?;
        }
        Ok(())
    }
}

impl ServerRuntime {
    pub fn add_udp_listener<F, Fut>(
        &self,
        config: ListenerConfig,
        handler: F,
    ) -> Result<ListenerId, Error>
    where
        F: Fn(UdpSocket, PacketMeta, Vec<u8>) -> Fut + Clone + Send + Sync + 'static,
        Fut: HandlerFuture,
    {
        add_udp_listener(self, config, handler)
    }
}

pub(crate) fn add_udp_listener<F, Fut>(
    runtime: &ServerRuntime,
    config: ListenerConfig,
    handler: F,
) -> Result<ListenerId, Error>
where
    F: Fn(UdpSocket, PacketMeta, Vec<u8>) -> Fut + Clone + Send + Sync + 'static,
    Fut: HandlerFuture,
{
    let service_config = runtime.service_config(config);
    service_config.validate()?;
    if !platform::supports_reuse_port_balancing() {
        return add_simulated_listener(runtime, service_config, handler);
    }

    add_reuse_port_listener(runtime, service_config, handler)
}

fn add_reuse_port_listener<F, Fut>(
    runtime: &ServerRuntime,
    service_config: ServiceConfig,
    handler: F,
) -> Result<ListenerId, Error>
where
    F: Fn(UdpSocket, PacketMeta, Vec<u8>) -> Fut + Clone + Send + Sync + 'static,
    Fut: HandlerFuture,
{
    let sockets = platform::bind_udp_workers(&service_config, runtime.worker_count())?;
    let addr = sockets
        .first()
        .ok_or_else(|| Error::InvalidConfig("worker count must be greater than zero".to_string()))?
        .local_addr()
        .map_err(Error::from)?;
    let active = Arc::new(AtomicBool::new(true));
    let id = runtime.register_listener(ListenerProtocol::Udp, addr, Arc::clone(&active))?;

    let handler = udp_handler(handler);
    for (worker_id, socket) in sockets.into_iter().enumerate() {
        let active = Arc::clone(&active);
        let handler = Arc::clone(&handler);
        runtime.submit_to_worker(worker_id, move || async move {
            let Ok(socket) = runtime::udp_socket_from_std(socket).map_err(Error::from) else {
                return;
            };
            udp_listener_loop(socket, active, handler).await;
        })?;
    }

    Ok(id)
}

fn add_simulated_listener<F, Fut>(
    runtime: &ServerRuntime,
    service_config: ServiceConfig,
    handler: F,
) -> Result<ListenerId, Error>
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
    let addr = socket.local_addr().map_err(Error::from)?;
    let active = Arc::new(AtomicBool::new(true));
    let id = runtime.register_listener(ListenerProtocol::Udp, addr, Arc::clone(&active))?;

    let server_runtime = runtime.clone();
    let worker_count = server_runtime.worker_count();
    let handler = udp_handler(handler);
    runtime.submit_to_worker(0, move || async move {
        let Ok(socket) = runtime::udp_socket_from_std(socket).map_err(Error::from) else {
            return;
        };
        simulated_udp_listener_loop(
            socket,
            active,
            server_runtime,
            worker_count,
            handler,
        )
        .await;
    })?;

    Ok(id)
}

pub struct QuicServer;

impl QuicServer {
    pub fn serve<F, Fut>(
        runtime: &ServerRuntime,
        config: ServiceConfig,
        handler: F,
    ) -> Result<(), Error>
    where
        F: Fn(UdpSocket, PacketMeta, Vec<u8>) -> Fut + Clone + Send + Sync + 'static,
        Fut: HandlerFuture,
    {
        config.validate()?;
        add_quic_routed_listener(runtime, config, handler)?;
        Ok(())
    }
}

impl ServerRuntime {
    pub fn add_quic_listener<F, Fut>(
        &self,
        config: ListenerConfig,
        handler: F,
    ) -> Result<ListenerId, Error>
    where
        F: Fn(UdpSocket, PacketMeta, Vec<u8>) -> Fut + Clone + Send + Sync + 'static,
        Fut: HandlerFuture,
    {
        add_quic_listener(self, config, handler)
    }
}

pub(crate) fn add_quic_listener<F, Fut>(
    runtime: &ServerRuntime,
    config: ListenerConfig,
    handler: F,
) -> Result<ListenerId, Error>
where
    F: Fn(UdpSocket, PacketMeta, Vec<u8>) -> Fut + Clone + Send + Sync + 'static,
    Fut: HandlerFuture,
{
    let service_config = runtime.service_config(config);
    service_config.validate()?;
    add_quic_routed_listener(runtime, service_config, handler)
}

fn add_quic_routed_listener<F, Fut>(
    runtime: &ServerRuntime,
    service_config: ServiceConfig,
    handler: F,
) -> Result<ListenerId, Error>
where
    F: Fn(UdpSocket, PacketMeta, Vec<u8>) -> Fut + Clone + Send + Sync + 'static,
    Fut: HandlerFuture,
{
    if platform::supports_reuse_port_balancing() {
        return add_quic_reuseport_listener(runtime, service_config, handler);
    }

    add_quic_simulated_listener(runtime, service_config, handler)
}

fn add_quic_reuseport_listener<F, Fut>(
    runtime: &ServerRuntime,
    service_config: ServiceConfig,
    handler: F,
) -> Result<ListenerId, Error>
where
    F: Fn(UdpSocket, PacketMeta, Vec<u8>) -> Fut + Clone + Send + Sync + 'static,
    Fut: HandlerFuture,
{
    if let Some(listener) = add_quic_reuseport_bpf_listener(runtime, &service_config, handler.clone())? {
        return Ok(listener);
    }

    add_quic_simulated_listener(runtime, service_config, handler)
}

fn add_quic_simulated_listener<F, Fut>(
    runtime: &ServerRuntime,
    service_config: ServiceConfig,
    handler: F,
) -> Result<ListenerId, Error>
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
    let addr = socket.local_addr().map_err(Error::from)?;
    let active = Arc::new(AtomicBool::new(true));
    let id = runtime.register_listener(ListenerProtocol::Udp, addr, Arc::clone(&active))?;

    let server_runtime = runtime.clone();
    let worker_count = server_runtime.worker_count();
    let handler = udp_handler(handler);
    runtime.submit_to_worker(0, move || async move {
        let Ok(socket) = runtime::udp_socket_from_std(socket).map_err(Error::from) else {
            return;
        };
        quic_routed_udp_listener_loop(socket, active, server_runtime, worker_count, handler).await;
    })?;

    Ok(id)
}

fn add_quic_reuseport_bpf_listener<F, Fut>(
    runtime: &ServerRuntime,
    service_config: &ServiceConfig,
    handler: F,
) -> Result<Option<ListenerId>, Error>
where
    F: Fn(UdpSocket, PacketMeta, Vec<u8>) -> Fut + Clone + Send + Sync + 'static,
    Fut: HandlerFuture,
{
    let Some(sockets) =
        platform::bind_quic_udp_reuseport_workers(service_config, runtime.worker_count())?
    else {
        return Ok(None);
    };
    let addr = sockets
        .first()
        .ok_or_else(|| Error::InvalidConfig("worker count must be greater than zero".to_string()))?
        .local_addr()
        .map_err(Error::from)?;
    let active = Arc::new(AtomicBool::new(true));
    let id = runtime.register_listener(ListenerProtocol::Udp, addr, Arc::clone(&active))?;

    let handler = udp_handler(handler);
    for (worker_id, socket) in sockets.into_iter().enumerate() {
        let active = Arc::clone(&active);
        let handler = Arc::clone(&handler);
        let worker_count = runtime.worker_count();
        runtime.submit_to_worker(worker_id, move || async move {
            let Ok(socket) = runtime::udp_socket_from_std(socket).map_err(Error::from) else {
                return;
            };
            quic_reuseport_bpf_listener_loop(socket, active, worker_count, handler).await;
        })?;
    }

    Ok(Some(id))
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
    active: Arc<AtomicBool>,
    runtime: ServerRuntime,
    worker_count: usize,
    handler: UdpHandler,
) {
    let mut buffer = vec![0_u8; 65_536];
    while active.load(Ordering::SeqCst) {
        let Ok((len, peer_addr)) = runtime::udp_recv_from(&socket, &mut buffer).await else {
            if active.load(Ordering::SeqCst) {
                continue;
            }
            break;
        };
        if !active.load(Ordering::SeqCst) {
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
        let submit_result =
            submit_udp_handler(&runtime, worker_id, socket, meta, payload, handler).await;
        if submit_result.is_err() {
            break;
        }
    }
}

async fn quic_reuseport_bpf_listener_loop(
    socket: UdpSocket,
    active: Arc<AtomicBool>,
    worker_count: usize,
    handler: UdpHandler,
) {
    let mut buffer = vec![0_u8; 65_536];
    while active.load(Ordering::SeqCst) {
        let Ok((len, peer_addr)) = runtime::udp_recv_from(&socket, &mut buffer).await else {
            if active.load(Ordering::SeqCst) {
                continue;
            }
            break;
        };
        if !active.load(Ordering::SeqCst) {
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
    active: Arc<AtomicBool>,
    handler: UdpHandler,
) {
    let mut buffer = vec![0_u8; 65_536];
    while active.load(Ordering::SeqCst) {
        let Ok((len, peer_addr)) = runtime::udp_recv_from(&socket, &mut buffer).await else {
            if active.load(Ordering::SeqCst) {
                continue;
            }
            break;
        };
        if !active.load(Ordering::SeqCst) {
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
    active: Arc<AtomicBool>,
    runtime: ServerRuntime,
    worker_count: usize,
    handler: UdpHandler,
) {
    let mut buffer = vec![0_u8; 65_536];
    while active.load(Ordering::SeqCst) {
        let Ok((len, peer_addr)) = runtime::udp_recv_from(&socket, &mut buffer).await else {
            if active.load(Ordering::SeqCst) {
                continue;
            }
            break;
        };
        if !active.load(Ordering::SeqCst) {
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
        let submit_result =
            submit_udp_handler(&runtime, worker_id, socket, meta, payload, handler).await;
        if submit_result.is_err() {
            break;
        }
    }
}

async fn submit_udp_handler(
    runtime: &ServerRuntime,
    worker_id: usize,
    socket: UdpSocket,
    meta: PacketMeta,
    payload: Vec<u8>,
    handler: UdpHandler,
) -> Result<(), Error> {
    let executor = runtime.worker_executor(worker_id)?;
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
