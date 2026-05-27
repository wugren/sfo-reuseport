#[cfg(not(target_os = "linux"))]
compile_error!("runtime-tokio-uring is only supported on Linux targets");

use std::collections::HashMap;
use std::future::Future;
use std::io;
use std::net::{
    SocketAddr, TcpListener as StdTcpListener, TcpStream as StdTcpStream,
    UdpSocket as StdUdpSocket,
};
use std::ops::Deref;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{self, ThreadId};

use tokio_uring::BufResult;
use tokio_uring::buf::BoundedBuf;

pub type TcpListener = tokio_uring::net::TcpListener;
pub type TcpStream = tokio_uring::net::TcpStream;
pub(crate) const SUPPORTS_USERSPACE_REUSEPORT_SIMULATION: bool = false;

#[derive(Clone)]
pub struct UdpSocket(Arc<tokio_uring::net::UdpSocket>);

impl UdpSocket {
    pub async fn bind(socket_addr: SocketAddr) -> io::Result<Self> {
        tokio_uring::net::UdpSocket::bind(socket_addr)
            .await
            .map(Self::new)
    }

    pub fn from_std(socket: StdUdpSocket) -> Self {
        Self::new(tokio_uring::net::UdpSocket::from_std(socket))
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.0.local_addr()
    }

    pub async fn connect(&self, socket_addr: SocketAddr) -> io::Result<()> {
        self.0.connect(socket_addr).await
    }

    pub async fn send_to<T: BoundedBuf>(&self, buf: T, socket_addr: SocketAddr) -> BufResult<usize, T> {
        self.0.send_to(buf, socket_addr).await
    }

    pub async fn recv_from<T: tokio_uring::buf::BoundedBufMut>(
        &self,
        buf: T,
    ) -> BufResult<(usize, SocketAddr), T> {
        self.0.recv_from(buf).await
    }

    fn new(socket: tokio_uring::net::UdpSocket) -> Self {
        Self(Arc::new(socket))
    }
}

impl Deref for UdpSocket {
    type Target = tokio_uring::net::UdpSocket;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub(crate) struct ShutdownSender(tokio::sync::oneshot::Sender<()>);
pub(crate) type ShutdownReceiver = tokio::sync::oneshot::Receiver<()>;
pub(crate) type ExecutorTask =
    Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = ()> + 'static>> + Send + 'static>;

pub struct TaskHandle {
    inner: TaskHandleInner,
}

enum TaskHandleInner {
    Local(tokio::task::JoinHandle<()>),
    Remote {
        id: u64,
        task_sender: tokio::sync::mpsc::UnboundedSender<TaskCommand>,
    },
}

impl TaskHandle {
    pub fn cancel(self) {
        match self.inner {
            TaskHandleInner::Local(handle) => handle.abort(),
            TaskHandleInner::Remote { id, task_sender } => {
                let _ = task_sender.send(TaskCommand::Cancel { id });
            }
        }
    }
}

pub(crate) fn executor_task<F, Fut>(task: F) -> ExecutorTask
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + 'static,
{
    Box::new(move || Box::pin(task()))
}

pub fn spawn<F>(future: F) -> io::Result<TaskHandle>
where
    F: Future<Output = ()> + 'static,
{
    let handle = tokio_uring::spawn(future);
    Ok(TaskHandle {
        inner: TaskHandleInner::Local(handle),
    })
}

pub(crate) fn shutdown_channel() -> (ShutdownSender, ShutdownReceiver) {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    (ShutdownSender(sender), receiver)
}

impl ShutdownSender {
    pub(crate) fn shutdown(self) {
        let _ = self.0.send(());
    }
}

enum TaskCommand {
    Spawn { id: u64, task: ExecutorTask },
    Cancel { id: u64 },
}

enum CurrentThreadExecutorInner {
    Runtime {
        runtime: tokio_uring::Runtime,
        task_receiver: tokio::sync::mpsc::UnboundedReceiver<TaskCommand>,
    },
}

pub struct CurrentThreadExecutor {
    owner_thread: ThreadId,
    next_task_id: Arc<AtomicU64>,
    task_sender: tokio::sync::mpsc::UnboundedSender<TaskCommand>,
    inner: CurrentThreadExecutorInner,
}

#[derive(Clone)]
pub(crate) struct ExecutorHandle {
    owner_thread: ThreadId,
    next_task_id: Arc<AtomicU64>,
    task_sender: tokio::sync::mpsc::UnboundedSender<TaskCommand>,
}

impl CurrentThreadExecutor {
    pub fn new() -> io::Result<Self> {
        let runtime = tokio_uring::Runtime::new(&tokio_uring::builder())?;
        let (task_sender, task_receiver) = tokio::sync::mpsc::unbounded_channel();
        Ok(Self {
            owner_thread: thread::current().id(),
            next_task_id: Arc::new(AtomicU64::new(1)),
            task_sender,
            inner: CurrentThreadExecutorInner::Runtime {
                runtime,
                task_receiver,
            },
        })
    }

    pub(crate) fn handle(&self) -> ExecutorHandle {
        ExecutorHandle {
            owner_thread: self.owner_thread,
            next_task_id: Arc::clone(&self.next_task_id),
            task_sender: self.task_sender.clone(),
        }
    }

    pub fn block_on<F>(&self, future: F) -> F::Output
    where
        F: Future + 'static,
    {
        match &self.inner {
            CurrentThreadExecutorInner::Runtime { runtime, .. } => runtime.block_on(future),
        }
    }

    pub fn spawn<F>(&self, future: F) -> io::Result<TaskHandle>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.spawn_task(Box::new(move || Box::pin(future)))
    }

    pub(crate) fn spawn_task(&self, task: ExecutorTask) -> io::Result<TaskHandle> {
        if thread::current().id() == self.owner_thread {
            let handle = tokio_uring::spawn(task());
            return Ok(TaskHandle {
                inner: TaskHandleInner::Local(handle),
            });
        }
        let id = self.next_task_id.fetch_add(1, Ordering::Relaxed);
        self.task_sender
            .send(TaskCommand::Spawn { id, task })
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "worker runtime stopped"))?;
        Ok(TaskHandle {
            inner: TaskHandleInner::Remote {
                id,
                task_sender: self.task_sender.clone(),
            },
        })
    }

    pub(crate) fn park_until_shutdown(self, shutdown: ShutdownReceiver) {
        let CurrentThreadExecutorInner::Runtime {
            runtime,
            mut task_receiver,
        } = self.inner;
        runtime.block_on(async move {
            let mut shutdown = shutdown;
            let mut task_handles = HashMap::new();
            loop {
                tokio::select! {
                    _ = &mut shutdown => break,
                    command = task_receiver.recv() => {
                        let Some(command) = command else {
                            break;
                        };
                        task_handles.retain(|_, handle: &mut tokio::task::JoinHandle<()>| {
                            !handle.is_finished()
                        });
                        match command {
                            TaskCommand::Spawn { id, task } => {
                                task_handles.insert(id, tokio_uring::spawn(task()));
                            }
                            TaskCommand::Cancel { id } => {
                                if let Some(handle) = task_handles.remove(&id) {
                                    handle.abort();
                                }
                            }
                        }
                    }
                }
            }
            for (_, handle) in task_handles {
                handle.abort();
            }
        });
    }
}

impl ExecutorHandle {
    pub(crate) fn spawn_task(&self, task: ExecutorTask) -> io::Result<TaskHandle> {
        if thread::current().id() == self.owner_thread {
            let handle = tokio_uring::spawn(task());
            return Ok(TaskHandle {
                inner: TaskHandleInner::Local(handle),
            });
        }
        let id = self.next_task_id.fetch_add(1, Ordering::Relaxed);
        self.task_sender
            .send(TaskCommand::Spawn { id, task })
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "worker runtime stopped"))?;
        Ok(TaskHandle {
            inner: TaskHandleInner::Remote {
                id,
                task_sender: self.task_sender.clone(),
            },
        })
    }
}

pub(crate) async fn submit_or_run_local<T, TaskFut, LocalFut>(
    _executor: &ExecutorHandle,
    _task: T,
    local: LocalFut,
) -> io::Result<()>
where
    T: FnOnce() -> TaskFut,
    TaskFut: Future<Output = ()>,
    LocalFut: Future<Output = ()>,
{
    local.await;
    Ok(())
}

pub fn tcp_listener_from_std(listener: StdTcpListener) -> io::Result<TcpListener> {
    listener.set_nonblocking(true)?;
    Ok(TcpListener::from_std(listener))
}

pub fn tcp_stream_from_std(stream: StdTcpStream) -> io::Result<TcpStream> {
    stream.set_nonblocking(true)?;
    Ok(TcpStream::from_std(stream))
}

pub fn udp_socket_from_std(socket: StdUdpSocket) -> io::Result<UdpSocket> {
    socket.set_nonblocking(true)?;
    Ok(UdpSocket::from_std(socket))
}

pub async fn udp_recv_from(
    socket: &UdpSocket,
    buffer: &mut Vec<u8>,
) -> io::Result<(usize, SocketAddr)> {
    let recv_buffer = std::mem::take(buffer);
    let (result, recv_buffer) = socket.recv_from(recv_buffer).await;
    *buffer = recv_buffer;
    result
}
