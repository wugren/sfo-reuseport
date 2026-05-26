mod config;
mod error;
mod schedule;
mod server_runtime;
mod tcp;
mod udp;

use std::future::Future;
use std::pin::Pin;

pub use config::{
    ListenerConfig, ServerRuntimeConfig, ServiceConfig, SocketInitCallback, SocketOptions,
    TransparentMode, WorkerCount,
};
pub use error::Error;
pub use server_runtime::{ListenerId, ListenerProtocol, ServerRuntime};
pub use tcp::TcpServer;
pub use udp::{PacketMeta, QuicServer, UdpServer};

pub(crate) use schedule::linux_reuseport_select;

pub(crate) type HandlerFutureBox = Pin<Box<dyn Future<Output = Result<(), Error>> + Send>>;

pub trait HandlerFuture: Future<Output = Result<(), Error>> + Send + 'static {}
impl<T> HandlerFuture for T where T: Future<Output = Result<(), Error>> + Send + 'static {}
