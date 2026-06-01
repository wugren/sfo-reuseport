use std::net::UdpSocket;

use crate::core::{Error, SocketConfig, TransparentMode, UdpServiceConfig};

pub(crate) fn set_reuse_port(_socket: &socket2::Socket) -> Result<(), Error> {
    Ok(())
}

pub(crate) fn apply_transparent(
    _socket: &socket2::Socket,
    config: &impl SocketConfig,
) -> Result<(), Error> {
    if matches!(
        (
            config.socket_options().ipv4_transparent,
            config.socket_options().ipv6_transparent,
        ),
        (TransparentMode::Required, _) | (_, TransparentMode::Required)
    ) {
        Err(Error::UnsupportedPlatformOption(
            "transparent sockets are only supported on Linux targets".to_string(),
        ))
    } else {
        Ok(())
    }
}

pub(crate) fn bind_quic_udp_reuseport_workers(
    _config: &UdpServiceConfig,
    _workers: usize,
) -> Result<Option<Vec<UdpSocket>>, Error> {
    Ok(None)
}

pub(crate) fn supports_quic_reuseport_bpf() -> bool {
    false
}

pub(crate) fn supports_reuse_port_balancing() -> bool {
    false
}
