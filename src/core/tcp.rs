use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use crate::core::{
    Error, HandlerFuture, HandlerFutureBox, PacketMeta, ServerRuntime, ServiceConfig,
    linux_reuseport_select,
};
use crate::platform;
use crate::runtime::{self, TcpStream};

type TcpHandler = Arc<dyn Fn(TcpStream) -> HandlerFutureBox + Send + Sync>;

#[derive(Clone)]
pub struct TcpServer {
    state: Arc<TcpServerState>,
}

struct TcpServerState {
    tasks: Mutex<Vec<runtime::TaskHandle>>,
}

impl TcpServer {
    pub fn serve<F, Fut>(
        runtime: &ServerRuntime,
        config: ServiceConfig,
        handler: F,
    ) -> Result<Self, Error>
    where
        F: Fn(TcpStream) -> Fut + Clone + Send + Sync + 'static,
        Fut: HandlerFuture,
    {
        config.validate()?;
        let server = Self {
            state: Arc::new(TcpServerState::new()),
        };
        if !platform::supports_reuse_port_balancing() {
            add_simulated_listener(runtime, config, handler, Arc::clone(&server.state))?;
        } else {
            add_reuse_port_listener(runtime, config, handler, Arc::clone(&server.state))?;
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
        if let Ok(mut tasks) = self.tasks.lock() {
            for task in tasks.drain(..) {
                task.cancel();
            }
        }
    }
}

fn add_reuse_port_listener<F, Fut>(
    runtime: &ServerRuntime,
    service_config: ServiceConfig,
    handler: F,
    state: Arc<TcpServerState>,
) -> Result<(), Error>
where
    F: Fn(TcpStream) -> Fut + Clone + Send + Sync + 'static,
    Fut: HandlerFuture,
{
    let listeners = platform::bind_tcp_workers(&service_config, runtime.worker_count())?;
    if listeners.is_empty() {
        return Err(Error::InvalidConfig(
            "worker count must be greater than zero".to_string(),
        ));
    }
    let runtime_active = runtime.active_flag();

    let handler = tcp_handler(handler);
    for (worker_id, listener) in listeners.into_iter().enumerate() {
        let runtime_active = Arc::clone(&runtime_active);
        let handler = Arc::clone(&handler);
        let task = runtime.submit_to_worker(worker_id, move || async move {
            let Ok(listener) = runtime::tcp_listener_from_std(listener).map_err(Error::from)
            else {
                return;
            };
            tcp_listener_loop(listener, runtime_active, handler).await;
        })?;
        state.register_task(task)?;
    }

    Ok(())
}

fn add_simulated_listener<F, Fut>(
    runtime: &ServerRuntime,
    service_config: ServiceConfig,
    handler: F,
    state: Arc<TcpServerState>,
) -> Result<(), Error>
where
    F: Fn(TcpStream) -> Fut + Clone + Send + Sync + 'static,
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

    let worker_executors = runtime.worker_executors();
    let worker_count = worker_executors.len();
    let handler = tcp_handler(handler);
    let task = runtime.submit_to_worker(0, move || async move {
        let Ok(listener) = runtime::tcp_listener_from_std(listener).map_err(Error::from) else {
            return;
        };
        simulated_tcp_accept_loop(
            listener,
            runtime_active,
            worker_executors,
            worker_count,
            addr,
            handler,
        )
        .await;
    })?;
    state.register_task(task)?;

    Ok(())
}

fn tcp_handler<F, Fut>(handler: F) -> TcpHandler
where
    F: Fn(TcpStream) -> Fut + Clone + Send + Sync + 'static,
    Fut: HandlerFuture,
{
    Arc::new(move |stream| {
        let future = handler.clone()(stream);
        Box::pin(future) as HandlerFutureBox
    })
}

async fn tcp_listener_loop(
    listener: runtime::TcpListener,
    runtime_active: Arc<AtomicBool>,
    handler: TcpHandler,
) {
    while is_active(&runtime_active) {
        let Ok((stream, _)) = listener.accept().await else {
            break;
        };
        if !is_active(&runtime_active) {
            break;
        }
        if handler(stream).await.is_err() {
            break;
        }
    }
}

async fn simulated_tcp_accept_loop(
    listener: runtime::TcpListener,
    runtime_active: Arc<AtomicBool>,
    worker_executors: Vec<runtime::ExecutorHandle>,
    worker_count: usize,
    local_addr: std::net::SocketAddr,
    handler: TcpHandler,
) {
    while is_active(&runtime_active) {
        let Ok((stream, peer_addr)) = listener.accept().await else {
            break;
        };
        if !is_active(&runtime_active) {
            break;
        }
        let meta = PacketMeta {
            peer_addr: Some(peer_addr),
            local_addr: Some(local_addr),
        };
        let Ok(worker_id) = select_simulated_tcp_worker(meta, worker_count) else {
            break;
        };
        let Some(executor) = worker_executors.get(worker_id) else {
            break;
        };
        let handler = Arc::clone(&handler);
        let submit_result = ServerRuntime::submit_to_executor(executor, move || async move {
            let _ = handler(stream).await;
        });
        if submit_result.is_err() {
            break;
        }
    }
}

fn is_active(runtime_active: &AtomicBool) -> bool {
    runtime_active.load(Ordering::SeqCst)
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
