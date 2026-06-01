use sfo_reuseport::{
    QuicServer, ServerRuntime, ServerRuntimeConfig, TcpServer, TcpServiceConfig, UdpServer,
    UdpServiceConfig, WorkerCount,
};

#[test]
fn server_runtime_config_can_set_worker_count() {
    let config = ServerRuntimeConfig::new().with_workers(2);
    assert_eq!(config.workers, WorkerCount::Fixed(2));
}

#[test]
fn service_config_records_bind_addr_without_worker_count() {
    let addr = "127.0.0.1:0".parse().unwrap();
    let config = UdpServiceConfig::new(addr);
    assert_eq!(config.bind_addr, addr);
}

#[test]
fn server_runtime_does_not_depend_on_server_facades() {
    let source = include_str!("../../src/core/server_runtime.rs");
    assert!(!source.contains("TcpServer"));
    assert!(!source.contains("UdpServer"));
    assert!(!source.contains("QuicServer"));
    assert!(!source.contains("tcp::"));
    assert!(!source.contains("udp::"));
    assert!(!source.contains("crate::core::{tcp"));
    assert!(!source.contains("crate::core::{udp"));
}

#[test]
fn servers_return_handles_when_attached_to_server_runtime_through_serve() {
    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(1)).unwrap();

    let tcp = TcpServer::serve(
        &runtime,
        TcpServiceConfig::new("127.0.0.1:0".parse().unwrap()),
        |_stream| async { Ok(()) },
    )
    .unwrap();
    let udp = UdpServer::serve(
        &runtime,
        UdpServiceConfig::new("127.0.0.1:0".parse().unwrap()),
        |_socket, _meta, _payload| async { Ok(()) },
    )
    .unwrap();
    let quic = QuicServer::serve(
        &runtime,
        UdpServiceConfig::new("127.0.0.1:0".parse().unwrap()),
        |_socket, _meta, _payload| async { Ok(()) },
    )
    .unwrap();

    tcp.close().unwrap();
    udp.close().unwrap();
    quic.close().unwrap();
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
