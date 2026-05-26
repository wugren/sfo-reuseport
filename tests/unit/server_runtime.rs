use sfo_reuseport::{ListenerConfig, ServerRuntime, ServerRuntimeConfig, ServiceConfig, WorkerCount};

#[test]
fn server_runtime_config_can_set_worker_count() {
    let config = ServerRuntimeConfig::new().with_workers(2);
    assert_eq!(config.workers, WorkerCount::Fixed(2));
}

#[test]
fn service_config_records_bind_addr_without_worker_count() {
    let addr = "127.0.0.1:0".parse().unwrap();
    let config = ServiceConfig::new(addr);
    assert_eq!(config.bind_addr, addr);
}

#[test]
fn listener_config_records_bind_addr_without_worker_count() {
    let addr = "127.0.0.1:0".parse().unwrap();
    let config = ListenerConfig::new(addr);
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
fn tcp_and_udp_servers_can_attach_to_server_runtime() {
    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(1)).unwrap();

    let tcp = runtime
        .add_tcp_listener(
            ListenerConfig::new("127.0.0.1:0".parse().unwrap()),
            |_stream| async { Ok(()) },
        )
        .unwrap();
    let udp = runtime
        .add_udp_listener(
            ListenerConfig::new("127.0.0.1:0".parse().unwrap()),
            |_socket, _meta, _payload| async { Ok(()) },
        )
        .unwrap();

    assert_ne!(tcp.get(), udp.get());
    runtime.remove_listener(tcp).unwrap();
    runtime.remove_listener(udp).unwrap();
}
