mod config;
mod error;
mod quic;
mod schedule;
mod server_runtime;
mod tcp;
mod udp;

use std::future::Future;
use std::pin::Pin;

pub use config::{
    ServerRuntimeConfig, ServiceConfig, SocketInitCallback, SocketOptions, TransparentMode,
    WorkerCount,
};
pub use error::Error;
pub use quic::QuicServer;
pub use server_runtime::ServerRuntime;
pub use tcp::TcpServer;
pub use udp::{PacketMeta, UdpServer, UdpSocket};

pub(crate) use schedule::linux_reuseport_select;

#[cfg(feature = "runtime-tokio-uring")]
pub(crate) type HandlerFutureBox = Pin<Box<dyn Future<Output = Result<(), Error>>>>;
#[cfg(any(feature = "runtime-tokio", feature = "runtime-async-std"))]
pub(crate) type HandlerFutureBox = Pin<Box<dyn Future<Output = Result<(), Error>> + Send>>;

#[cfg(feature = "runtime-tokio-uring")]
pub trait HandlerFuture: Future<Output = Result<(), Error>> + 'static {}
#[cfg(feature = "runtime-tokio-uring")]
impl<T> HandlerFuture for T where T: Future<Output = Result<(), Error>> + 'static {}

#[cfg(any(feature = "runtime-tokio", feature = "runtime-async-std"))]
pub trait HandlerFuture: Future<Output = Result<(), Error>> + Send + 'static {}
#[cfg(any(feature = "runtime-tokio", feature = "runtime-async-std"))]
impl<T> HandlerFuture for T where T: Future<Output = Result<(), Error>> + Send + 'static {}
