use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use sfo_reuseport::{ServerRuntime, ServerRuntimeConfig, ServiceConfig, TcpServer};

fn free_addr() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap()
}

#[tokio::test]
async fn tcp_loopback_serve_accepts_multiple_connections_without_exiting() {
    let addr = free_addr();
    let seen = Arc::new(AtomicUsize::new(0));
    let handler_seen = Arc::clone(&seen);
    let server = tokio::spawn(async move {
        let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(1))?;
        TcpServer::serve(
            &runtime,
            ServiceConfig::new(addr),
            move |_stream| {
                let handler_seen = Arc::clone(&handler_seen);
                async move {
                    handler_seen.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
            },
        )
        .await
    });

    for _ in 0..2 {
        let mut client = None;
        for _ in 0..50 {
            match tokio::net::TcpStream::connect(addr).await {
                Ok(stream) => {
                    client = Some(stream);
                    break;
                }
                Err(_) => tokio::time::sleep(std::time::Duration::from_millis(10)).await,
            }
        }
        let _client = client.expect("server should accept loopback connection");
    }

    for _ in 0..200 {
        if seen.load(Ordering::SeqCst) >= 2 {
            server.abort();
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    server.abort();
    panic!("handler should observe accepted connection");
}

#[tokio::test]
async fn tcp_rejects_zero_runtime_workers() {
    let message = match ServerRuntime::start(ServerRuntimeConfig::new().with_workers(0)) {
        Ok(_) => panic!("zero worker runtime should be rejected"),
        Err(error) => error.to_string(),
    };
    assert!(message.contains("worker count"));
}
