use std::cell::RefCell;
use std::future::Future;
use std::io;
#[cfg(feature = "quinn")]
use std::io::IoSliceMut;
use std::net::{
    SocketAddr,
    TcpListener as StdTcpListener, TcpStream as StdTcpStream, UdpSocket as StdUdpSocket,
};
use std::pin::Pin;
use std::sync::Arc;

pub type TcpListener = async_std::net::TcpListener;
pub type TcpStream = async_std::net::TcpStream;
pub type UdpSocket = Arc<async_std::net::UdpSocket>;
pub(crate) const SUPPORTS_USERSPACE_REUSEPORT_SIMULATION: bool = true;

pub(crate) struct ShutdownSender(async_std::channel::Sender<()>);
pub(crate) type ShutdownReceiver = async_std::channel::Receiver<()>;
pub(crate) type ExecutorTask = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;
pub(crate) type ExecutorHandle = CurrentThreadExecutor;

pub struct TaskHandle {
    task: Option<async_executor::Task<()>>,
}

impl TaskHandle {
    pub fn cancel(mut self) {
        self.task.take();
    }
}

impl Drop for TaskHandle {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.detach();
        }
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
    CURRENT_EXECUTOR.with(|current| match current.borrow().as_ref() {
        Some(executor) => executor.spawn(future),
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
}

impl CurrentThreadExecutor {
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            executor: Arc::new(async_executor::Executor::new()),
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
            task: Some(self.executor.spawn(future)),
        })
    }

    pub(crate) fn spawn_task(&self, task: ExecutorTask) -> io::Result<TaskHandle> {
        self.spawn(task)
    }

    pub(crate) fn park_until_shutdown(&self, shutdown: ShutdownReceiver) {
        self.block_on(async move {
            let _ = shutdown.recv().await;
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
