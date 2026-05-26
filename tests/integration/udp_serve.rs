use std::net::UdpSocket as StdUdpSocket;

use sfo_reuseport::{ServerRuntime, ServerRuntimeConfig, ServiceConfig, UdpServer};

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
            ServiceConfig::new(addr),
            |_socket, meta, payload| async move {
                assert_eq!(payload, b"ping");
                _socket.send_to(b"pong", meta.peer_addr.unwrap()).await?;
                Ok(())
            },
        )
        .await
    });

    let client = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client.send_to(b"ping", addr).await.unwrap();
    let mut buffer = [0_u8; 16];
    let (len, _) = client.recv_from(&mut buffer).await.unwrap();
    assert_eq!(&buffer[..len], b"pong");
    server.abort();
}
