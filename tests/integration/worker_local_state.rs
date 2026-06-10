use std::cell::RefCell;
use std::net::TcpListener;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use sfo_reuseport::{Error, ServerRuntime, ServerRuntimeConfig, TcpServer, TcpServiceConfig};

#[derive(Clone)]
struct WorkerState {
    inner: Rc<WorkerStateInner>,
}

struct WorkerStateInner {
    hits: RefCell<usize>,
    drops: Arc<AtomicUsize>,
}

impl Drop for WorkerStateInner {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

fn free_addr() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap()
}

async fn connect_with_retry(addr: std::net::SocketAddr) -> tokio::net::TcpStream {
    for _ in 0..50 {
        if let Ok(stream) = tokio::net::TcpStream::connect(addr).await {
            return stream;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("server should accept loopback connection");
}

async fn wait_for_atomic_at_least(value: &AtomicUsize, expected: usize) {
    for _ in 0..200 {
        if value.load(Ordering::SeqCst) >= expected {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("atomic value did not reach {expected}");
}

#[tokio::test]
async fn tcp_serve_with_state_reuses_mutable_worker_state_and_releases_it_on_close() {
    let addr = free_addr();
    let constructed = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let (hit_tx, mut hit_rx) = tokio::sync::mpsc::unbounded_channel();

    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(1)).unwrap();
    let server = TcpServer::serve_with_state(
        &runtime,
        TcpServiceConfig::new(addr),
        {
            let constructed = Arc::clone(&constructed);
            let drops = Arc::clone(&drops);
            move || {
                constructed.fetch_add(1, Ordering::SeqCst);
                WorkerState {
                    inner: Rc::new(WorkerStateInner {
                        hits: RefCell::new(0),
                        drops: Arc::clone(&drops),
                    }),
                }
            }
        },
        move |state, _stream| {
            let hit_tx = hit_tx.clone();
            async move {
                let hit = {
                    let mut hits = state.inner.hits.borrow_mut();
                    *hits += 1;
                    *hits
                };
                hit_tx.send(hit).unwrap();
                Ok::<(), Error>(())
            }
        },
    )
    .unwrap();

    let first = connect_with_retry(addr).await;
    drop(first);
    let second = connect_with_retry(addr).await;
    drop(second);

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
    assert_eq!(constructed.load(Ordering::SeqCst), 1);

    server.close().unwrap();
    drop(server);
    wait_for_atomic_at_least(&drops, 1).await;
}
