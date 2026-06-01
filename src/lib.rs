#![deny(unsafe_code)]

#[cfg(any(
    all(feature = "runtime-tokio", feature = "runtime-async-std"),
    all(feature = "runtime-tokio", feature = "runtime-tokio-uring"),
    all(feature = "runtime-async-std", feature = "runtime-tokio-uring"),
))]
compile_error!(
    "features `runtime-tokio`, `runtime-async-std`, and `runtime-tokio-uring` are mutually exclusive"
);

#[cfg(not(any(
    feature = "runtime-tokio",
    feature = "runtime-async-std",
    feature = "runtime-tokio-uring"
)))]
compile_error!(
    "enable exactly one runtime feature: `runtime-tokio`, `runtime-async-std`, or `runtime-tokio-uring`"
);

#[cfg(all(feature = "runtime-tokio-uring", not(target_os = "linux")))]
compile_error!("feature `runtime-tokio-uring` is only supported on Linux targets");

#[cfg(not(all(feature = "runtime-tokio-uring", not(target_os = "linux"))))]
pub mod core;
#[cfg(not(all(feature = "runtime-tokio-uring", not(target_os = "linux"))))]
pub mod platform;
#[cfg(not(all(feature = "runtime-tokio-uring", not(target_os = "linux"))))]
pub mod runtime;

#[cfg(not(all(feature = "runtime-tokio-uring", not(target_os = "linux"))))]
pub use crate::core::{
    Error, PacketMeta, QuicCidGenerator, QuicServer, ServerRuntime, ServerRuntimeConfig,
    SocketInitCallback, SocketOptions, TcpServer, TcpServiceConfig, TransparentMode, UdpServer,
    UdpServiceConfig, UdpSocket, WorkerCount, DEFAULT_ROUTED_PACKET_CHANNEL_CAPACITY,
};
#[cfg(not(all(feature = "runtime-tokio-uring", not(target_os = "linux"))))]
pub use crate::runtime::{spawn, TaskHandle, TcpStream};
