use std::net::UdpSocket;

use crate::core::{Error, ServiceConfig, TransparentMode};

pub(crate) fn set_reuse_port(socket: &socket2::Socket) -> Result<(), Error> {
    super::unix::set_reuse_port(socket)
}

pub(crate) fn apply_ipv4_transparent(
    _socket: &socket2::Socket,
    config: &ServiceConfig,
) -> Result<(), Error> {
    match config.socket_options.ipv4_transparent {
        TransparentMode::Disabled | TransparentMode::BestEffort => Ok(()),
        TransparentMode::Required => Err(Error::UnsupportedPlatformOption(
            "ipv4 transparent sockets are only supported on Linux targets".to_string(),
        )),
    }
}

pub(crate) fn bind_quic_udp_reuseport_workers(
    _config: &ServiceConfig,
    _workers: usize,
) -> Result<Option<Vec<UdpSocket>>, Error> {
    Ok(None)
}

pub(crate) fn supports_quic_reuseport_bpf() -> bool {
    false
}

pub(crate) fn supports_reuse_port_balancing() -> bool {
    true
}
