use std::net::{TcpListener, UdpSocket as StdUdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use sfo_reuseport::{
    QuicServer, ServerRuntime, ServerRuntimeConfig, TcpServer, TcpServiceConfig, UdpServer,
    UdpServiceConfig,
};
use tokio::sync::Semaphore;

fn free_tcp_addr() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap()
}

fn free_udp_addr() -> std::net::SocketAddr {
    let socket = StdUdpSocket::bind("127.0.0.1:0").unwrap();
    socket.local_addr().unwrap()
}

#[tokio::test]
async fn tcp_concurrency_limit_waits_for_per_worker_permit() {
    let addr = free_tcp_addr();
    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(1)).unwrap();
    let entered = tracked_entries();
    let release = Arc::new(Semaphore::new(0));

    let server = TcpServer::serve(
        &runtime,
        TcpServiceConfig::new(addr).with_max_concurrency_per_worker(1),
        {
            let entered = entered.clone();
            let release = Arc::clone(&release);
            move |_stream| {
                let entered = entered.clone();
                let release = Arc::clone(&release);
                async move {
                    entered.enter();
                    let permit = release.acquire().await.unwrap();
                    drop(permit);
                    entered.exit();
                    Ok(())
                }
            }
        },
    )
    .unwrap();

    let _first = tokio::net::TcpStream::connect(addr).await.unwrap();
    entered.wait_for_total(1);
    let _second = tokio::net::TcpStream::connect(addr).await.unwrap();
    entered.assert_no_new_entry();

    release.add_permits(1);
    entered.wait_for_total(2);
    release.add_permits(1);
    server.close().unwrap();
}

#[tokio::test]
async fn tcp_concurrency_limit_close_exits_waiting_listener() {
    let addr = free_tcp_addr();
    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(1)).unwrap();
    let entered = tracked_entries();
    let release = Arc::new(Semaphore::new(0));

    let server = TcpServer::serve(
        &runtime,
        TcpServiceConfig::new(addr).with_max_concurrency_per_worker(1),
        {
            let entered = entered.clone();
            let release = Arc::clone(&release);
            move |_stream| {
                let entered = entered.clone();
                let release = Arc::clone(&release);
                async move {
                    entered.enter();
                    let permit = release.acquire().await.unwrap();
                    drop(permit);
                    entered.exit();
                    Ok(())
                }
            }
        },
    )
    .unwrap();

    let _first = tokio::net::TcpStream::connect(addr).await.unwrap();
    entered.wait_for_total(1);
    let _second = tokio::net::TcpStream::connect(addr).await.unwrap();
    entered.assert_no_new_entry();

    server.close().unwrap();
    release.add_permits(1);
    entered.assert_no_new_entry();
}

#[tokio::test]
async fn udp_concurrency_limit_waits_for_per_worker_permit() {
    let addr = free_udp_addr();
    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(1)).unwrap();
    let entered = tracked_entries();
    let release = Arc::new(Semaphore::new(0));

    let server = UdpServer::serve(
        &runtime,
        UdpServiceConfig::new(addr).with_max_concurrency_per_worker(1),
        {
            let entered = entered.clone();
            let release = Arc::clone(&release);
            move |_socket, _meta, _payload| {
                let entered = entered.clone();
                let release = Arc::clone(&release);
                async move {
                    entered.enter();
                    let permit = release.acquire().await.unwrap();
                    drop(permit);
                    entered.exit();
                    Ok(())
                }
            }
        },
    )
    .unwrap();

    let client = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client.send_to(b"first", addr).await.unwrap();
    entered.wait_for_total(1);
    client.send_to(b"second", addr).await.unwrap();
    entered.assert_no_new_entry();

    release.add_permits(1);
    entered.wait_for_total(2);
    release.add_permits(1);
    server.close().unwrap();
}

#[tokio::test]
async fn udp_concurrency_limit_close_exits_waiting_listener() {
    let addr = free_udp_addr();
    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(1)).unwrap();
    let entered = tracked_entries();
    let release = Arc::new(Semaphore::new(0));

    let server = UdpServer::serve(
        &runtime,
        UdpServiceConfig::new(addr).with_max_concurrency_per_worker(1),
        {
            let entered = entered.clone();
            let release = Arc::clone(&release);
            move |_socket, _meta, _payload| {
                let entered = entered.clone();
                let release = Arc::clone(&release);
                async move {
                    entered.enter();
                    let permit = release.acquire().await.unwrap();
                    drop(permit);
                    entered.exit();
                    Ok(())
                }
            }
        },
    )
    .unwrap();

    let client = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client.send_to(b"first", addr).await.unwrap();
    entered.wait_for_total(1);
    client.send_to(b"second", addr).await.unwrap();
    entered.assert_no_new_entry();

    server.close().unwrap();
    release.add_permits(1);
    entered.assert_no_new_entry();
}

#[tokio::test]
async fn quic_concurrency_limit_waits_for_per_worker_permit() {
    let addr = free_udp_addr();
    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(1)).unwrap();
    let entered = tracked_entries();
    let release = Arc::new(Semaphore::new(0));

    let server = QuicServer::serve(
        &runtime,
        UdpServiceConfig::new(addr).with_max_concurrency_per_worker(1),
        {
            let entered = entered.clone();
            let release = Arc::clone(&release);
            move |_socket, _meta, _payload| {
                let entered = entered.clone();
                let release = Arc::clone(&release);
                async move {
                    entered.enter();
                    let permit = release.acquire().await.unwrap();
                    drop(permit);
                    entered.exit();
                    Ok(())
                }
            }
        },
    )
    .unwrap();

    let client = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client.send_to(&quic_packet(b'a'), addr).await.unwrap();
    entered.wait_for_total(1);
    client.send_to(&quic_packet(b'b'), addr).await.unwrap();
    entered.assert_no_new_entry();

    release.add_permits(1);
    entered.wait_for_total(2);
    release.add_permits(1);
    server.close().unwrap();
}

#[tokio::test]
async fn quic_concurrency_limit_close_exits_waiting_listener() {
    let addr = free_udp_addr();
    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(1)).unwrap();
    let entered = tracked_entries();
    let release = Arc::new(Semaphore::new(0));

    let server = QuicServer::serve(
        &runtime,
        UdpServiceConfig::new(addr).with_max_concurrency_per_worker(1),
        {
            let entered = entered.clone();
            let release = Arc::clone(&release);
            move |_socket, _meta, _payload| {
                let entered = entered.clone();
                let release = Arc::clone(&release);
                async move {
                    entered.enter();
                    let permit = release.acquire().await.unwrap();
                    drop(permit);
                    entered.exit();
                    Ok(())
                }
            }
        },
    )
    .unwrap();

    let client = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client.send_to(&quic_packet(b'a'), addr).await.unwrap();
    entered.wait_for_total(1);
    client.send_to(&quic_packet(b'b'), addr).await.unwrap();
    entered.assert_no_new_entry();

    server.close().unwrap();
    release.add_permits(1);
    entered.assert_no_new_entry();
}

fn quic_packet(last: u8) -> [u8; 12] {
    [0xe0, 0, 0, 0, 1, 4, 0, 0, b'p', b'i', b'n', last]
}

fn tracked_entries() -> TrackedEntries {
    let (sender, receiver) = mpsc::channel();
    TrackedEntries {
        active: Arc::new(AtomicUsize::new(0)),
        max_active: Arc::new(AtomicUsize::new(0)),
        total: Arc::new(AtomicUsize::new(0)),
        sender,
        receiver: Arc::new(std::sync::Mutex::new(receiver)),
    }
}

#[derive(Clone)]
struct TrackedEntries {
    active: Arc<AtomicUsize>,
    max_active: Arc<AtomicUsize>,
    total: Arc<AtomicUsize>,
    sender: mpsc::Sender<usize>,
    receiver: Arc<std::sync::Mutex<mpsc::Receiver<usize>>>,
}

impl TrackedEntries {
    fn enter(&self) {
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_active.fetch_max(active, Ordering::SeqCst);
        let total = self.total.fetch_add(1, Ordering::SeqCst) + 1;
        self.sender.send(total).unwrap();
    }

    fn exit(&self) {
        self.active.fetch_sub(1, Ordering::SeqCst);
    }

    fn wait_for_total(&self, expected: usize) {
        while self.total.load(Ordering::SeqCst) < expected {
            self.receiver
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(2))
                .unwrap();
        }
        assert_eq!(self.max_active.load(Ordering::SeqCst), 1);
    }

    fn assert_no_new_entry(&self) {
        match self
            .receiver
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_millis(150))
        {
            Ok(total) => panic!("unexpected handler entry {total} while permit was held"),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(error) => panic!("entry channel closed unexpectedly: {error}"),
        }
        assert_eq!(self.max_active.load(Ordering::SeqCst), 1);
    }
}
