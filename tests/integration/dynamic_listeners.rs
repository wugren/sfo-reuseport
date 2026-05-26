use std::net::{TcpListener, UdpSocket as StdUdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use sfo_reuseport::{ServerRuntime, ServerRuntimeConfig, ServiceConfig, TcpServer, UdpServer};

fn free_tcp_addr() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap()
}

fn free_udp_addr() -> std::net::SocketAddr {
    let socket = StdUdpSocket::bind("127.0.0.1:0").unwrap();
    socket.local_addr().unwrap()
}

async fn connect_with_retry(addr: std::net::SocketAddr) -> Option<tokio::net::TcpStream> {
    for _ in 0..50 {
        match tokio::net::TcpStream::connect(addr).await {
            Ok(stream) => return Some(stream),
            Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
        }
    }
    None
}

#[tokio::test]
async fn tcp_server_serve_registers_listener_on_runtime() {
    let addr = free_tcp_addr();
    let seen = Arc::new(AtomicUsize::new(0));
    let tcp_seen = Arc::clone(&seen);
    let server = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(1)).unwrap();

    TcpServer::serve(&server, ServiceConfig::new(addr), move |_stream| {
        let tcp_seen = Arc::clone(&tcp_seen);
        async move {
            tcp_seen.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    })
    .unwrap();
    let _client = connect_with_retry(addr)
        .await
        .expect("dynamic tcp listener should accept connections");

    for _ in 0..50 {
        if seen.load(Ordering::SeqCst) == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(seen.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn udp_server_serve_registers_listener_on_runtime() {
    let addr = free_udp_addr();
    let seen = Arc::new(AtomicUsize::new(0));
    let udp_seen = Arc::clone(&seen);
    let server = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(1)).unwrap();

    UdpServer::serve(
        &server,
        ServiceConfig::new(addr),
        move |_socket, _meta, _payload| {
            let udp_seen = Arc::clone(&udp_seen);
            async move {
                udp_seen.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        },
    )
    .unwrap();
    let client = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client.send_to(b"ping", addr).await.unwrap();

    for _ in 0..50 {
        if seen.load(Ordering::SeqCst) == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(seen.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn one_server_runtime_handles_tcp_and_udp_serve_listeners() {
    let tcp_addr = free_tcp_addr();
    let udp_addr = free_udp_addr();
    let tcp_seen = Arc::new(AtomicUsize::new(0));
    let udp_seen = Arc::new(AtomicUsize::new(0));
    let tcp_handler_seen = Arc::clone(&tcp_seen);
    let udp_handler_seen = Arc::clone(&udp_seen);

    let server = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(1)).unwrap();

    TcpServer::serve(
        &server,
        ServiceConfig::new(tcp_addr),
        move |_stream| {
            let tcp_handler_seen = Arc::clone(&tcp_handler_seen);
            async move {
                tcp_handler_seen.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        },
    )
    .unwrap();
    UdpServer::serve(
        &server,
        ServiceConfig::new(udp_addr),
        move |_socket, _meta, _payload| {
            let udp_handler_seen = Arc::clone(&udp_handler_seen);
            async move {
                udp_handler_seen.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        },
    )
    .unwrap();

    let _client = connect_with_retry(tcp_addr)
        .await
        .expect("mixed dynamic tcp listener should accept connections");
    let udp_client = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    udp_client.send_to(b"ping", udp_addr).await.unwrap();

    for _ in 0..50 {
        if tcp_seen.load(Ordering::SeqCst) == 1 && udp_seen.load(Ordering::SeqCst) == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(tcp_seen.load(Ordering::SeqCst), 1);
    assert_eq!(udp_seen.load(Ordering::SeqCst), 1);
}
