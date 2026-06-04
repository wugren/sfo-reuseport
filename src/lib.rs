#![deny(unsafe_code)]

#[cfg(any(
    all(feature = "runtime-tokio", feature = "runtime-async-std"),
))]
compile_error!("features `runtime-tokio` and `runtime-async-std` are mutually exclusive");

#[cfg(not(any(feature = "runtime-tokio", feature = "runtime-async-std")))]
compile_error!("enable exactly one runtime feature: `runtime-tokio` or `runtime-async-std`");

pub mod core;
pub(crate) mod platform;
pub mod runtime;

pub use crate::core::{
    Error, PacketMeta, QuicCidGenerator, QuicServer, ServerRuntime, ServerRuntimeConfig,
    SocketInitCallback, SocketOptions, TcpServer, TcpServiceConfig, TransparentMode, UdpServer,
    UdpServiceConfig, UdpSocket, WorkerCount, DEFAULT_ROUTED_PACKET_CHANNEL_CAPACITY,
};
pub use crate::runtime::{spawn_local, TaskHandle, TcpStream};
