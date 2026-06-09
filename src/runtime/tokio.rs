use std::collections::HashMap;
use std::future::Future;
use std::io;
#[cfg(feature = "quinn")]
use std::io::IoSliceMut;
use std::net::{
    SocketAddr,
    TcpListener as StdTcpListener, TcpStream as StdTcpStream, UdpSocket as StdUdpSocket,
};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{self, ThreadId};

pub type TcpListener = tokio::net::TcpListener;
pub type TcpStream = tokio::net::TcpStream;
pub type UdpSocket = Arc<tokio::net::UdpSocket>;
pub(crate) const SUPPORTS_USERSPACE_REUSEPORT_SIMULATION: bool = true;

pub(crate) struct ShutdownSender(tokio::sync::oneshot::Sender<()>);
pub(crate) type ShutdownReceiver = tokio::sync::oneshot::Receiver<()>;
type LocalTask = Pin<Box<dyn Future<Output = ()> + 'static>>;
type BoxedExecutorTask = Box<dyn FnOnce() -> LocalTask + Send + 'static>;
type TaskCompletionSender = tokio::sync::mpsc::UnboundedSender<u64>;

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

pub fn spawn_local<F>(future: F) -> io::Result<TaskHandle>
where
    F: Future<Output = ()> + 'static,
{
    let handle = tokio::task::spawn_local(future);
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

enum CurrentThreadExecutorInner {
    Runtime {
        runtime: tokio::runtime::Runtime,
        local_set: tokio::task::LocalSet,
        task_receiver: tokio::sync::mpsc::UnboundedReceiver<TaskCommand>,
    },
}

enum TaskCommand {
    Spawn { id: u64, task: BoxedExecutorTask },
    Cancel { id: u64 },
}

struct TaskCompletionGuard {
    id: u64,
    completion_sender: TaskCompletionSender,
}

impl Drop for TaskCompletionGuard {
    fn drop(&mut self) {
        let _ = self.completion_sender.send(self.id);
    }
}

pub struct CurrentThreadExecutor {
    inner: CurrentThreadExecutorInner,
    task_sender: tokio::sync::mpsc::UnboundedSender<TaskCommand>,
    owner_thread: ThreadId,
    next_task_id: Arc<AtomicU64>,
}

#[derive(Clone)]
pub(crate) struct ExecutorHandle {
    task_sender: tokio::sync::mpsc::UnboundedSender<TaskCommand>,
    owner_thread: ThreadId,
    next_task_id: Arc<AtomicU64>,
}

impl CurrentThreadExecutor {
    pub fn new() -> io::Result<Self> {
        let mut builder = tokio::runtime::Builder::new_current_thread();
        builder.enable_all();
        let runtime = builder.build()?;
        let (task_sender, task_receiver) = tokio::sync::mpsc::unbounded_channel();
        Ok(Self {
            inner: CurrentThreadExecutorInner::Runtime {
                runtime,
                local_set: tokio::task::LocalSet::new(),
                task_receiver,
            },
            task_sender,
            owner_thread: thread::current().id(),
            next_task_id: Arc::new(AtomicU64::new(1)),
        })
    }

    pub(crate) fn handle(&self) -> ExecutorHandle {
        ExecutorHandle {
            task_sender: self.task_sender.clone(),
            owner_thread: self.owner_thread,
            next_task_id: Arc::clone(&self.next_task_id),
        }
    }

    pub fn block_on<F>(&self, future: F) -> F::Output
    where
        F: Future,
    {
        match &self.inner {
            CurrentThreadExecutorInner::Runtime {
                runtime, local_set, ..
            } => {
                runtime.block_on(local_set.run_until(future))
            }
        }
    }

    pub fn spawn<F>(&self, future: F) -> io::Result<TaskHandle>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let handle = match &self.inner {
            CurrentThreadExecutorInner::Runtime { runtime, .. } => runtime.spawn(future),
        };
        Ok(TaskHandle {
            inner: TaskHandleInner::Local(handle),
        })
    }

    pub(crate) fn park_until_shutdown(self, shutdown: ShutdownReceiver) {
        let CurrentThreadExecutorInner::Runtime {
            runtime,
            local_set,
            mut task_receiver,
        } = self.inner;
        runtime.block_on(local_set.run_until(async move {
            let mut shutdown = shutdown;
            let (completion_sender, mut completion_receiver) =
                tokio::sync::mpsc::unbounded_channel();
            let mut task_handles = HashMap::new();
            loop {
                tokio::select! {
                    _ = &mut shutdown => break,
                    completed_id = completion_receiver.recv() => {
                        let Some(completed_id) = completed_id else {
                            break;
                        };
                        task_handles.remove(&completed_id);
                    }
                    command = task_receiver.recv() => {
                        let Some(command) = command else {
                            break;
                        };
                        match command {
                            TaskCommand::Spawn { id, task } => {
                                let completion_sender = completion_sender.clone();
                                task_handles.insert(id, tokio::task::spawn_local(async move {
                                    let _completion_guard = TaskCompletionGuard {
                                        id,
                                        completion_sender,
                                    };
                                    task().await;
                                }));
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
        }));
    }
}

impl ExecutorHandle {
    pub(crate) fn spawn_local_task<F>(&self, future: F) -> io::Result<TaskHandle>
    where
        F: Future<Output = ()> + 'static,
    {
        if thread::current().id() != self.owner_thread {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "local task must be spawned on the executor owner thread",
            ));
        }
        spawn_local(future)
    }

    pub(crate) fn spawn_task<T>(&self, task: T) -> io::Result<TaskHandle>
    where
        T: FnOnce() -> Pin<Box<dyn Future<Output = ()> + 'static>> + Send + 'static,
    {
        if thread::current().id() == self.owner_thread {
            return self.spawn_local_task(task());
        }
        let id = self.next_task_id.fetch_add(1, Ordering::Relaxed);
        let task = Box::new(task);
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

pub fn tcp_listener_from_std(listener: StdTcpListener) -> io::Result<TcpListener> {
    listener.set_nonblocking(true)?;
    TcpListener::from_std(listener)
}

pub fn tcp_stream_from_std(stream: StdTcpStream) -> io::Result<TcpStream> {
    stream.set_nonblocking(true)?;
    TcpStream::from_std(stream)
}

pub fn udp_socket_from_std(socket: StdUdpSocket) -> io::Result<UdpSocket> {
    socket.set_nonblocking(true)?;
    tokio::runtime::Handle::try_current()
        .map_err(|error| io::Error::new(io::ErrorKind::NotConnected, error))?;
    tokio::net::UdpSocket::from_std(socket).map(Arc::new)
}

pub async fn udp_recv_from(
    socket: &UdpSocket,
    mut buffer: Vec<u8>,
) -> io::Result<(usize, SocketAddr, Vec<u8>)> {
    let (len, peer_addr) = socket.recv_from(buffer.as_mut_slice()).await?;
    Ok((len, peer_addr, buffer))
}

pub async fn udp_recv_from_slice(
    socket: &UdpSocket,
    buffer: &mut [u8],
) -> io::Result<(usize, SocketAddr)> {
    socket.recv_from(buffer).await
}

pub async fn udp_send_to(
    socket: &UdpSocket,
    buffer: &[u8],
    target: SocketAddr,
) -> io::Result<usize> {
    socket.send_to(buffer, target).await
}

#[cfg(feature = "quinn")]
pub fn udp_try_send_to(
    socket: &UdpSocket,
    buffer: &[u8],
    target: SocketAddr,
) -> io::Result<usize> {
    socket.try_send_to(buffer, target)
}

#[cfg(feature = "quinn")]
pub fn udp_poll_send_ready(
    socket: &UdpSocket,
    cx: &mut std::task::Context<'_>,
) -> std::task::Poll<io::Result<()>> {
    socket.poll_send_ready(cx)
}

#[cfg(feature = "quinn")]
pub fn udp_poll_recv_from_slice(
    socket: &UdpSocket,
    cx: &mut std::task::Context<'_>,
    buffer: &mut [u8],
) -> std::task::Poll<io::Result<(usize, SocketAddr)>> {
    let mut future = Box::pin(socket.recv_from(buffer));
    future.as_mut().poll(cx)
}

#[cfg(feature = "quinn")]
pub fn udp_poll_recv_from_vectored(
    socket: &UdpSocket,
    cx: &mut std::task::Context<'_>,
    buffers: &mut [IoSliceMut<'_>],
) -> std::task::Poll<io::Result<(usize, SocketAddr)>> {
    let mut buffer = vec![0_u8; 65_536];
    match udp_poll_recv_from_slice(socket, cx, &mut buffer) {
        std::task::Poll::Pending => std::task::Poll::Pending,
        std::task::Poll::Ready(Err(error)) => std::task::Poll::Ready(Err(error)),
        std::task::Poll::Ready(Ok((len, peer_addr))) => {
            scatter_datagram(&buffer[..len], buffers);
            std::task::Poll::Ready(Ok((len, peer_addr)))
        }
    }
}

#[cfg(feature = "quinn")]
fn scatter_datagram(payload: &[u8], buffers: &mut [IoSliceMut<'_>]) {
    let mut offset = 0;
    for buffer in buffers {
        if offset >= payload.len() {
            break;
        }
        let copy_len = (payload.len() - offset).min(buffer.len());
        buffer[..copy_len].copy_from_slice(&payload[offset..offset + copy_len]);
        offset += copy_len;
    }
}
