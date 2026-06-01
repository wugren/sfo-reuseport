use std::net::{TcpListener, UdpSocket as StdUdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use sfo_reuseport::{
    QuicServer, ServerRuntime, ServerRuntimeConfig, TcpServer, TcpServiceConfig, UdpServer,
    UdpServiceConfig, UdpSocket,
};

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

async fn wait_for_udp_listener_socket(server: &UdpServer) -> UdpSocket {
    for _ in 0..50 {
        if let Ok(socket) = server.listener_socket() {
            return socket;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("udp listener socket was not registered");
}

async fn wait_for_quic_listener_socket(server: &QuicServer) -> UdpSocket {
    for _ in 0..50 {
        if let Ok(socket) = server.listener_socket() {
            return socket;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("quic listener socket was not registered");
}

#[tokio::test]
async fn tcp_server_serve_registers_listener_on_runtime() {
    let addr = free_tcp_addr();
    let seen = Arc::new(AtomicUsize::new(0));
    let tcp_seen = Arc::clone(&seen);
    let server = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(1)).unwrap();

    TcpServer::serve(&server, TcpServiceConfig::new(addr), move |_stream| {
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
async fn tcp_server_close_allows_reopening_same_addr() {
    let addr = free_tcp_addr();
    let seen = Arc::new(AtomicUsize::new(0));
    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(1)).unwrap();

    let tcp = TcpServer::serve(&runtime, TcpServiceConfig::new(addr), |_stream| async { Ok(()) })
        .unwrap();

    tcp.close().unwrap();
    drop(tcp);

    let reopened = {
        let mut reopened = None;
        for _ in 0..50 {
            let tcp_seen = Arc::clone(&seen);
            match TcpServer::serve(&runtime, TcpServiceConfig::new(addr), move |_stream| {
                let tcp_seen = Arc::clone(&tcp_seen);
                async move {
                    tcp_seen.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
            }) {
                Ok(server) => {
                    reopened = Some(server);
                    break;
                }
                Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
            }
        }
        reopened.expect("tcp server should reopen the same address after close")
    };

    let _client = connect_with_retry(addr)
        .await
        .expect("reopened tcp server should accept connections");

    for _ in 0..50 {
        if seen.load(Ordering::SeqCst) == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(seen.load(Ordering::SeqCst), 1);
    reopened.close().unwrap();
}

#[tokio::test]
async fn udp_server_serve_registers_listener_on_runtime() {
    let addr = free_udp_addr();
    let seen = Arc::new(AtomicUsize::new(0));
    let udp_seen = Arc::clone(&seen);
    let server = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(1)).unwrap();

    UdpServer::serve(
        &server,
        UdpServiceConfig::new(addr),
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
async fn udp_server_listener_socket_is_available_and_close_stops_new_work() {
    let addr = free_udp_addr();
    let seen = Arc::new(AtomicUsize::new(0));
    let udp_seen = Arc::clone(&seen);
    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(1)).unwrap();

    let udp = UdpServer::serve(
        &runtime,
        UdpServiceConfig::new(addr),
        move |_socket, _meta, _payload| {
            let udp_seen = Arc::clone(&udp_seen);
            async move {
                udp_seen.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        },
    )
    .unwrap();

    let listener_socket = wait_for_udp_listener_socket(&udp).await;
    assert_eq!(listener_socket.local_addr().unwrap(), addr);

    udp.close().unwrap();
    assert!(udp.listener_socket().is_err());

    let client = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client.send_to(b"ping", addr).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    assert_eq!(seen.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn quic_server_listener_socket_is_available() {
    let addr = free_udp_addr();
    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(1)).unwrap();

    let quic = QuicServer::serve(
        &runtime,
        UdpServiceConfig::new(addr),
        |_socket, _meta, _payload| async { Ok(()) },
    )
    .unwrap();

    let listener_socket = wait_for_quic_listener_socket(&quic).await;
    assert_eq!(listener_socket.local_addr().unwrap(), addr);

    quic.close().unwrap();
    assert!(quic.listener_socket().is_err());
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
        TcpServiceConfig::new(tcp_addr),
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
        UdpServiceConfig::new(udp_addr),
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
