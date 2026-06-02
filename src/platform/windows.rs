use std::io;
use std::net::{SocketAddr, UdpSocket};

use crate::core::{Error, SocketConfig, TransparentMode, UdpServiceConfig};
use crate::runtime;

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

pub(crate) async fn recv_udp_original_dst(
    _socket: &runtime::UdpSocket,
    _buffer: Vec<u8>,
    _fallback_local_addr: SocketAddr,
) -> io::Result<(usize, SocketAddr, SocketAddr, Vec<u8>)> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "UDP original destination receive is unsupported by the selected platform or runtime",
    ))
}
