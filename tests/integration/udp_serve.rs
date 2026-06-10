use std::cell::RefCell;
use std::net::UdpSocket as StdUdpSocket;
#[cfg(feature = "quinn")]
use std::future::poll_fn;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use sfo_reuseport::{
    Error, QuicServer, ServerRuntime, ServerRuntimeConfig, UdpServiceConfig, UdpServer,
};

fn free_addr() -> std::net::SocketAddr {
    let socket = StdUdpSocket::bind("127.0.0.1:0").unwrap();
    socket.local_addr().unwrap()
}

#[tokio::test]
async fn udp_loopback_serve_receives_packet_and_sends_response() {
    let addr = free_addr();
    let server = tokio::spawn(async move {
        let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(1))?;
        UdpServer::serve(
            &runtime,
            UdpServiceConfig::new(addr),
            |_socket, meta, payload| async move {
                assert_eq!(payload, b"ping");
                _socket.send_to(b"pong", meta.peer_addr.unwrap()).await?;
                Ok(())
            },
        )?;
        std::future::pending::<Result<(), Error>>().await
    });

    let client = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client.send_to(b"ping", addr).await.unwrap();
    let mut buffer = [0_u8; 16];
    let (len, _) = client.recv_from(&mut buffer).await.unwrap();
    assert_eq!(&buffer[..len], b"pong");
    server.abort();
}

#[tokio::test]
async fn udp_serve_with_state_reuses_mutable_worker_state() {
    let addr = free_addr();
    let (hit_tx, mut hit_rx) = tokio::sync::mpsc::unbounded_channel();
    let server = tokio::spawn(async move {
        let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(1))?;
        UdpServer::serve_with_state(
            &runtime,
            UdpServiceConfig::new(addr),
            || Rc::new(RefCell::new(0_usize)),
            move |state, socket, meta, _payload| {
                let hit_tx = hit_tx.clone();
                async move {
                    let hit = {
                        let mut hits = state.borrow_mut();
                        *hits += 1;
                        *hits
                    };
                    hit_tx.send(hit).unwrap();
                    socket.send_to(hit.to_string().as_bytes(), meta.peer_addr.unwrap()).await?;
                    Ok(())
                }
            },
        )?;
        std::future::pending::<Result<(), Error>>().await
    });

    let client = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let mut buffer = [0_u8; 16];
    client.send_to(b"first", addr).await.unwrap();
    let (len, _) = tokio::time::timeout(Duration::from_secs(2), client.recv_from(&mut buffer))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&buffer[..len], b"1");
    client.send_to(b"second", addr).await.unwrap();
    let (len, _) = tokio::time::timeout(Duration::from_secs(2), client.recv_from(&mut buffer))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&buffer[..len], b"2");

    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), hit_rx.recv())
            .await
            .unwrap(),
        Some(1)
    );
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), hit_rx.recv())
            .await
            .unwrap(),
        Some(2)
    );
    server.abort();
}

#[tokio::test]
async fn udp_server_serve_socket_returns_socket_for_application_recv() {
    let addr = free_addr();
    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(1)).unwrap();
    let (socket_tx, socket_rx) = mpsc::channel();
    let server = UdpServer::serve_socket(&runtime, UdpServiceConfig::new(addr), move |socket, worker_id| {
        let socket_tx = socket_tx.clone();
        async move {
            socket_tx.send((socket, worker_id)).unwrap();
            Ok(())
        }
    })
    .unwrap();
    let (socket, worker_id) = socket_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(worker_id, 0);
    let local_addr = socket.local_addr().unwrap();

    let client = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client.send_to(b"socket-only", local_addr).await.unwrap();

    let buffer = vec![0_u8; 32];
    let (len, peer_addr, buffer) = tokio::time::timeout(Duration::from_secs(2), socket.recv_from_vec(buffer))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(&buffer[..len], b"socket-only");
    assert_eq!(peer_addr, client.local_addr().unwrap());
    server.close().unwrap();
}

#[tokio::test]
async fn quic_server_serve_socket_delivers_quic_routable_packet_to_application_socket() {
    let addr = free_addr();
    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(1)).unwrap();
    let (socket_tx, socket_rx) = mpsc::channel();
    let server = QuicServer::serve_socket(&runtime, UdpServiceConfig::new(addr), move |socket, worker_id| {
        let socket_tx = socket_tx.clone();
        async move {
            socket_tx.send((socket, worker_id)).unwrap();
            Ok(())
        }
    })
    .unwrap();
    let (socket, worker_id) = socket_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(worker_id, 0);
    let local_addr = socket.local_addr().unwrap();

    let client = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let packet = [0xe0, 0, 0, 0, 1, 4, 0, 0, b'p', b'i', b'n', b'g'];
    client.send_to(&packet, local_addr).await.unwrap();

    let mut buffer = [0_u8; 32];
    let (len, peer_addr) = tokio::time::timeout(Duration::from_secs(2), socket.recv_from(&mut buffer))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(&buffer[..len], &packet);
    assert_eq!(peer_addr, client.local_addr().unwrap());
    server.close().unwrap();
}

#[cfg(feature = "quinn")]
#[tokio::test]
async fn udp_server_serve_socket_quinn_helpers_recv_and_send_on_native_socket() {
    let addr = free_addr();
    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(1)).unwrap();
    let (socket_tx, socket_rx) = mpsc::channel();
    let server = UdpServer::serve_socket(&runtime, UdpServiceConfig::new(addr), move |socket, worker_id| {
        let socket_tx = socket_tx.clone();
        async move {
            socket_tx.send((socket, worker_id)).unwrap();
            Ok(())
        }
    })
    .unwrap();
    let (socket, worker_id) = socket_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(worker_id, 0);
    let local_addr = socket.local_addr().unwrap();

    let client = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client.send_to(b"quinn-helper", local_addr).await.unwrap();

    let mut buffer = [0_u8; 32];
    let (len, peer_addr) = tokio::time::timeout(
        Duration::from_secs(2),
        poll_fn(|cx| socket.poll_recv_from(cx, &mut buffer)),
    )
    .await
    .unwrap()
    .unwrap();

    assert_eq!(&buffer[..len], b"quinn-helper");
    assert_eq!(peer_addr, client.local_addr().unwrap());

    tokio::time::timeout(Duration::from_secs(2), poll_fn(|cx| socket.poll_send_ready(cx)))
        .await
        .unwrap()
        .unwrap();
    socket.try_send_to(b"quinn-reply", peer_addr).unwrap();

    let mut reply = [0_u8; 32];
    let (len, source) = tokio::time::timeout(Duration::from_secs(2), client.recv_from(&mut reply))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&reply[..len], b"quinn-reply");
    assert_eq!(source, local_addr);
    server.close().unwrap();
}
