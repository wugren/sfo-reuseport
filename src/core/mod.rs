mod cid;
mod concurrency;
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
    DEFAULT_ROUTED_PACKET_CHANNEL_CAPACITY, ServerRuntimeConfig, SocketInitCallback,
    SocketOptions, TcpServiceConfig, TransparentMode, UdpServiceConfig, WorkerCount,
};
pub(crate) use config::SocketConfig;
pub use cid::QuicCidGenerator;
pub use error::Error;
pub use quic::QuicServer;
pub use server_runtime::ServerRuntime;
pub use tcp::TcpServer;
pub use udp::{PacketMeta, UdpServer, UdpSocket};

pub(crate) use schedule::linux_reuseport_select;
pub(crate) use concurrency::{ConcurrencyPermit, WorkerConcurrencyLimit};

#[cfg(any(
    feature = "runtime-tokio",
    feature = "runtime-async-std",
))]
pub(crate) type HandlerFutureBox = Pin<Box<dyn Future<Output = Result<(), Error>> + Send>>;

#[cfg(any(
    feature = "runtime-tokio",
    feature = "runtime-async-std",
))]
pub trait HandlerFuture: Future<Output = Result<(), Error>> + Send + 'static {}
#[cfg(any(
    feature = "runtime-tokio",
    feature = "runtime-async-std",
))]
impl<T> HandlerFuture for T where T: Future<Output = Result<(), Error>> + Send + 'static {}
