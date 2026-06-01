use std::future::Future;
use std::sync::mpsc;
use std::time::Duration;

use sfo_reuseport::{
    Error, PacketMeta, QuicCidGenerator, QuicServer, ServerRuntime, ServerRuntimeConfig,
    TcpServer, TcpServiceConfig, TcpStream, UdpServer, UdpServiceConfig, UdpSocket,
};
#[cfg(windows)]
use sfo_reuseport::DEFAULT_ROUTED_PACKET_CHANNEL_CAPACITY;

fn assert_tcp_handler<F, Fut>(_handler: F)
where
    F: Fn(TcpStream) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<(), Error>> + Send + 'static,
{
}

fn assert_udp_handler<F, Fut>(_handler: F)
where
    F: Fn(UdpSocket, PacketMeta, Vec<u8>) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<(), Error>> + Send + 'static,
{
}

fn assert_socket_callback<F, Fut>(_callback: F)
where
    F: Fn(UdpSocket, usize) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<(), Error>> + Send + 'static,
{
}

#[test]
fn regular_callback_signatures_do_not_include_worker_id() {
    assert_tcp_handler(|_stream| async { Ok(()) });
    assert_udp_handler(|_socket, _meta, _payload| async { Ok(()) });
}

#[test]
fn socket_only_callback_signatures_include_worker_id() {
    assert_socket_callback(|_socket, _worker_id| async { Ok(()) });
}

#[test]
fn server_entrypoints_are_public() {
    let tcp_config = TcpServiceConfig::new("127.0.0.1:0".parse().unwrap());
    let udp_config = UdpServiceConfig::new("127.0.0.1:0".parse().unwrap());
    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(1)).unwrap();
    let tcp: Result<TcpServer, Error> =
        TcpServer::serve(&runtime, tcp_config, |_stream| async { Ok(()) });
    let udp: Result<UdpServer, Error> =
        UdpServer::serve(&runtime, udp_config.clone(), |_socket, _meta, _payload| async {
            Ok(())
        });
    let quic: Result<QuicServer, Error> =
        QuicServer::serve(&runtime, udp_config, |_socket, _meta, _payload| async {
            Ok(())
        });
    let (udp_tx, udp_rx) = mpsc::channel();
    let (quic_tx, quic_rx) = mpsc::channel();
    let udp_socket: Result<UdpServer, Error> = UdpServer::serve_socket(
        &runtime,
        UdpServiceConfig::new("127.0.0.1:0".parse().unwrap()),
        move |socket, worker_id| {
            let udp_tx = udp_tx.clone();
            async move {
                udp_tx.send((socket.local_addr()?, worker_id)).unwrap();
                Ok(())
            }
        },
    );
    let quic_socket: Result<QuicServer, Error> = QuicServer::serve_socket(
        &runtime,
        UdpServiceConfig::new("127.0.0.1:0".parse().unwrap()),
        move |socket, worker_id| {
            let quic_tx = quic_tx.clone();
            async move {
                quic_tx.send((socket.local_addr()?, worker_id)).unwrap();
                Ok(())
            }
        },
    );

    tcp.unwrap().close().unwrap();
    udp.unwrap().close().unwrap();
    quic.unwrap().close().unwrap();
    let udp_socket = udp_socket.unwrap();
    let quic_socket = quic_socket.unwrap();
    assert_eq!(udp_rx.recv_timeout(Duration::from_secs(2)).unwrap().1, 0);
    assert_eq!(quic_rx.recv_timeout(Duration::from_secs(2)).unwrap().1, 0);
    udp_socket.close().unwrap();
    quic_socket.close().unwrap();
}

#[test]
fn service_config_exposes_per_worker_concurrency_limit() {
    let addr = "127.0.0.1:0".parse().unwrap();

    assert_eq!(TcpServiceConfig::new(addr).max_concurrency_per_worker(), None);
    assert_eq!(UdpServiceConfig::new(addr).max_concurrency_per_worker(), None);
    assert_eq!(
        TcpServiceConfig::new(addr)
            .with_max_concurrency_per_worker(2)
            .max_concurrency_per_worker(),
        Some(2)
    );
    assert_eq!(
        UdpServiceConfig::new(addr)
            .with_max_concurrency_per_worker(0)
            .max_concurrency_per_worker(),
        Some(0)
    );
    assert_eq!(
        UdpServiceConfig::new(addr)
            .with_max_concurrency_per_worker(2)
            .max_concurrency_per_worker(),
        Some(2)
    );
}

#[test]
#[cfg(windows)]
fn service_config_exposes_routed_packet_channel_capacity() {
    let addr = "127.0.0.1:0".parse().unwrap();

    assert_eq!(
        UdpServiceConfig::new(addr).routed_packet_channel_capacity(),
        DEFAULT_ROUTED_PACKET_CHANNEL_CAPACITY
    );
    assert_eq!(
        UdpServiceConfig::new(addr)
            .with_routed_packet_channel_capacity(128)
            .routed_packet_channel_capacity(),
        128
    );

    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(1)).unwrap();
    let result = UdpServer::serve(
        &runtime,
        UdpServiceConfig::new(addr).with_routed_packet_channel_capacity(0),
        |_socket, _meta, _payload| async { Ok(()) },
    );
    assert!(matches!(result, Err(Error::InvalidConfig(_))));

    let result = QuicServer::serve(
        &runtime,
        UdpServiceConfig::new(addr).with_routed_packet_channel_capacity(0),
        |_socket, _meta, _payload| async { Ok(()) },
    );
    assert!(matches!(result, Err(Error::InvalidConfig(_))));

    let tcp = TcpServer::serve(
        &runtime,
        TcpServiceConfig::new(addr),
        |_stream| async { Ok(()) },
    );
    assert!(tcp.is_ok());
    tcp.unwrap().close().unwrap();
}

#[test]
#[cfg(not(windows))]
fn routed_packet_channel_capacity_api_is_windows_only() {
    let config = include_str!("../../src/core/config.rs");
    let capacity_setter = config
        .find("pub fn with_routed_packet_channel_capacity")
        .unwrap();
    let capacity_getter = config.find("pub fn routed_packet_channel_capacity").unwrap();
    assert!(config[..capacity_setter].contains("#[cfg(windows)]"));
    assert!(config[capacity_setter..capacity_getter].contains("#[cfg(windows)]"));
}

#[test]
fn service_config_types_are_split_by_protocol_family() {
    let config = include_str!("../../src/core/config.rs");
    let lib = include_str!("../../src/lib.rs");

    assert!(config.contains("pub struct TcpServiceConfig"));
    assert!(config.contains("pub struct UdpServiceConfig"));
    assert!(!config.contains("pub struct ServiceConfig"));
    assert!(
        !lib.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .any(|token| token == "ServiceConfig")
    );

    let tcp_start = config.find("pub struct TcpServiceConfig").unwrap();
    let udp_start = config.find("pub struct UdpServiceConfig").unwrap();
    let tcp_section = &config[tcp_start..udp_start];
    assert!(!tcp_section.contains("routed_packet_channel_capacity"));
}

#[test]
fn legacy_server_entrypoints_are_not_public() {
    let tcp = include_str!("../../src/core/tcp.rs");
    let udp = include_str!("../../src/core/udp.rs");
    assert!(!tcp.contains("pub async fn serve_with_runtime"));
    assert!(!tcp.contains("pub fn serve_on"));
    assert!(!udp.contains("pub async fn serve_with_runtime"));
    assert!(!udp.contains("pub fn serve_on"));
}

#[test]
fn serve_entrypoints_are_synchronous_and_do_not_pending() {
    let tcp = include_str!("../../src/core/tcp.rs");
    let udp = include_str!("../../src/core/udp.rs");

    assert!(tcp.contains("pub fn serve"));
    assert!(udp.contains("pub fn serve"));
    assert!(udp.contains("pub fn serve_socket"));
    assert!(!tcp.contains("pub async fn serve"));
    assert!(!udp.contains("pub async fn serve"));
    assert!(!udp.contains("pub async fn serve_socket"));
    assert!(!tcp.contains("std::future::pending"));
    assert!(!udp.contains("std::future::pending"));
    assert!(!tcp.contains("pending::<"));
    assert!(!udp.contains("pending::<"));
}

#[test]
fn listener_dynamic_management_api_is_not_public() {
    let tcp = include_str!("../../src/core/tcp.rs");
    let udp = include_str!("../../src/core/udp.rs");
    let runtime = include_str!("../../src/core/server_runtime.rs");
    let lib = include_str!("../../src/lib.rs");
    let core = include_str!("../../src/core/mod.rs");

    assert!(!tcp.contains("pub fn add_tcp_listener"));
    assert!(!udp.contains("pub fn add_udp_listener"));
    assert!(!udp.contains("pub fn add_quic_listener"));
    assert!(!runtime.contains("pub fn remove_listener"));
    assert!(!runtime.contains("ListenerId"));
    assert!(!runtime.contains("ListenerProtocol"));
    assert!(!lib.contains("ListenerId"));
    assert!(!lib.contains("ListenerProtocol"));
    assert!(!core.contains("ListenerId"));
    assert!(!core.contains("ListenerProtocol"));
}

#[test]
fn balanced_udp_socket_is_not_public() {
    let lib = include_str!("../../src/lib.rs");
    let core = include_str!("../../src/core/mod.rs");
    assert!(!lib.contains("BalancedUdpSocket"));
    assert!(!core.contains("BalancedUdpSocket"));
}

#[test]
fn dispatch_policy_is_not_public() {
    let lib = include_str!("../../src/lib.rs");
    let core = include_str!("../../src/core/mod.rs");
    let config = include_str!("../../src/core/config.rs");

    assert!(!lib.contains("DispatchPolicy"));
    assert!(!core.contains("DispatchPolicy"));
    assert!(!config.contains("with_dispatch"));
}

#[test]
fn quinn_feature_is_default_off_and_has_no_dependency() {
    let cargo = include_str!("../../Cargo.toml");

    assert!(cargo.contains("quinn = []"));
    assert!(!cargo.contains("default = [\"runtime-tokio\", \"quinn\"]"));
    assert!(!cargo.contains("dep:quinn"));
    assert!(!cargo.contains("dep:quinn-udp"));
}

#[test]
fn quic_cid_generator_is_public_without_quinn_types() {
    let generator = QuicCidGenerator::new(3).unwrap();

    assert_eq!(generator.worker_index(), 3);
    assert_eq!(generator.cid_len(), QuicCidGenerator::DEFAULT_CID_LEN);
}

#[test]
fn quinn_udp_socket_helpers_are_feature_gated() {
    let udp = include_str!("../../src/core/udp.rs");

    assert!(udp.contains("#[cfg(feature = \"quinn\")]\n    pub fn try_send_to"));
    assert!(udp.contains("#[cfg(feature = \"quinn\")]\n    pub fn poll_send_ready"));
    assert!(udp.contains("#[cfg(feature = \"quinn\")]\n    pub fn poll_recv_from"));
    assert!(udp.contains("#[cfg(feature = \"quinn\")]\n    pub fn poll_recv_from_vectored"));
}

#[cfg(feature = "quinn")]
#[test]
fn quinn_udp_socket_helpers_are_public_when_feature_enabled() {
    use std::future::poll_fn;
    use std::io::{self, IoSliceMut};
    use std::net::SocketAddr;

    fn assert_quinn_helpers(socket: &UdpSocket, target: SocketAddr) {
        let mut buffer = [0_u8; 8];
        let mut vectored_buffer = [0_u8; 8];
        let mut buffers = [IoSliceMut::new(&mut vectored_buffer)];

        let _: io::Result<usize> = socket.try_send_to(b"ping", target);
        let _ = poll_fn(|cx| socket.poll_send_ready(cx));
        let _ = poll_fn(|cx| socket.poll_recv_from(cx, &mut buffer));
        let _ = poll_fn(|cx| socket.poll_recv_from_vectored(cx, &mut buffers));
    }

    let _ = assert_quinn_helpers as fn(&UdpSocket, SocketAddr);
}
