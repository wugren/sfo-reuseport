use std::future::Future;
use std::sync::mpsc;
use std::time::Duration;

use sfo_reuseport::{
    Error, PacketMeta, QuicCidGenerator, QuicServer, ServerRuntime, ServerRuntimeConfig,
    ServiceConfig, TcpServer, TcpStream, UdpServer, UdpSocket,
};

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
    let config = ServiceConfig::new("127.0.0.1:0".parse().unwrap());
    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(1)).unwrap();
    let tcp: Result<TcpServer, Error> = TcpServer::serve(&runtime, config.clone(), |_stream| async {
        Ok(())
    });
    let udp: Result<UdpServer, Error> =
        UdpServer::serve(&runtime, config.clone(), |_socket, _meta, _payload| async {
            Ok(())
        });
    let quic: Result<QuicServer, Error> =
        QuicServer::serve(&runtime, config, |_socket, _meta, _payload| async { Ok(()) });
    let (udp_tx, udp_rx) = mpsc::channel();
    let (quic_tx, quic_rx) = mpsc::channel();
    let udp_socket: Result<UdpServer, Error> = UdpServer::serve_socket(
        &runtime,
        ServiceConfig::new("127.0.0.1:0".parse().unwrap()),
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
        ServiceConfig::new("127.0.0.1:0".parse().unwrap()),
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
