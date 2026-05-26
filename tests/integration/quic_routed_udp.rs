use std::net::UdpSocket as StdUdpSocket;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use sfo_reuseport::{
    ListenerConfig, QuicServer, ServerRuntime, ServerRuntimeConfig, ServiceConfig,
};

fn free_addr() -> std::net::SocketAddr {
    let socket = StdUdpSocket::bind("127.0.0.1:0").unwrap();
    socket.local_addr().unwrap()
}

#[tokio::test]
async fn quic_server_serve_delivers_long_header_dcid_and_sends_response() {
    let addr = free_addr();
    let server = tokio::spawn(async move {
        let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(3))?;
        QuicServer::serve(
            &runtime,
            ServiceConfig::new(addr),
            |socket, meta, payload| async move {
                assert_eq!(payload, [0xc0, 0, 0, 0, 1, 4, 0, 2, 9, 9]);
                socket.send_to(b"quic-ok", meta.peer_addr.unwrap()).await?;
                Ok(())
            },
        )
        .await
    });

    let client = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client
        .send_to(&[0xc0, 0, 0, 0, 1, 4, 0, 2, 9, 9], addr)
        .await
        .unwrap();

    let mut buffer = [0_u8; 16];
    let (len, _) = tokio::time::timeout(Duration::from_secs(2), client.recv_from(&mut buffer))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&buffer[..len], b"quic-ok");

    server.abort();
}

#[tokio::test]
async fn quic_routed_udp_delivers_long_header_dcid_to_target_worker() {
    let addr = free_addr();
    let seen_worker = Arc::new(Mutex::new(None));
    let handler_seen_worker = Arc::clone(&seen_worker);
    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(3)).unwrap();
    let listener = runtime.add_quic_listener(
        ListenerConfig::new(addr),
        move |socket, meta, _payload| {
            let handler_seen_worker = Arc::clone(&handler_seen_worker);
            async move {
                let thread = std::thread::current();
                let name = thread.name().unwrap_or_default().to_string();
                *handler_seen_worker.lock().unwrap() = Some(name);
                socket.send_to(b"ok", meta.peer_addr.unwrap()).await?;
                Ok(())
            }
        },
    )
    .unwrap();

    let client = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let packet = [0xc0, 0, 0, 0, 1, 4, 0, 2, 9, 9];
    client.send_to(&packet, addr).await.unwrap();

    let mut buffer = [0_u8; 8];
    let (len, _) = tokio::time::timeout(Duration::from_secs(2), client.recv_from(&mut buffer))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&buffer[..len], b"ok");

    for _ in 0..100 {
        if let Some(name) = seen_worker.lock().unwrap().clone() {
            assert!(name.ends_with("worker-2"), "unexpected worker thread: {name}");
            runtime.remove_listener(listener).unwrap();
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    panic!("quic routed udp handler did not record a worker");
}

#[tokio::test]
async fn quic_routed_udp_drops_invalid_route_key() {
    let addr = free_addr();
    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(2)).unwrap();
    let listener = runtime.add_quic_listener(
        ListenerConfig::new(addr),
        |_socket, _meta, _payload| async {
            panic!("invalid QUIC route key should not reach handler");
        },
    )
    .unwrap();

    let client = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client.send_to(&[0xc0, 0, 0, 0, 1, 4, 1], addr).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    runtime.remove_listener(listener).unwrap();
}

#[tokio::test]
async fn quic_routed_udp_requires_sixteen_bit_worker_shard() {
    let addr = free_addr();
    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(2)).unwrap();
    let listener = runtime.add_quic_listener(
        ListenerConfig::new(addr),
        |_socket, _meta, _payload| async {
            panic!("one-byte DCID shard should not reach handler");
        },
    )
    .unwrap();

    let client = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client.send_to(&[0xc0, 0, 0, 0, 1, 1, 1], addr).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    runtime.remove_listener(listener).unwrap();
}
