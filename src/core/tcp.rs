use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use crate::core::{
    ConcurrencyPermit, Error, HandlerFuture, PacketMeta, ServerRuntime, TcpServiceConfig,
    WorkerConcurrencyLimit, linux_reuseport_select,
};
use crate::platform;
use crate::runtime::{self, TcpStream};

const FALLBACK_TCP_WORK_QUEUE_CAPACITY: usize = 4096;

struct TcpWorkItem {
    stream: TcpStream,
    permit: ConcurrencyPermit,
}

#[cfg(feature = "runtime-async-std")]
type TcpWorkSender = async_std::channel::Sender<TcpWorkItem>;
#[cfg(feature = "runtime-async-std")]
type TcpWorkReceiver = async_std::channel::Receiver<TcpWorkItem>;

#[cfg(feature = "runtime-tokio")]
type TcpWorkSender = tokio::sync::mpsc::Sender<TcpWorkItem>;
#[cfg(feature = "runtime-tokio")]
type TcpWorkReceiver = tokio::sync::mpsc::Receiver<TcpWorkItem>;

#[derive(Clone)]
pub struct TcpServer {
    state: Arc<TcpServerState>,
}

struct TcpServerState {
    active: Arc<AtomicBool>,
    tasks: Mutex<Vec<runtime::TaskHandle>>,
}

impl TcpServer {
    pub fn serve<F, Fut>(
        runtime: &ServerRuntime,
        config: TcpServiceConfig,
        handler: F,
    ) -> Result<Self, Error>
    where
        F: Send + Sync + 'static + Fn(TcpStream) -> Fut,
        Fut: HandlerFuture,
    {
        Self::serve_with_state(runtime, config, || (), move |(), stream| handler(stream))
    }

    pub fn serve_with_state<S, SF, F, Fut>(
        runtime: &ServerRuntime,
        config: TcpServiceConfig,
        state_factory: SF,
        handler: F,
    ) -> Result<Self, Error>
    where
        S: Clone + 'static,
        SF: Send + Sync + 'static + Fn() -> S,
        F: Send + Sync + 'static + Fn(S, TcpStream) -> Fut,
        Fut: HandlerFuture,
    {
        config.validate()?;
        let server = Self {
            state: Arc::new(TcpServerState::new()),
        };
        if !platform::capabilities().reuse_port_balancing {
            add_simulated_listener_with_state(
                runtime,
                config,
                state_factory,
                handler,
                Arc::clone(&server.state),
            )?;
        } else {
            add_reuse_port_listener_with_state(
                runtime,
                config,
                state_factory,
                handler,
                Arc::clone(&server.state),
            )?;
        }
        Ok(server)
    }

    pub fn close(&self) -> Result<(), Error> {
        self.state.close();
        Ok(())
    }
}

impl TcpServerState {
    fn new() -> Self {
        Self {
            active: Arc::new(AtomicBool::new(true)),
            tasks: Mutex::new(Vec::new()),
        }
    }

    fn register_task(&self, task: runtime::TaskHandle) -> Result<(), Error> {
        self.tasks
            .lock()
            .map_err(|_| Error::Runtime("tcp task registry lock poisoned".to_string()))?
            .push(task);
        Ok(())
    }

    fn close(&self) {
        self.active.store(false, Ordering::SeqCst);
        if let Ok(mut tasks) = self.tasks.lock() {
            for task in tasks.drain(..) {
                task.cancel();
            }
        }
    }

    fn active_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.active)
    }
}

fn add_reuse_port_listener_with_state<S, SF, F, Fut>(
    runtime: &ServerRuntime,
    service_config: TcpServiceConfig,
    state_factory: SF,
    handler: F,
    state: Arc<TcpServerState>,
) -> Result<(), Error>
where
    S: Clone + 'static,
    SF: Send + Sync + 'static + Fn() -> S,
    F: Send + Sync + 'static + Fn(S, TcpStream) -> Fut,
    Fut: HandlerFuture,
{
    let listeners = platform::bind_tcp_workers(&service_config, runtime.worker_count())?;
    if listeners.is_empty() {
        return Err(Error::InvalidConfig(
            "worker count must be greater than zero".to_string(),
        ));
    }
    let runtime_active = runtime.active_flag();
    let max_concurrency = service_config.max_concurrency_per_worker;

    let handler = Arc::new(handler);
    let state_factory = Arc::new(state_factory);
    for (worker_id, listener) in listeners.into_iter().enumerate() {
        let runtime_active = Arc::clone(&runtime_active);
        let server_active = state.active_flag();
        let handler = Arc::clone(&handler);
        let state_factory = Arc::clone(&state_factory);
        let limit = WorkerConcurrencyLimit::new(max_concurrency);
        let task = runtime.submit_to_worker(worker_id, move || Box::pin(async move {
            let Ok(listener) = runtime::tcp_listener_from_std(listener).map_err(Error::from)
            else {
                return;
            };
            let worker_state = state_factory();
            tcp_listener_loop_with_state(
                listener,
                runtime_active,
                server_active,
                handler,
                limit,
                worker_state,
            )
            .await;
        }))?;
        state.register_task(task)?;
    }

    Ok(())
}

fn add_simulated_listener_with_state<S, SF, F, Fut>(
    runtime: &ServerRuntime,
    service_config: TcpServiceConfig,
    state_factory: SF,
    handler: F,
    state: Arc<TcpServerState>,
) -> Result<(), Error>
where
    S: Clone + 'static,
    SF: Send + Sync + 'static + Fn() -> S,
    F: Send + Sync + 'static + Fn(S, TcpStream) -> Fut,
    Fut: HandlerFuture,
{
    if !runtime::SUPPORTS_USERSPACE_REUSEPORT_SIMULATION {
        return Err(Error::UnsupportedPlatformOption(
            "selected runtime requires native reuse-port worker sockets".to_string(),
        ));
    }

    let listener = platform::bind_tcp(&service_config)?;
    let addr = listener.local_addr().map_err(Error::from)?;
    let runtime_active = runtime.active_flag();
    let server_active = state.active_flag();

    let worker_executors = runtime.worker_executors();
    let worker_count = worker_executors.len();
    let limits = worker_limits(worker_count, service_config.max_concurrency_per_worker);
    let handler = Arc::new(handler);
    let state_factory = Arc::new(state_factory);
    let mut senders = Vec::with_capacity(worker_count);
    for worker_id in 0..worker_count {
        let (sender, receiver) = tcp_work_channel(FALLBACK_TCP_WORK_QUEUE_CAPACITY);
        senders.push(sender);
        let Some(executor) = worker_executors.get(worker_id).cloned() else {
            return Err(Error::InvalidConfig("worker index is out of range".to_string()));
        };
        let handler = Arc::clone(&handler);
        let state_factory = Arc::clone(&state_factory);
        let server_active = Arc::clone(&server_active);
        let task = ServerRuntime::submit_to_executor(
            &executor,
            move || Box::pin(async move {
                let worker_state = state_factory();
                tcp_state_dispatch_loop(receiver, server_active, handler, worker_state).await;
            }),
        )?;
        state.register_task(task)?;
    }
    let task = runtime.submit_to_worker(0, move || Box::pin(async move {
        let Ok(listener) = runtime::tcp_listener_from_std(listener).map_err(Error::from) else {
            return;
        };
        simulated_tcp_accept_loop(
            listener,
            runtime_active,
            server_active,
            worker_count,
            addr,
            senders,
            limits,
        )
        .await;
    }))?;
    state.register_task(task)?;

    Ok(())
}

async fn tcp_listener_loop_with_state<S, F, Fut>(
    listener: runtime::TcpListener,
    runtime_active: Arc<AtomicBool>,
    server_active: Arc<AtomicBool>,
    handler: Arc<F>,
    limit: WorkerConcurrencyLimit,
    worker_state: S,
) where
    S: Clone + 'static,
    F: Send + Sync + 'static + Fn(S, TcpStream) -> Fut,
    Fut: HandlerFuture,
{
    while is_active(&runtime_active, &server_active) {
        let permit = limit.acquire().await;
        if !is_active(&runtime_active, &server_active) {
            break;
        }
        let Ok((stream, _)) = listener.accept().await else {
            break;
        };
        if !is_active(&runtime_active, &server_active) {
            break;
        }
        if spawn_tcp_handler_with_state(
            worker_state.clone(),
            stream,
            Arc::clone(&handler),
            permit,
        )
        .is_err()
        {
            break;
        }
    }
}

async fn simulated_tcp_accept_loop(
    listener: runtime::TcpListener,
    runtime_active: Arc<AtomicBool>,
    server_active: Arc<AtomicBool>,
    worker_count: usize,
    local_addr: std::net::SocketAddr,
    senders: Vec<TcpWorkSender>,
    limits: Vec<WorkerConcurrencyLimit>,
) {
    while is_active(&runtime_active, &server_active) {
        let Ok((stream, peer_addr)) = listener.accept().await else {
            break;
        };
        if !is_active(&runtime_active, &server_active) {
            break;
        }
        let meta = PacketMeta {
            peer_addr: Some(peer_addr),
            local_addr: Some(local_addr),
        };
        let Ok(worker_id) = select_simulated_tcp_worker(meta, worker_count) else {
            break;
        };
        let Some(limit) = limits.get(worker_id) else {
            break;
        };
        let Some(permit) = limit.try_acquire() else {
            drop(stream);
            continue;
        };
        let Some(sender) = senders.get(worker_id) else {
            break;
        };
        if send_tcp_work(sender, TcpWorkItem { stream, permit }).await.is_err() {
            break;
        }
    }
}

async fn tcp_state_dispatch_loop<S, F, Fut>(
    mut receiver: TcpWorkReceiver,
    server_active: Arc<AtomicBool>,
    handler: Arc<F>,
    worker_state: S,
) where
    S: Clone + 'static,
    F: Send + Sync + 'static + Fn(S, TcpStream) -> Fut,
    Fut: HandlerFuture,
{
    while server_active.load(Ordering::SeqCst) {
        let Some(work) = recv_tcp_work(&mut receiver).await else {
            break;
        };
        if !server_active.load(Ordering::SeqCst) {
            break;
        }
        if spawn_tcp_handler_with_state(
            worker_state.clone(),
            work.stream,
            Arc::clone(&handler),
            work.permit,
        )
        .is_err()
        {
            break;
        }
    }
}

fn is_active(runtime_active: &AtomicBool, server_active: &AtomicBool) -> bool {
    runtime_active.load(Ordering::SeqCst) && server_active.load(Ordering::SeqCst)
}

fn worker_limits(worker_count: usize, max: Option<usize>) -> Vec<WorkerConcurrencyLimit> {
    (0..worker_count)
        .map(|_| WorkerConcurrencyLimit::new(max))
        .collect()
}

fn spawn_tcp_handler_with_state<S, F, Fut>(
    state: S,
    stream: TcpStream,
    handler: Arc<F>,
    permit: ConcurrencyPermit,
) -> Result<(), Error>
where
    S: Clone + 'static,
    F: Send + Sync + 'static + Fn(S, TcpStream) -> Fut,
    Fut: HandlerFuture,
{
    runtime::spawn_local(async move {
        let _permit = permit;
        let _ = handler(state, stream).await;
    })
    .map(|_| ())
    .map_err(Error::from)
}

#[cfg(feature = "runtime-async-std")]
fn tcp_work_channel(capacity: usize) -> (TcpWorkSender, TcpWorkReceiver) {
    async_std::channel::bounded(capacity)
}

#[cfg(feature = "runtime-tokio")]
fn tcp_work_channel(capacity: usize) -> (TcpWorkSender, TcpWorkReceiver) {
    tokio::sync::mpsc::channel(capacity)
}

#[cfg(feature = "runtime-async-std")]
async fn send_tcp_work(sender: &TcpWorkSender, work: TcpWorkItem) -> Result<(), ()> {
    sender.send(work).await.map_err(|_| ())
}

#[cfg(feature = "runtime-tokio")]
async fn send_tcp_work(sender: &TcpWorkSender, work: TcpWorkItem) -> Result<(), ()> {
    sender.send(work).await.map_err(|_| ())
}

#[cfg(feature = "runtime-async-std")]
async fn recv_tcp_work(receiver: &mut TcpWorkReceiver) -> Option<TcpWorkItem> {
    receiver.recv().await.ok()
}

#[cfg(feature = "runtime-tokio")]
async fn recv_tcp_work(receiver: &mut TcpWorkReceiver) -> Option<TcpWorkItem> {
    receiver.recv().await
}

fn select_simulated_tcp_worker(
    meta: PacketMeta,
    worker_count: usize,
) -> Result<usize, Error> {
    if worker_count <= 1 {
        return linux_reuseport_select(meta, worker_count);
    }
    linux_reuseport_select(meta, worker_count - 1).map(|worker_id| worker_id + 1)
}
