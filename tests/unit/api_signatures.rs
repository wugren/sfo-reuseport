use std::future::Future;

use sfo_reuseport::{
    Error, PacketMeta, QuicServer, ServerRuntime, ServerRuntimeConfig, ServiceConfig, TcpServer,
    TcpStream, UdpServer, UdpSocket,
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

#[test]
fn callback_signatures_do_not_include_worker_id() {
    assert_tcp_handler(|_stream| async { Ok(()) });
    assert_udp_handler(|_socket, _meta, _payload| async { Ok(()) });
}

#[test]
fn server_entrypoints_are_public() {
    let _tcp = TcpServer;
    let _udp = UdpServer;
    let _quic = QuicServer;
    let config = ServiceConfig::new("127.0.0.1:0".parse().unwrap());
    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(1)).unwrap();
    let tcp = TcpServer::serve(&runtime, config.clone(), |_stream| async { Ok(()) });
    let udp = UdpServer::serve(&runtime, config.clone(), |_socket, _meta, _payload| async {
        Ok(())
    });
    let quic = QuicServer::serve(&runtime, config, |_socket, _meta, _payload| async {
        Ok(())
    });
    drop((tcp, udp, quic));
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
