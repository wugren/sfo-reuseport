use std::future::Future;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

use crate::core::{Error, ServerRuntimeConfig, WorkerCount};
use crate::runtime;

#[derive(Clone)]
pub struct ServerRuntime {
    inner: Arc<ServerRuntimeInner>,
}

struct ServerRuntimeInner {
    workers: Vec<WorkerHandle>,
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
                active: Arc::new(AtomicBool::new(true)),
            }),
        })
    }

    pub(crate) fn worker_count(&self) -> usize {
        self.inner.workers.len()
    }

    #[cfg(feature = "runtime-tokio-uring")]
    pub(crate) fn submit_to_worker<T, Fut>(&self, worker_id: usize, task: T) -> Result<(), Error>
    where
        T: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + 'static,
    {
        let worker = self.worker(worker_id)?;
        worker.submit(runtime::executor_task(task)).map_err(Error::from)
    }

    #[cfg(any(feature = "runtime-tokio", feature = "runtime-async-std"))]
    pub(crate) fn submit_to_worker<T, Fut>(&self, worker_id: usize, task: T) -> Result<(), Error>
    where
        T: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let worker = self.worker(worker_id)?;
        worker.submit(runtime::executor_task(task)).map_err(Error::from)
    }

    #[cfg(feature = "runtime-tokio-uring")]
    pub(crate) fn submit_to_executor<T, Fut>(
        executor: &runtime::ExecutorHandle,
        task: T,
    ) -> Result<(), Error>
    where
        T: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + 'static,
    {
        executor
            .spawn_task(runtime::executor_task(task))
            .map_err(Error::from)
    }

    #[cfg(any(feature = "runtime-tokio", feature = "runtime-async-std"))]
    pub(crate) fn submit_to_executor<T, Fut>(
        executor: &runtime::ExecutorHandle,
        task: T,
    ) -> Result<(), Error>
    where
        T: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        executor
            .spawn_task(runtime::executor_task(task))
            .map_err(Error::from)
    }

    pub(crate) fn worker_executors(&self) -> Vec<runtime::ExecutorHandle> {
        self.inner
            .workers
            .iter()
            .map(|worker| worker.executor.clone())
            .collect()
    }

    pub(crate) fn active_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.inner.active)
    }

    fn worker(&self, worker_id: usize) -> Result<&WorkerHandle, Error> {
        self.inner
            .workers
            .get(worker_id)
            .ok_or_else(|| Error::InvalidConfig("worker index is out of range".to_string()))
    }
}

impl Drop for ServerRuntimeInner {
    fn drop(&mut self) {
        self.active.store(false, Ordering::SeqCst);
    }
}

struct WorkerHandle {
    executor: runtime::ExecutorHandle,
    shutdown: Option<runtime::ShutdownSender>,
    join: Option<thread::JoinHandle<()>>,
}

impl WorkerHandle {
    fn submit(&self, task: runtime::ExecutorTask) -> io::Result<()> {
        self.executor.spawn_task(task)
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
