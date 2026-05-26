use std::collections::HashMap;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::core::{Error, ListenerConfig, ServerRuntimeConfig, ServiceConfig, WorkerCount};
use crate::runtime::{self, IntoExecutorTask};

#[derive(Clone)]
pub struct ServerRuntime {
    inner: Arc<ServerRuntimeInner>,
}

struct ServerRuntimeInner {
    workers: Vec<WorkerHandle>,
    next_id: AtomicU64,
    listeners: Mutex<HashMap<ListenerId, ListenerControl>>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ListenerId(u64);

impl ListenerId {
    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ListenerProtocol {
    Tcp,
    Udp,
}

struct ListenerControl {
    protocol: ListenerProtocol,
    addr: SocketAddr,
    active: Arc<AtomicBool>,
}

impl ServerRuntime {
    pub fn start(config: ServerRuntimeConfig) -> Result<Self, Error> {
        config.validate()?;
        let count = match config.workers {
            WorkerCount::Default => num_cpus::get().max(1),
            WorkerCount::Fixed(workers) => workers,
        };
        let mut workers = Vec::with_capacity(count);
        for worker_id in 0..count {
            workers.push(
                start_worker(format!("sfo-reuseport-worker-{worker_id}")).map_err(Error::from)?,
            );
        }

        Ok(Self {
            inner: Arc::new(ServerRuntimeInner {
                workers,
                next_id: AtomicU64::new(1),
                listeners: Mutex::new(HashMap::new()),
            }),
        })
    }

    pub fn remove_listener(&self, id: ListenerId) -> Result<(), Error> {
        let control = self
            .inner
            .listeners
            .lock()
            .map_err(|_| Error::Runtime("listener registry lock poisoned".to_string()))?
            .remove(&id)
            .ok_or(Error::UnknownListener)?;
        control.active.store(false, Ordering::SeqCst);
        match control.protocol {
            ListenerProtocol::Tcp => wake_tcp(control.addr),
            ListenerProtocol::Udp => wake_udp(control.addr),
        }
        Ok(())
    }

    pub(crate) fn worker_count(&self) -> usize {
        self.inner.workers.len()
    }

    pub(crate) fn submit_to_worker<T>(&self, worker_id: usize, task: T) -> Result<(), Error>
    where
        T: IntoExecutorTask,
    {
        let worker = self.worker(worker_id)?;
        worker.submit(task).map_err(Error::from)
    }

    pub(crate) fn worker_executor(
        &self,
        worker_id: usize,
    ) -> Result<&runtime::WorkerExecutorHandle, Error> {
        Ok(&self.worker(worker_id)?.executor)
    }

    pub(crate) fn register_listener(
        &self,
        protocol: ListenerProtocol,
        addr: SocketAddr,
        active: Arc<AtomicBool>,
    ) -> Result<ListenerId, Error> {
        let id = self.next_listener_id();
        self.inner
            .listeners
            .lock()
            .map_err(|_| Error::Runtime("listener registry lock poisoned".to_string()))?
            .insert(
                id,
                ListenerControl {
                    protocol,
                    addr,
                    active,
                },
            );
        Ok(id)
    }

    pub(crate) fn service_config(&self, listener: ListenerConfig) -> ServiceConfig {
        ServiceConfig {
            bind_addr: listener.bind_addr,
            socket_options: listener.socket_options,
            socket_init_callback: None,
        }
    }

    fn next_listener_id(&self) -> ListenerId {
        ListenerId(self.inner.next_id.fetch_add(1, Ordering::SeqCst))
    }

    fn worker(&self, worker_id: usize) -> Result<&WorkerHandle, Error> {
        self.inner
            .workers
            .get(worker_id)
            .ok_or_else(|| Error::InvalidConfig("worker index is out of range".to_string()))
    }
}

struct WorkerHandle {
    executor: runtime::WorkerExecutorHandle,
    shutdown: Option<runtime::ShutdownSender>,
    join: Option<thread::JoinHandle<()>>,
}

impl WorkerHandle {
    fn submit<T>(&self, task: T) -> io::Result<()>
    where
        T: IntoExecutorTask,
    {
        self.executor.spawn_task(task.into_executor_task())
    }
}

impl Drop for WorkerHandle {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            shutdown.shutdown();
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn start_worker(name: String) -> io::Result<WorkerHandle> {
    let (executor_sender, executor_receiver) = mpsc::channel();
    let (shutdown_sender, shutdown_receiver) = runtime::shutdown_channel();
    let join = thread::Builder::new().name(name).spawn(move || {
        let executor = runtime::CurrentThreadExecutor::new().expect("worker runtime should build");
        let handle = executor.handle();
        if executor_sender.send(handle).is_err() {
            return;
        }
        executor.park_until_shutdown(shutdown_receiver);
    })?;
    let executor = executor_receiver
        .recv()
        .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "worker runtime stopped"))?;
    Ok(WorkerHandle {
        executor,
        shutdown: Some(shutdown_sender),
        join: Some(join),
    })
}

fn wake_tcp(addr: SocketAddr) {
    let _ = std::net::TcpStream::connect_timeout(&wake_addr(addr), Duration::from_millis(50));
}

fn wake_udp(addr: SocketAddr) {
    let bind_addr = match addr {
        SocketAddr::V4(_) => "127.0.0.1:0",
        SocketAddr::V6(_) => "[::1]:0",
    };
    if let Ok(socket) = std::net::UdpSocket::bind(bind_addr) {
        let _ = socket.send_to(&[], wake_addr(addr));
    }
}

fn wake_addr(addr: SocketAddr) -> SocketAddr {
    if !addr.ip().is_unspecified() {
        return addr;
    }
    match addr {
        SocketAddr::V4(addr) => SocketAddr::new(IpAddr::from([127, 0, 0, 1]), addr.port()),
        SocketAddr::V6(addr) => SocketAddr::new(IpAddr::from([0, 0, 0, 0, 0, 0, 0, 1]), addr.port()),
    }
}
