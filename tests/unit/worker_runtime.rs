use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc;
use std::time::Duration;

use sfo_reuseport::{ServerRuntime, ServerRuntimeConfig, TcpServiceConfig, TcpServer};

#[test]
fn worker_runtime_runs_listener_handler_on_named_worker_thread() {
    let addr = free_addr();
    let (sender, receiver) = mpsc::channel();
    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(1)).unwrap();
    TcpServer::serve(
        &runtime,
        TcpServiceConfig::new(addr),
        move |_stream| {
            let sender = sender.clone();
            async move {
                let name = std::thread::current().name().unwrap_or("").to_string();
                sender.send(name).unwrap();
                Ok(())
            }
        },
    )
    .unwrap();

    connect_with_retry(addr);

    let name = receiver.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(name.starts_with("sfo-reuseport-worker-"));
}

fn free_addr() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap()
}

fn connect_with_retry(addr: SocketAddr) {
    for _ in 0..50 {
        if TcpStream::connect(addr).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("worker listener should accept loopback connection");
}
