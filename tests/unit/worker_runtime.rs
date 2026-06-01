use std::net::{SocketAddr, TcpListener};

use sfo_reuseport::{ServerRuntime, ServerRuntimeConfig, TcpServiceConfig, TcpServer};

#[test]
fn worker_runtime_accepts_listener_registration_without_driving_local_handlers() {
    let addr = free_addr();
    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(1)).unwrap();
    TcpServer::serve(
        &runtime,
        TcpServiceConfig::new(addr),
        move |_stream| async move { Ok(()) },
    )
    .unwrap();
}

fn free_addr() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap()
}
