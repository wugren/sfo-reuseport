use std::net::SocketAddr;
use std::sync::Arc;

use crate::core::Error;

pub type SocketInitCallback =
    Arc<dyn Fn(&socket2::Socket) -> Result<(), Error> + Send + Sync + 'static>;

#[derive(Clone)]
pub struct ServiceConfig {
    pub bind_addr: SocketAddr,
    pub socket_options: SocketOptions,
    pub socket_init_callback: Option<SocketInitCallback>,
}

impl ServiceConfig {
    pub fn new(bind_addr: SocketAddr) -> Self {
        Self {
            bind_addr,
            socket_options: SocketOptions::default(),
            socket_init_callback: None,
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

    pub(crate) fn validate(&self) -> Result<(), Error> {
        Ok(())
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

#[derive(Clone)]
pub struct ListenerConfig {
    pub bind_addr: SocketAddr,
    pub socket_options: SocketOptions,
}

impl ListenerConfig {
    pub fn new(bind_addr: SocketAddr) -> Self {
        Self {
            bind_addr,
            socket_options: SocketOptions::default(),
        }
    }

    pub fn with_socket_options(mut self, socket_options: SocketOptions) -> Self {
        self.socket_options = socket_options;
        self
    }
}

impl From<ServiceConfig> for ListenerConfig {
    fn from(config: ServiceConfig) -> Self {
        Self {
            bind_addr: config.bind_addr,
            socket_options: config.socket_options,
        }
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
}

impl Default for SocketOptions {
    fn default() -> Self {
        Self {
            reuse_address: true,
            ipv4_transparent: TransparentMode::Disabled,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransparentMode {
    Disabled,
    Required,
    BestEffort,
}
