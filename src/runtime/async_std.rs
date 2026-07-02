use std::cell::RefCell;
use std::future::Future;
use std::io;
#[cfg(feature = "quinn")]
use std::io::IoSliceMut;
use std::net::{
    SocketAddr,
    TcpListener as StdTcpListener, TcpStream as StdTcpStream, UdpSocket as StdUdpSocket,
};
use std::sync::Arc;
use std::thread::{self, ThreadId};

pub type TcpListener = async_std::net::TcpListener;
pub type TcpStream = async_std::net::TcpStream;
pub type UdpSocket = Arc<async_std::net::UdpSocket>;
pub(crate) const SUPPORTS_USERSPACE_REUSEPORT_SIMULATION: bool = true;

pub(crate) struct ShutdownSender(async_std::channel::Sender<()>);
pub(crate) type ShutdownReceiver = async_std::channel::Receiver<()>;
pub(crate) type ExecutorHandle = CurrentThreadExecutor;

pub struct TaskHandle {
    task: Option<TaskHandleInner>,
}

enum TaskHandleInner {
    Executor(async_executor::Task<()>),
    Local(async_std::task::JoinHandle<()>),
}

impl TaskHandle {
    pub fn cancel(mut self) {
        match self.task.take() {
            Some(TaskHandleInner::Executor(task)) => {
                drop(task);
            }
            Some(TaskHandleInner::Local(handle)) => {
                let _ = async_std::task::block_on(handle.cancel());
            }
            None => {}
        }
    }
}

impl Drop for TaskHandle {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            match task {
                TaskHandleInner::Executor(task) => task.detach(),
                TaskHandleInner::Local(handle) => drop(handle),
            }
        }
    }
}

pub fn spawn_local<F>(future: F) -> io::Result<TaskHandle>
where
    F: Future<Output = ()> + 'static,
{
    CURRENT_EXECUTOR.with(|current| match current.borrow().as_ref() {
        Some(executor) => executor.local_spawn_task(future),
        None => Err(io::Error::new(
            io::ErrorKind::NotConnected,
            "no current sfo-reuseport async-std executor on this thread",
        )),
    })
}

pub(crate) fn shutdown_channel() -> (ShutdownSender, ShutdownReceiver) {
    let (sender, receiver) = async_std::channel::bounded(1);
    (ShutdownSender(sender), receiver)
}

impl ShutdownSender {
    pub(crate) fn shutdown(self) {
        let _ = self.0.try_send(());
    }
}

thread_local! {
    static CURRENT_EXECUTOR: RefCell<Option<CurrentThreadExecutor>> = RefCell::new(None);
}

#[derive(Clone)]
pub struct CurrentThreadExecutor {
    executor: Arc<async_executor::Executor<'static>>,
    owner_thread: ThreadId,
}

impl CurrentThreadExecutor {
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            executor: Arc::new(async_executor::Executor::new()),
            owner_thread: thread::current().id(),
        })
    }

    pub(crate) fn handle(&self) -> Self {
        self.clone()
    }

    pub fn block_on<F>(&self, future: F) -> F::Output
    where
        F: Future,
    {
        let previous = CURRENT_EXECUTOR.with(|current| current.replace(Some(self.handle())));
        let output = async_std::task::block_on(self.executor.run(future));
        CURRENT_EXECUTOR.with(|current| {
            current.replace(previous);
        });
        output
    }

    pub fn spawn<F>(&self, future: F) -> io::Result<TaskHandle>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        Ok(TaskHandle {
            task: Some(TaskHandleInner::Executor(self.executor.spawn(future))),
        })
    }

    pub(crate) fn spawn_task<T, Fut>(&self, task: T) -> io::Result<TaskHandle>
    where
        T: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + 'static,
    {
        if self.is_owner_thread() {
            return self.local_spawn_task(task());
        }
        let executor = self.clone();
        self.spawn(async move {
            let _ = executor.local_spawn_task(task());
        })
    }

    pub(crate) fn local_spawn_task<F>(&self, future: F) -> io::Result<TaskHandle>
    where
        F: Future<Output = ()> + 'static,
    {
        if !self.is_owner_thread() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "local task must be spawned on the executor owner thread",
            ));
        }
        Ok(TaskHandle {
            task: Some(TaskHandleInner::Local(async_std::task::spawn_local(future))),
        })
    }

    pub(crate) fn park_until_shutdown(&self, shutdown: ShutdownReceiver) {
        self.block_on(async move {
            let _ = shutdown.recv().await;
        });
    }

    pub(crate) fn is_owner_thread(&self) -> bool {
        thread::current().id() == self.owner_thread
    }
}

pub fn tcp_listener_from_std(listener: StdTcpListener) -> io::Result<TcpListener> {
    listener.set_nonblocking(true)?;
    Ok(TcpListener::from(listener))
}

pub fn tcp_stream_from_std(stream: StdTcpStream) -> io::Result<TcpStream> {
    stream.set_nonblocking(true)?;
    Ok(TcpStream::from(stream))
}

pub fn udp_socket_from_std(socket: StdUdpSocket) -> io::Result<UdpSocket> {
    socket.set_nonblocking(true)?;
    Ok(Arc::new(async_std::net::UdpSocket::from(socket)))
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
    let _ = (socket, buffer, target);
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "async-std UDP socket does not expose nonblocking send readiness",
    ))
}

#[cfg(feature = "quinn")]
pub fn udp_poll_send_ready(
    _socket: &UdpSocket,
    _cx: &mut std::task::Context<'_>,
) -> std::task::Poll<io::Result<()>> {
    std::task::Poll::Ready(Ok(()))
}

#[cfg(feature = "quinn")]
pub fn udp_poll_recv_from_slice(
    _socket: &UdpSocket,
    _cx: &mut std::task::Context<'_>,
    _buffer: &mut [u8],
) -> std::task::Poll<io::Result<(usize, SocketAddr)>> {
    std::task::Poll::Ready(Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "async-std UDP socket does not expose poll-based recv readiness",
    )))
}

#[cfg(feature = "quinn")]
pub fn udp_poll_recv_from_vectored(
    _socket: &UdpSocket,
    _cx: &mut std::task::Context<'_>,
    _buffers: &mut [IoSliceMut<'_>],
) -> std::task::Poll<io::Result<(usize, SocketAddr)>> {
    std::task::Poll::Ready(Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "async-std UDP socket does not expose poll-based recv readiness",
    )))
}
