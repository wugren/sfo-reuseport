use std::net::{TcpListener, UdpSocket};

use crate::core::{Error, ServiceConfig};

#[cfg(any(target_os = "macos", target_os = "freebsd"))]
mod bsd;
#[cfg(any(target_os = "linux", target_os = "android"))]
mod linux;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(any(target_os = "linux", target_os = "android"))]
use linux as imp;
#[cfg(any(target_os = "macos", target_os = "freebsd"))]
use bsd as imp;
#[cfg(windows)]
use windows as imp;

pub(crate) fn bind_tcp(config: &ServiceConfig) -> Result<TcpListener, Error> {
    let socket = socket2::Socket::new(
        socket2::Domain::for_address(config.bind_addr),
        socket2::Type::STREAM,
        Some(socket2::Protocol::TCP),
    )
    .map_err(Error::from)?;
    apply_socket_init_callback(&socket, config)?;
    apply_common_options(&socket, config)?;
    socket
        .bind(&config.bind_addr.into())
        .map_err(Error::from)?;
    socket.listen(1024).map_err(Error::from)?;
    let listener: TcpListener = socket.into();
    Ok(listener)
}

pub(crate) fn bind_tcp_workers(
    config: &ServiceConfig,
    workers: usize,
) -> Result<Vec<TcpListener>, Error> {
    if supports_reuse_port_balancing() {
        (0..workers).map(|_| bind_tcp(config)).collect()
    } else {
        Ok(vec![bind_tcp(config)?])
    }
}

pub(crate) fn bind_udp(config: &ServiceConfig) -> Result<UdpSocket, Error> {
    let socket = socket2::Socket::new(
        socket2::Domain::for_address(config.bind_addr),
        socket2::Type::DGRAM,
        Some(socket2::Protocol::UDP),
    )
    .map_err(Error::from)?;
    apply_socket_init_callback(&socket, config)?;
    apply_common_options(&socket, config)?;
    socket
        .bind(&config.bind_addr.into())
        .map_err(Error::from)?;
    let socket: UdpSocket = socket.into();
    Ok(socket)
}

pub(crate) fn bind_udp_workers(
    config: &ServiceConfig,
    workers: usize,
) -> Result<Vec<UdpSocket>, Error> {
    if supports_reuse_port_balancing() {
        (0..workers).map(|_| bind_udp(config)).collect()
    } else {
        Ok(vec![bind_udp(config)?])
    }
}

pub(crate) fn bind_quic_udp_reuseport_workers(
    config: &ServiceConfig,
    workers: usize,
) -> Result<Option<Vec<UdpSocket>>, Error> {
    imp::bind_quic_udp_reuseport_workers(config, workers)
}

pub fn supports_quic_reuseport_bpf() -> bool {
    imp::supports_quic_reuseport_bpf()
}

pub fn supports_reuse_port_balancing() -> bool {
    imp::supports_reuse_port_balancing()
}

fn apply_common_options(socket: &socket2::Socket, config: &ServiceConfig) -> Result<(), Error> {
    socket
        .set_reuse_address(config.socket_options.reuse_address)
        .map_err(Error::from)?;
    imp::set_reuse_port(socket)?;

    imp::apply_ipv4_transparent(socket, config)?;

    Ok(())
}

fn apply_socket_init_callback(
    socket: &socket2::Socket,
    config: &ServiceConfig,
) -> Result<(), Error> {
    if let Some(callback) = &config.socket_init_callback {
        callback(socket).map_err(|error| Error::SocketInitCallback(error.to_string()))?;
    }
    Ok(())
}
