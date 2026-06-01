use std::net::SocketAddr;
use std::sync::Arc;

use crate::core::Error;

pub type SocketInitCallback =
    Arc<dyn Fn(&socket2::Socket) -> Result<(), Error> + Send + Sync + 'static>;

pub const DEFAULT_ROUTED_PACKET_CHANNEL_CAPACITY: usize = 4096;

#[derive(Clone)]
pub struct TcpServiceConfig {
    pub bind_addr: SocketAddr,
    pub socket_options: SocketOptions,
    pub socket_init_callback: Option<SocketInitCallback>,
    pub max_concurrency_per_worker: Option<usize>,
}

impl TcpServiceConfig {
    pub fn new(bind_addr: SocketAddr) -> Self {
        Self {
            bind_addr,
            socket_options: SocketOptions::default(),
            socket_init_callback: None,
            max_concurrency_per_worker: None,
        }
    }

    pub fn with_socket_options(mut self, socket_options: SocketOptions) -> Self {
        self.socket_options = socket_options;
        self
    }

    pub fn with_socket_init_callback<F>(mut self, callback: F) -> Self
    where
        F: Fn(&socket2::Socket) -> Result<(), Error> + Send + Sync + 'static,
    {
        self.socket_init_callback = Some(Arc::new(callback));
        self
    }

    pub fn without_socket_init_callback(mut self) -> Self {
        self.socket_init_callback = None;
        self
    }

    pub fn with_max_concurrency_per_worker(mut self, max: usize) -> Self {
        self.max_concurrency_per_worker = Some(max);
        self
    }

    pub fn max_concurrency_per_worker(&self) -> Option<usize> {
        self.max_concurrency_per_worker
    }

    pub(crate) fn validate(&self) -> Result<(), Error> {
        Ok(())
    }
}

#[derive(Clone)]
pub struct UdpServiceConfig {
    pub bind_addr: SocketAddr,
    pub socket_options: SocketOptions,
    pub socket_init_callback: Option<SocketInitCallback>,
    pub max_concurrency_per_worker: Option<usize>,
    pub(crate) routed_packet_channel_capacity: usize,
}

impl UdpServiceConfig {
    pub fn new(bind_addr: SocketAddr) -> Self {
        Self {
            bind_addr,
            socket_options: SocketOptions::default(),
            socket_init_callback: None,
            max_concurrency_per_worker: None,
            routed_packet_channel_capacity: DEFAULT_ROUTED_PACKET_CHANNEL_CAPACITY,
        }
    }

    pub fn with_socket_options(mut self, socket_options: SocketOptions) -> Self {
        self.socket_options = socket_options;
        self
    }

    pub fn with_socket_init_callback<F>(mut self, callback: F) -> Self
    where
        F: Fn(&socket2::Socket) -> Result<(), Error> + Send + Sync + 'static,
    {
        self.socket_init_callback = Some(Arc::new(callback));
        self
    }

    pub fn without_socket_init_callback(mut self) -> Self {
        self.socket_init_callback = None;
        self
    }

    pub fn with_max_concurrency_per_worker(mut self, max: usize) -> Self {
        self.max_concurrency_per_worker = Some(max);
        self
    }

    pub fn max_concurrency_per_worker(&self) -> Option<usize> {
        self.max_concurrency_per_worker
    }

    #[cfg(windows)]
    pub fn with_routed_packet_channel_capacity(mut self, capacity: usize) -> Self {
        self.routed_packet_channel_capacity = capacity;
        self
    }

    #[cfg(windows)]
    pub fn routed_packet_channel_capacity(&self) -> usize {
        self.routed_packet_channel_capacity
    }

    #[cfg(all(
        not(windows),
        any(feature = "runtime-tokio", feature = "runtime-async-std")
    ))]
    pub(crate) fn routed_packet_channel_capacity(&self) -> usize {
        self.routed_packet_channel_capacity
    }

    pub(crate) fn validate(&self) -> Result<(), Error> {
        self.validate_routed_packet_channel_capacity()
    }

    pub(crate) fn validate_routed_packet_channel_capacity(&self) -> Result<(), Error> {
        if self.routed_packet_channel_capacity == 0 {
            return Err(Error::InvalidConfig(
                "routed packet channel capacity must be greater than zero".to_string(),
            ));
        }
        Ok(())
    }
}

pub(crate) trait SocketConfig {
    fn bind_addr(&self) -> SocketAddr;
    fn socket_options(&self) -> &SocketOptions;
    fn socket_init_callback(&self) -> Option<&SocketInitCallback>;
}

impl SocketConfig for TcpServiceConfig {
    fn bind_addr(&self) -> SocketAddr {
        self.bind_addr
    }

    fn socket_options(&self) -> &SocketOptions {
        &self.socket_options
    }

    fn socket_init_callback(&self) -> Option<&SocketInitCallback> {
        self.socket_init_callback.as_ref()
    }
}

impl SocketConfig for UdpServiceConfig {
    fn bind_addr(&self) -> SocketAddr {
        self.bind_addr
    }

    fn socket_options(&self) -> &SocketOptions {
        &self.socket_options
    }

    fn socket_init_callback(&self) -> Option<&SocketInitCallback> {
        self.socket_init_callback.as_ref()
    }
}

#[derive(Clone)]
pub struct ServerRuntimeConfig {
    pub workers: WorkerCount,
}

impl Default for ServerRuntimeConfig {
    fn default() -> Self {
        Self {
            workers: WorkerCount::Default,
        }
    }
}

impl ServerRuntimeConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_workers(mut self, workers: usize) -> Self {
        self.workers = WorkerCount::Fixed(workers);
        self
    }

    pub(crate) fn validate(&self) -> Result<(), Error> {
        if matches!(self.workers, WorkerCount::Fixed(0)) {
            return Err(Error::InvalidConfig(
                "worker count must be greater than zero".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkerCount {
    Default,
    Fixed(usize),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SocketOptions {
    pub reuse_address: bool,
    pub ipv4_transparent: TransparentMode,
    pub ipv6_transparent: TransparentMode,
}

impl Default for SocketOptions {
    fn default() -> Self {
        Self {
            reuse_address: true,
            ipv4_transparent: TransparentMode::Disabled,
            ipv6_transparent: TransparentMode::Disabled,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransparentMode {
    Disabled,
    Required,
    BestEffort,
}
