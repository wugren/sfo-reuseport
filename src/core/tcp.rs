use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use crate::core::{
    Error, HandlerFuture, HandlerFutureBox, ListenerConfig, ListenerId, ListenerProtocol,
    PacketMeta, ServerRuntime, ServiceConfig, linux_reuseport_select,
};
use crate::platform;
use crate::runtime::{self, TcpStream};

type TcpHandler = Arc<dyn Fn(TcpStream) -> HandlerFutureBox + Send + Sync>;

pub struct TcpServer;

impl TcpServer {
    pub fn serve<F, Fut>(
        runtime: &ServerRuntime,
        config: ServiceConfig,
        handler: F,
    ) -> Result<(), Error>
    where
        F: Fn(TcpStream) -> Fut + Clone + Send + Sync + 'static,
        Fut: HandlerFuture,
    {
        config.validate()?;
        if !platform::supports_reuse_port_balancing() {
            add_simulated_listener(runtime, config, handler)?;
        } else {
            add_reuse_port_listener(runtime, config, handler)?;
        }
        Ok(())
    }
}

impl ServerRuntime {
    pub fn add_tcp_listener<F, Fut>(
        &self,
        config: ListenerConfig,
        handler: F,
    ) -> Result<ListenerId, Error>
    where
        F: Fn(TcpStream) -> Fut + Clone + Send + Sync + 'static,
        Fut: HandlerFuture,
    {
        add_tcp_listener(self, config, handler)
    }
}

pub(crate) fn add_tcp_listener<F, Fut>(
    runtime: &ServerRuntime,
    config: ListenerConfig,
    handler: F,
) -> Result<ListenerId, Error>
where
    F: Fn(TcpStream) -> Fut + Clone + Send + Sync + 'static,
    Fut: HandlerFuture,
{
    let service_config = runtime.service_config(config);
    service_config.validate()?;
    if !platform::supports_reuse_port_balancing() {
        return add_simulated_listener(runtime, service_config, handler);
    }

    add_reuse_port_listener(runtime, service_config, handler)
}

fn add_reuse_port_listener<F, Fut>(
    runtime: &ServerRuntime,
    service_config: ServiceConfig,
    handler: F,
) -> Result<ListenerId, Error>
where
    F: Fn(TcpStream) -> Fut + Clone + Send + Sync + 'static,
    Fut: HandlerFuture,
{
    let listeners = platform::bind_tcp_workers(&service_config, runtime.worker_count())?;
    let addr = listeners
        .first()
        .ok_or_else(|| Error::InvalidConfig("worker count must be greater than zero".to_string()))?
        .local_addr()
        .map_err(Error::from)?;
    let active = Arc::new(AtomicBool::new(true));
    let id = runtime.register_listener(ListenerProtocol::Tcp, addr, Arc::clone(&active))?;

    let handler = tcp_handler(handler);
    for (worker_id, listener) in listeners.into_iter().enumerate() {
        let active = Arc::clone(&active);
        let handler = Arc::clone(&handler);
        runtime.submit_to_worker(worker_id, move || async move {
            let Ok(listener) = runtime::tcp_listener_from_std(listener).map_err(Error::from)
            else {
                return;
            };
            tcp_listener_loop(listener, active, handler).await;
        })?;
    }

    Ok(id)
}

fn add_simulated_listener<F, Fut>(
    runtime: &ServerRuntime,
    service_config: ServiceConfig,
    handler: F,
) -> Result<ListenerId, Error>
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
    let active = Arc::new(AtomicBool::new(true));
    let id = runtime.register_listener(ListenerProtocol::Tcp, addr, Arc::clone(&active))?;

    let server_runtime = runtime.clone();
    let worker_count = server_runtime.worker_count();
    let handler = tcp_handler(handler);
    thread::Builder::new()
        .name("sfo-reuseport-simulated-tcp-accept".to_string())
        .spawn(move || {
            simulated_tcp_accept_loop(
                listener,
                active,
                server_runtime,
                worker_count,
                addr,
                handler,
            );
        })
        .map_err(Error::from)?;

    Ok(id)
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
    active: Arc<AtomicBool>,
    handler: TcpHandler,
) {
    while active.load(Ordering::SeqCst) {
        let Ok((stream, _)) = listener.accept().await else {
            if active.load(Ordering::SeqCst) {
                continue;
            }
            break;
        };
        if !active.load(Ordering::SeqCst) {
            break;
        }
        if handler(stream).await.is_err() {
            break;
        }
    }
}

fn simulated_tcp_accept_loop(
    listener: std::net::TcpListener,
    active: Arc<AtomicBool>,
    runtime: ServerRuntime,
    worker_count: usize,
    local_addr: std::net::SocketAddr,
    handler: TcpHandler,
) {
    if listener.set_nonblocking(true).is_err() {
        active.store(false, Ordering::SeqCst);
        return;
    }

    while active.load(Ordering::SeqCst) {
        let (stream, peer_addr) = match listener.accept() {
            Ok(accepted) => accepted,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(5));
                continue;
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => {
                if active.load(Ordering::SeqCst) {
                    thread::sleep(Duration::from_millis(5));
                    continue;
                }
                break;
            }
        };
        if !active.load(Ordering::SeqCst) {
            break;
        }
        let meta = PacketMeta {
            peer_addr: Some(peer_addr),
            local_addr: Some(local_addr),
        };
        let Ok(worker_id) = select_simulated_tcp_worker(meta, worker_count) else {
            break;
        };
        let handler = Arc::clone(&handler);
        let submit_result = runtime
            .submit_to_worker(worker_id, move || async move {
                let Ok(stream) = runtime::tcp_stream_from_std(stream).map_err(Error::from) else {
                    return;
                };
                let _ = handler(stream).await;
            });
        if submit_result.is_err() {
            break;
        }
    }
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
