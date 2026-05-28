use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::core::udp::{
    PacketMeta, UdpHandler, UdpServerState, UdpSocket, add_socket_callback_simulated_listener,
    is_active, socket_callback, submit_udp_handler, udp_handler,
};
use crate::core::{Error, HandlerFuture, ServerRuntime, ServiceConfig};
use crate::platform;
use crate::runtime;

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
        add_quic_socket_callback_listener(runtime, config, callback, Arc::clone(&server.state))?;
        Ok(server)
    }
}

fn add_quic_socket_callback_listener<F, Fut>(
    runtime: &ServerRuntime,
    service_config: ServiceConfig,
    callback: F,
    state: Arc<UdpServerState>,
) -> Result<(), Error>
where
    F: Fn(UdpSocket) -> Fut + Clone + Send + Sync + 'static,
    Fut: HandlerFuture,
{
    if platform::supports_reuse_port_balancing() {
        if add_quic_socket_callback_reuseport_bpf_listener(
            runtime,
            &service_config,
            callback.clone(),
            Arc::clone(&state),
        )? {
            return Ok(());
        }
    }

    add_socket_callback_simulated_listener(
        runtime,
        service_config,
        callback,
        state,
        |packet, _meta, worker_count| quic_worker_index(packet, worker_count),
    )
}

fn add_quic_socket_callback_reuseport_bpf_listener<F, Fut>(
    runtime: &ServerRuntime,
    service_config: &ServiceConfig,
    callback: F,
    state: Arc<UdpServerState>,
) -> Result<bool, Error>
where
    F: Fn(UdpSocket) -> Fut + Clone + Send + Sync + 'static,
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

    Ok(true)
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
        let Ok(socket) = runtime::udp_socket_from_std(socket)
            .map(UdpSocket::from_runtime)
            .map_err(Error::from)
        else {
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
            let Ok(socket) = runtime::udp_socket_from_std(socket)
                .map(UdpSocket::from_runtime)
                .map_err(Error::from)
            else {
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
