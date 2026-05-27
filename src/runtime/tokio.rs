use std::future::Future;
use std::io;
use std::net::{
    SocketAddr,
    TcpListener as StdTcpListener, TcpStream as StdTcpStream, UdpSocket as StdUdpSocket,
};
use std::pin::Pin;
use std::sync::Arc;

pub type TcpListener = tokio::net::TcpListener;
pub type TcpStream = tokio::net::TcpStream;
pub type UdpSocket = Arc<tokio::net::UdpSocket>;
pub(crate) const SUPPORTS_USERSPACE_REUSEPORT_SIMULATION: bool = true;

pub(crate) struct ShutdownSender(tokio::sync::oneshot::Sender<()>);
pub(crate) type ShutdownReceiver = tokio::sync::oneshot::Receiver<()>;
pub(crate) type ExecutorTask = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;
pub(crate) type ExecutorHandle = CurrentThreadExecutor;

pub struct TaskHandle {
    handle: tokio::task::JoinHandle<()>,
}

impl TaskHandle {
    pub fn cancel(self) {
        self.handle.abort();
    }
}

pub(crate) fn executor_task<F, Fut>(task: F) -> ExecutorTask
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    Box::pin(task())
}

pub fn spawn<F>(future: F) -> io::Result<TaskHandle>
where
    F: Future<Output = ()> + Send + 'static,
{
    let handle = tokio::runtime::Handle::try_current()
        .map_err(|error| io::Error::new(io::ErrorKind::NotConnected, error))?
        .spawn(future);
    Ok(TaskHandle { handle })
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
    Runtime(tokio::runtime::Runtime),
    Handle(tokio::runtime::Handle),
}

pub struct CurrentThreadExecutor {
    inner: CurrentThreadExecutorInner,
}

impl Clone for CurrentThreadExecutor {
    fn clone(&self) -> Self {
        self.handle()
    }
}

impl CurrentThreadExecutor {
    pub fn new() -> io::Result<Self> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        Ok(Self {
            inner: CurrentThreadExecutorInner::Runtime(runtime),
        })
    }

    pub(crate) fn handle(&self) -> Self {
        match &self.inner {
            CurrentThreadExecutorInner::Runtime(runtime) => Self {
                inner: CurrentThreadExecutorInner::Handle(runtime.handle().clone()),
            },
            CurrentThreadExecutorInner::Handle(handle) => Self {
                inner: CurrentThreadExecutorInner::Handle(handle.clone()),
            },
        }
    }

    pub fn block_on<F>(&self, future: F) -> F::Output
    where
        F: Future,
    {
        match &self.inner {
            CurrentThreadExecutorInner::Runtime(runtime) => runtime.block_on(future),
            CurrentThreadExecutorInner::Handle(handle) => handle.block_on(future),
        }
    }

    pub fn spawn<F>(&self, future: F) -> io::Result<TaskHandle>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let handle = match &self.inner {
            CurrentThreadExecutorInner::Runtime(runtime) => runtime.spawn(future),
            CurrentThreadExecutorInner::Handle(handle) => handle.spawn(future),
        };
        Ok(TaskHandle { handle })
    }

    pub(crate) fn spawn_task(&self, task: ExecutorTask) -> io::Result<TaskHandle> {
        self.spawn(task)
    }

    pub(crate) fn park_until_shutdown(&self, shutdown: ShutdownReceiver) {
        self.block_on(async move {
            let _ = shutdown.await;
        });
    }
}

pub(crate) async fn submit_or_run_local<T, TaskFut, LocalFut>(
    executor: &ExecutorHandle,
    task: T,
    _local: LocalFut,
) -> io::Result<()>
where
    T: FnOnce() -> TaskFut + Send + 'static,
    TaskFut: Future<Output = ()> + Send + 'static,
    LocalFut: Future<Output = ()>,
{
    executor.spawn_task(executor_task(task)).map(|_| ())
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
    tokio::net::UdpSocket::from_std(socket).map(Arc::new)
}

pub async fn udp_recv_from(
    socket: &UdpSocket,
    buffer: &mut Vec<u8>,
) -> io::Result<(usize, SocketAddr)> {
    socket.recv_from(buffer.as_mut_slice()).await
}
