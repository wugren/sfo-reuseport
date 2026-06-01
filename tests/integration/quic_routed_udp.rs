use std::net::UdpSocket as StdUdpSocket;
#[cfg(feature = "quinn")]
use std::fmt;
#[cfg(feature = "quinn")]
use std::pin::Pin;
#[cfg(feature = "quinn")]
use std::task::{Context, Poll};
use std::sync::{Arc, Mutex, Once};
#[cfg(feature = "quinn")]
use std::sync::mpsc;
use std::time::Duration;

use sfo_reuseport::{
    Error, QuicServer, ServerRuntime, ServerRuntimeConfig, UdpServiceConfig,
};
#[cfg(feature = "quinn")]
use sfo_reuseport::QuicCidGenerator;
#[cfg(feature = "quinn")]
use sfo_reuseport::UdpSocket;

static QUIC_TEST_LOCK: Mutex<()> = Mutex::new(());
static DISABLE_QUIC_BPF: Once = Once::new();

fn disable_quic_bpf_for_test() {
    DISABLE_QUIC_BPF.call_once(|| unsafe {
        std::env::set_var("SFO_REUSEPORT_DISABLE_QUIC_BPF", "1");
    });
}

fn free_addr() -> std::net::SocketAddr {
    let socket = StdUdpSocket::bind("127.0.0.1:0").unwrap();
    socket.local_addr().unwrap()
}

#[tokio::test]
async fn quic_server_serve_delivers_long_header_dcid_and_sends_response() {
    disable_quic_bpf_for_test();
    let _guard = QUIC_TEST_LOCK.lock().unwrap();
    let addr = free_addr();
    let server = tokio::spawn(async move {
        let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(3))?;
        QuicServer::serve(
            &runtime,
            UdpServiceConfig::new(addr),
            |socket, meta, payload| async move {
                assert_eq!(payload, [0xe0, 0, 0, 0, 1, 4, 0, 2, 9, 9]);
                socket.send_to(b"quic-ok", meta.peer_addr.unwrap()).await?;
                Ok(())
            },
        )?;
        std::future::pending::<Result<(), Error>>().await
    });

    let client = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client
        .send_to(&[0xe0, 0, 0, 0, 1, 4, 0, 2, 9, 9], addr)
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
    disable_quic_bpf_for_test();
    let _guard = QUIC_TEST_LOCK.lock().unwrap();
    let addr = free_addr();
    let seen_worker = Arc::new(Mutex::new(None));
    let handler_seen_worker = Arc::clone(&seen_worker);
    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(3)).unwrap();
    let server = QuicServer::serve(
        &runtime,
        UdpServiceConfig::new(addr),
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
    let packet = [0xe0, 0, 0, 0, 1, 4, 0, 2, 9, 9];
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
            server.close().unwrap();
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    server.close().unwrap();
    panic!("quic routed udp handler did not record a worker");
}

#[tokio::test]
async fn quic_routed_udp_initial_prefix_matches_followup_prefix() {
    disable_quic_bpf_for_test();
    let _guard = QUIC_TEST_LOCK.lock().unwrap();
    let addr = free_addr();
    let seen_workers = Arc::new(Mutex::new(Vec::new()));
    let handler_seen_workers = Arc::clone(&seen_workers);
    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(3)).unwrap();
    let server = QuicServer::serve(
        &runtime,
        UdpServiceConfig::new(addr),
        move |socket, meta, _payload| {
            let handler_seen_workers = Arc::clone(&handler_seen_workers);
            async move {
                let thread = std::thread::current();
                let name = thread.name().unwrap_or_default().to_string();
                handler_seen_workers.lock().unwrap().push(name);
                socket.send_to(b"ok", meta.peer_addr.unwrap()).await?;
                Ok(())
            }
        },
    )
    .unwrap();

    let expected_worker = 2;
    let client = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let initial = [0xc0, 0, 0, 0, 1, 8, 0, 2, 6, 5, 4, 3, 2, 1];
    client.send_to(&initial, addr).await.unwrap();

    let mut buffer = [0_u8; 8];
    let (len, _) = tokio::time::timeout(Duration::from_secs(2), client.recv_from(&mut buffer))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&buffer[..len], b"ok");

    let followup = [
        0xe0,
        0,
        0,
        0,
        1,
        8,
        0,
        expected_worker as u8,
        9,
        9,
        9,
        9,
        9,
        9,
        9,
    ];
    client.send_to(&followup, addr).await.unwrap();
    let (len, _) = tokio::time::timeout(Duration::from_secs(2), client.recv_from(&mut buffer))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&buffer[..len], b"ok");

    for _ in 0..100 {
        let workers = seen_workers.lock().unwrap().clone();
        if workers.len() >= 2 {
            let suffix = format!("worker-{expected_worker}");
            assert!(workers.iter().all(|name| name.ends_with(&suffix)), "{workers:?}");
            server.close().unwrap();
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    server.close().unwrap();
    panic!("quic routed udp handler did not record both workers");
}

#[tokio::test]
async fn quic_routed_udp_drops_invalid_route_key() {
    disable_quic_bpf_for_test();
    let _guard = QUIC_TEST_LOCK.lock().unwrap();
    let addr = free_addr();
    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(2)).unwrap();
    let server = QuicServer::serve(
        &runtime,
        UdpServiceConfig::new(addr),
        |_socket, _meta, _payload| async {
            panic!("invalid QUIC route key should not reach handler");
        },
    )
    .unwrap();

    let client = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client.send_to(&[0xc0, 0, 0, 0, 1, 4, 1], addr).await.unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    server.close().unwrap();
}

#[tokio::test]
async fn quic_routed_udp_supports_full_16_bit_worker_index_prefix() {
    disable_quic_bpf_for_test();
    let _guard = QUIC_TEST_LOCK.lock().unwrap();
    let addr = free_addr();
    let seen_worker = Arc::new(Mutex::new(None));
    let handler_seen_worker = Arc::clone(&seen_worker);
    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(4)).unwrap();
    let server = QuicServer::serve(
        &runtime,
        UdpServiceConfig::new(addr),
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
    client
        .send_to(&[0xe0, 0, 0, 0, 1, 4, 0x01, 0x03, 9, 9], addr)
        .await
        .unwrap();

    let mut buffer = [0_u8; 8];
    let (len, _) = tokio::time::timeout(Duration::from_secs(2), client.recv_from(&mut buffer))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&buffer[..len], b"ok");

    for _ in 0..100 {
        if let Some(name) = seen_worker.lock().unwrap().clone() {
            assert!(name.ends_with("worker-3"), "unexpected worker thread: {name}");
            server.close().unwrap();
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    server.close().unwrap();
    panic!("quic routed udp handler did not record a worker");
}

#[cfg(feature = "quinn")]
#[derive(Clone)]
struct SfoQuinnUdpSocket {
    socket: UdpSocket,
}

#[cfg(feature = "quinn")]
impl SfoQuinnUdpSocket {
    fn new(socket: UdpSocket) -> Self {
        Self { socket }
    }
}

#[cfg(feature = "quinn")]
impl fmt::Debug for SfoQuinnUdpSocket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SfoQuinnUdpSocket")
            .field("local_addr", &self.socket.local_addr().ok())
            .finish()
    }
}

#[cfg(feature = "quinn")]
impl quinn::AsyncUdpSocket for SfoQuinnUdpSocket {
    fn create_io_poller(self: Arc<Self>) -> Pin<Box<dyn quinn::UdpPoller>> {
        Box::pin(SfoQuinnUdpPoller { socket: self })
    }

    fn try_send(&self, transmit: &quinn::udp::Transmit<'_>) -> std::io::Result<()> {
        if let Some(segment_size) = transmit.segment_size {
            for segment in transmit.contents.chunks(segment_size) {
                self.socket.try_send_to(segment, transmit.destination)?;
            }
            return Ok(());
        }

        self.socket
            .try_send_to(transmit.contents, transmit.destination)
            .map(|_| ())
    }

    fn poll_recv(
        &self,
        cx: &mut Context<'_>,
        buffers: &mut [std::io::IoSliceMut<'_>],
        meta: &mut [quinn::udp::RecvMeta],
    ) -> Poll<std::io::Result<usize>> {
        if buffers.is_empty() || meta.is_empty() {
            return Poll::Ready(Ok(0));
        }

        match self.socket.poll_recv_from_vectored(cx, buffers) {
            Poll::Ready(Ok((len, peer_addr))) => {
                meta[0] = quinn::udp::RecvMeta {
                    addr: peer_addr,
                    len,
                    stride: len,
                    ecn: None,
                    dst_ip: None,
                };
                Poll::Ready(Ok(1))
            }
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Pending => Poll::Pending,
        }
    }

    fn local_addr(&self) -> std::io::Result<std::net::SocketAddr> {
        self.socket
            .local_addr()
            .map_err(|error| std::io::Error::other(error.to_string()))
    }

    fn may_fragment(&self) -> bool {
        true
    }
}

#[cfg(feature = "quinn")]
#[derive(Debug)]
struct SfoQuinnUdpPoller {
    socket: Arc<SfoQuinnUdpSocket>,
}

#[cfg(feature = "quinn")]
impl quinn::UdpPoller for SfoQuinnUdpPoller {
    fn poll_writable(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        self.get_mut().socket.socket.poll_send_ready(cx)
    }
}

#[cfg(feature = "quinn")]
#[derive(Debug)]
struct WorkerShardCidGenerator {
    generator: QuicCidGenerator,
}

#[cfg(feature = "quinn")]
impl WorkerShardCidGenerator {
    fn new(worker_id: usize) -> Self {
        Self {
            generator: QuicCidGenerator::for_worker(worker_id).unwrap(),
        }
    }
}

#[cfg(feature = "quinn")]
impl quinn::ConnectionIdGenerator for WorkerShardCidGenerator {
    fn generate_cid(&mut self) -> quinn::ConnectionId {
        let cid = self.generator.generate().unwrap();
        quinn::ConnectionId::new(&cid)
    }

    fn cid_len(&self) -> usize {
        self.generator.cid_len()
    }

    fn cid_lifetime(&self) -> Option<Duration> {
        None
    }
}

#[cfg(feature = "quinn")]
#[derive(Debug)]
struct QuinnServed {
    server_id: usize,
    worker_id: usize,
    request: Vec<u8>,
}

#[cfg(feature = "quinn")]
#[derive(Clone, Debug)]
struct QuinnWorkerEndpoint {
    server_id: usize,
    worker_id: usize,
    command_tx: tokio::sync::mpsc::UnboundedSender<QuinnCommand>,
}

#[cfg(feature = "quinn")]
#[derive(Debug)]
struct QuinnCommand {
    addr: std::net::SocketAddr,
    request: Vec<u8>,
    reply_tx: tokio::sync::oneshot::Sender<Vec<u8>>,
}

#[cfg(feature = "quinn")]
fn quinn_server_config() -> (
    quinn::ServerConfig,
    quinn::rustls::pki_types::CertificateDer<'static>,
) {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
    let cert_der = quinn::rustls::pki_types::CertificateDer::from(cert.cert);
    let key_der =
        quinn::rustls::pki_types::PrivatePkcs8KeyDer::from(cert.signing_key.serialize_der());
    let mut config = quinn::ServerConfig::with_single_cert(vec![cert_der.clone()], key_der.into())
        .unwrap();
    Arc::get_mut(&mut config.transport)
        .unwrap()
        .max_concurrent_uni_streams(0_u8.into());
    (config, cert_der)
}

#[cfg(feature = "quinn")]
fn quinn_client_config(
    cert_der: quinn::rustls::pki_types::CertificateDer<'static>,
) -> quinn::ClientConfig {
    let mut roots = quinn::rustls::RootCertStore::empty();
    roots.add(cert_der).unwrap();
    quinn::ClientConfig::with_root_certificates(Arc::new(roots)).unwrap()
}

#[cfg(feature = "quinn")]
async fn run_quinn_worker_endpoint(
    server_id: usize,
    worker_id: usize,
    socket: UdpSocket,
    server_config: quinn::ServerConfig,
    client_config: quinn::ClientConfig,
    ready_tx: mpsc::Sender<QuinnWorkerEndpoint>,
    served_tx: mpsc::Sender<QuinnServed>,
) -> Result<(), Error> {
    let mut endpoint_config = quinn::EndpointConfig::default();
    endpoint_config.cid_generator(move || Box::new(WorkerShardCidGenerator::new(worker_id)));
    let mut endpoint = quinn::Endpoint::new_with_abstract_socket(
        endpoint_config,
        Some(server_config),
        Arc::new(SfoQuinnUdpSocket::new(socket)),
        Arc::new(quinn::TokioRuntime),
    )
    .map_err(|error| Error::Runtime(error.to_string()))?;
    endpoint.set_default_client_config(client_config.clone());
    let mut outbound_endpoint = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap())
        .map_err(|error| Error::Runtime(error.to_string()))?;
    outbound_endpoint.set_default_client_config(client_config);
    let (command_tx, mut command_rx) = tokio::sync::mpsc::unbounded_channel();
    ready_tx
        .send(QuinnWorkerEndpoint {
            server_id,
            worker_id,
            command_tx,
        })
        .map_err(|error| Error::Runtime(error.to_string()))?;

    loop {
        tokio::select! {
            incoming = endpoint.accept() => {
                let Some(incoming) = incoming else {
                    outbound_endpoint.close(0_u32.into(), b"done");
                    return Ok(());
                };
                handle_quinn_incoming(server_id, worker_id, incoming, &served_tx).await?;
            }
            command = command_rx.recv() => {
                let Some(command) = command else {
                    endpoint.close(0_u32.into(), b"done");
                    outbound_endpoint.close(0_u32.into(), b"done");
                    return Ok(());
                };
                let response = quinn_request(&outbound_endpoint, command.addr, &command.request).await;
                let _ = command.reply_tx.send(response);
            }
        }
    }
}

#[cfg(feature = "quinn")]
async fn handle_quinn_incoming(
    server_id: usize,
    worker_id: usize,
    incoming: quinn::Incoming,
    served_tx: &mpsc::Sender<QuinnServed>,
) -> Result<(), Error> {
    let connection = incoming
        .await
        .map_err(|error| Error::Runtime(error.to_string()))?;
    let (mut send, mut recv) = connection
        .accept_bi()
        .await
        .map_err(|error| Error::Runtime(error.to_string()))?;
    let request = recv
        .read_to_end(64 * 1024)
        .await
        .map_err(|error| Error::Runtime(error.to_string()))?;
    let response = format!("server-{server_id}-worker-{worker_id}-ok");
    send.write_all(response.as_bytes())
        .await
        .map_err(|error| Error::Runtime(error.to_string()))?;
    send.finish()
        .map_err(|error| Error::Runtime(error.to_string()))?;
    served_tx
        .send(QuinnServed {
            server_id,
            worker_id,
            request,
        })
        .map_err(|error| Error::Runtime(error.to_string()))?;
    tokio::time::sleep(Duration::from_millis(100)).await;
    Ok(())
}

#[cfg(feature = "quinn")]
async fn wait_for_quinn_workers_ready(
    ready_rx: &mpsc::Receiver<QuinnWorkerEndpoint>,
    expected: usize,
) -> Vec<QuinnWorkerEndpoint> {
    let mut ready = Vec::with_capacity(expected);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while ready.len() < expected {
        while let Ok(item) = ready_rx.try_recv() {
            ready.push(item);
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for quinn worker endpoints: {ready:?}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    ready
}

#[cfg(feature = "quinn")]
async fn quinn_request(
    endpoint: &quinn::Endpoint,
    addr: std::net::SocketAddr,
    request: &[u8],
) -> Vec<u8> {
    let connection = endpoint
        .connect(addr, "localhost")
        .unwrap()
        .await
        .unwrap();
    let (mut send, mut recv) = connection.open_bi().await.unwrap();
    send.write_all(request).await.unwrap();
    send.finish().unwrap();
    let response = recv.read_to_end(64 * 1024).await.unwrap();
    connection.close(0_u32.into(), b"done");
    response
}

#[cfg(feature = "quinn")]
async fn quinn_worker_request(
    worker: &QuinnWorkerEndpoint,
    addr: std::net::SocketAddr,
    request: &'static [u8],
) -> Result<Vec<u8>, Error> {
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    worker
        .command_tx
        .send(QuinnCommand {
            addr,
            request: request.to_vec(),
            reply_tx,
        })
        .map_err(|error| Error::Runtime(error.to_string()))?;
    reply_rx
        .await
        .map_err(|error| Error::Runtime(error.to_string()))
}

#[cfg(feature = "quinn")]
async fn quinn_server_worker_request(
    workers: &[QuinnWorkerEndpoint],
    server_id: usize,
    addr: std::net::SocketAddr,
    request: &'static [u8],
) -> Vec<u8> {
    for worker in workers.iter().filter(|worker| worker.server_id == server_id) {
        match tokio::time::timeout(
            Duration::from_secs(5),
            quinn_worker_request(worker, addr, request),
        )
        .await
        {
            Ok(Ok(response)) => return response,
            Ok(Err(_)) | Err(_) => continue,
        }
    }

    panic!("no quinn worker endpoint completed request for server {server_id}");
}

#[cfg(feature = "quinn")]
async fn collect_quinn_served(
    served_rx: &mpsc::Receiver<QuinnServed>,
    expected: usize,
) -> Vec<QuinnServed> {
    let mut served = Vec::with_capacity(expected);
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while served.len() < expected {
        while let Ok(item) = served_rx.try_recv() {
            served.push(item);
        }
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for quinn server requests: {served:?}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    served
}

#[cfg(feature = "quinn")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn quic_endpoint_servers_connect_to_each_other_and_exchange_data() {
    disable_quic_bpf_for_test();
    let addr_a = free_addr();
    let addr_b = free_addr();
    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(2)).unwrap();
    let (server_config, cert_der) = quinn_server_config();
    let client_config = quinn_client_config(cert_der);
    let (ready_tx, ready_rx) = mpsc::channel();
    let (served_tx, served_rx) = mpsc::channel();

    let server_a_config = server_config.clone();
    let server_a_client = client_config.clone();
    let server_a_ready = ready_tx.clone();
    let server_a_served = served_tx.clone();
    let server_a = QuicServer::serve_socket(
        &runtime,
        UdpServiceConfig::new(addr_a),
        move |socket, worker_id| {
            run_quinn_worker_endpoint(
                0,
                worker_id,
                socket,
                server_a_config.clone(),
                server_a_client.clone(),
                server_a_ready.clone(),
                server_a_served.clone(),
            )
        },
    )
    .unwrap();

    let server_b_config = server_config;
    let server_b_client = client_config;
    let server_b_ready = ready_tx;
    let server_b_served = served_tx;
    let server_b = QuicServer::serve_socket(
        &runtime,
        UdpServiceConfig::new(addr_b),
        move |socket, worker_id| {
            run_quinn_worker_endpoint(
                1,
                worker_id,
                socket,
                server_b_config.clone(),
                server_b_client.clone(),
                server_b_ready.clone(),
                server_b_served.clone(),
            )
        },
    )
    .unwrap();

    let mut ready = wait_for_quinn_workers_ready(&ready_rx, 4).await;
    ready.sort_by_key(|endpoint| (endpoint.server_id, endpoint.worker_id));
    tokio::time::sleep(Duration::from_millis(50)).await;
    let mut ready_ids = ready
        .iter()
        .map(|endpoint| (endpoint.server_id, endpoint.worker_id))
        .collect::<Vec<_>>();
    ready_ids.sort_unstable();
    assert_eq!(ready_ids, vec![(0, 0), (0, 1), (1, 0), (1, 1)]);
    let reply_from_b =
        quinn_server_worker_request(&ready, 0, addr_b, b"hello-from-a").await;
    let reply_from_a =
        quinn_server_worker_request(&ready, 1, addr_a, b"hello-from-b").await;

    assert!(String::from_utf8(reply_from_b).unwrap().starts_with("server-1-worker-"));
    assert!(String::from_utf8(reply_from_a).unwrap().starts_with("server-0-worker-"));

    let mut served = collect_quinn_served(&served_rx, 2).await;
    served.sort_by_key(|item| item.server_id);
    assert_eq!(served[0].server_id, 0);
    assert!(served[0].worker_id < 2);
    assert_eq!(served[0].request, b"hello-from-b");
    assert_eq!(served[1].server_id, 1);
    assert!(served[1].worker_id < 2);
    assert_eq!(served[1].request, b"hello-from-a");

    drop(ready);
    tokio::time::sleep(Duration::from_millis(50)).await;
    server_a.close().unwrap();
    server_b.close().unwrap();
}
