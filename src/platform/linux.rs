use std::io;
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::os::fd::AsRawFd;
use std::time::{Duration, Instant};

use crate::core::{Error, SocketConfig, TransparentMode, UdpServiceConfig};
use crate::runtime;

pub(crate) fn set_reuse_port(socket: &socket2::Socket) -> Result<(), Error> {
    super::unix::set_reuse_port(socket)
}

pub(crate) fn apply_transparent(
    socket: &socket2::Socket,
    config: &impl SocketConfig,
) -> Result<(), Error> {
    apply_ipv4_transparent(socket, config)?;
    apply_ipv6_transparent(socket, config)?;
    Ok(())
}

fn apply_ipv4_transparent(socket: &socket2::Socket, config: &impl SocketConfig) -> Result<(), Error> {
    match config.socket_options().ipv4_transparent {
        TransparentMode::Disabled => Ok(()),
        TransparentMode::BestEffort if !config.bind_addr().is_ipv4() => Ok(()),
        TransparentMode::Required if !config.bind_addr().is_ipv4() => Err(
            Error::UnsupportedPlatformOption(
                "ipv4 transparent requires an IPv4 bind address".to_string(),
            ),
        ),
        TransparentMode::BestEffort => {
            let _ = socket.set_ip_transparent_v4(true);
            let _ = set_socket_opt_int(socket, libc::SOL_IP, libc::IP_ORIGDSTADDR, 1);
            let _ = set_socket_opt_int(socket, libc::SOL_IP, libc::IP_FREEBIND, 1);
            Ok(())
        }
        TransparentMode::Required => {
            socket.set_ip_transparent_v4(true).map_err(Error::from)?;
            set_socket_opt_int(socket, libc::SOL_IP, libc::IP_ORIGDSTADDR, 1)
                .map_err(Error::from)?;
            set_socket_opt_int(socket, libc::SOL_IP, libc::IP_FREEBIND, 1).map_err(Error::from)
        }
    }
}

fn apply_ipv6_transparent(socket: &socket2::Socket, config: &impl SocketConfig) -> Result<(), Error> {
    match config.socket_options().ipv6_transparent {
        TransparentMode::Disabled => Ok(()),
        TransparentMode::BestEffort if !config.bind_addr().is_ipv6() => Ok(()),
        TransparentMode::Required if !config.bind_addr().is_ipv6() => Err(
            Error::UnsupportedPlatformOption(
                "ipv6 transparent requires an IPv6 bind address".to_string(),
            ),
        ),
        TransparentMode::BestEffort => {
            let _ = socket.set_ip_transparent_v6(true);
            let _ = set_socket_opt_int(socket, libc::SOL_IPV6, libc::IPV6_RECVORIGDSTADDR, 1);
            let _ = set_socket_opt_int(socket, libc::SOL_IPV6, libc::IPV6_FREEBIND, 1);
            Ok(())
        }
        TransparentMode::Required => {
            socket.set_ip_transparent_v6(true).map_err(Error::from)?;
            set_socket_opt_int(socket, libc::SOL_IPV6, libc::IPV6_RECVORIGDSTADDR, 1)
                .map_err(Error::from)?;
            set_socket_opt_int(socket, libc::SOL_IPV6, libc::IPV6_FREEBIND, 1).map_err(Error::from)
        }
    }
}

#[allow(unsafe_code)]
fn set_socket_opt_int<T: AsRawFd>(
    socket: &T,
    level: libc::c_int,
    name: libc::c_int,
    value: libc::c_int,
) -> io::Result<()> {
    let result = unsafe {
        libc::setsockopt(
            socket.as_raw_fd(),
            level,
            name,
            (&value as *const libc::c_int).cast(),
            std::mem::size_of::<libc::c_int>() as libc::socklen_t,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(all(target_os = "linux", feature = "runtime-tokio"))]
pub(crate) async fn recv_udp_original_dst(
    socket: &runtime::UdpSocket,
    mut buffer: Vec<u8>,
    fallback_local_addr: SocketAddr,
) -> io::Result<(usize, SocketAddr, SocketAddr, Vec<u8>)> {
    let (len, peer_addr, local_addr) = socket
        .async_io(tokio::io::Interest::READABLE, || {
            recv_udp_original_dst_raw(socket.as_ref(), buffer.as_mut_slice(), fallback_local_addr)
        })
        .await?;
    Ok((len, peer_addr, local_addr, buffer))
}

#[cfg(not(all(target_os = "linux", feature = "runtime-tokio")))]
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

#[cfg(all(target_os = "linux", feature = "runtime-tokio"))]
#[allow(unsafe_code)]
fn recv_udp_original_dst_raw<T: AsRawFd>(
    socket: &T,
    buffer: &mut [u8],
    fallback_local_addr: SocketAddr,
) -> io::Result<(usize, SocketAddr, SocketAddr)> {
    unsafe {
        let mut control = [0_u8; 128];
        let mut source: libc::sockaddr_storage = std::mem::zeroed();
        let mut iov = libc::iovec {
            iov_base: buffer.as_mut_ptr().cast(),
            iov_len: buffer.len(),
        };
        let mut message: libc::msghdr = std::mem::zeroed();
        message.msg_name = (&mut source as *mut libc::sockaddr_storage).cast();
        message.msg_namelen = std::mem::size_of_val(&source) as libc::socklen_t;
        message.msg_iov = &mut iov;
        message.msg_iovlen = 1;
        message.msg_control = control.as_mut_ptr().cast();
        message.msg_controllen = control.len();

        let len = libc::recvmsg(socket.as_raw_fd(), &mut message, 0);
        if len < 0 {
            return Err(io::Error::last_os_error());
        }

        let destination = match original_destination_addr(&message) {
            Some(destination) => sockaddr_storage_to_socket_addr(&destination)?,
            None => fallback_local_addr,
        };

        Ok((
            len as usize,
            sockaddr_storage_to_socket_addr(&source)?,
            destination,
        ))
    }
}

#[cfg(all(target_os = "linux", feature = "runtime-tokio"))]
#[allow(unsafe_code)]
unsafe fn original_destination_addr(message: &libc::msghdr) -> Option<libc::sockaddr_storage> {
    let mut cmsg = unsafe { libc::CMSG_FIRSTHDR(message) };
    while !cmsg.is_null() {
        let header = unsafe { &*cmsg };
        let copy_len = match (header.cmsg_level, header.cmsg_type) {
            (libc::SOL_IP, libc::IP_ORIGDSTADDR) => std::mem::size_of::<libc::sockaddr_in>(),
            (libc::SOL_IPV6, libc::IPV6_RECVORIGDSTADDR) => {
                std::mem::size_of::<libc::sockaddr_in6>()
            }
            _ => {
                cmsg = unsafe { libc::CMSG_NXTHDR(message, cmsg) };
                continue;
            }
        };

        let mut addr: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
        unsafe {
            std::ptr::copy_nonoverlapping(
                libc::CMSG_DATA(cmsg),
                (&mut addr as *mut libc::sockaddr_storage).cast(),
                copy_len,
            );
        }
        return Some(addr);
    }
    None
}

#[cfg(all(target_os = "linux", feature = "runtime-tokio"))]
#[allow(unsafe_code)]
fn sockaddr_storage_to_socket_addr(addr: &libc::sockaddr_storage) -> io::Result<SocketAddr> {
    match i32::from(addr.ss_family) {
        libc::AF_INET => {
            let addr =
                unsafe { &*(addr as *const libc::sockaddr_storage).cast::<libc::sockaddr_in>() };
            let ip = IpAddr::from(addr.sin_addr.s_addr.to_ne_bytes());
            Ok(SocketAddr::new(ip, u16::from_be(addr.sin_port)))
        }
        libc::AF_INET6 => {
            let addr =
                unsafe { &*(addr as *const libc::sockaddr_storage).cast::<libc::sockaddr_in6>() };
            let ip = IpAddr::from(addr.sin6_addr.s6_addr);
            Ok(SocketAddr::new(ip, u16::from_be(addr.sin6_port)))
        }
        family => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unsupported socket address family {family}"),
        )),
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn bind_quic_udp_reuseport_workers(
    config: &UdpServiceConfig,
    workers: usize,
) -> Result<Option<Vec<UdpSocket>>, Error> {
    if std::env::var_os("SFO_REUSEPORT_DISABLE_QUIC_BPF").is_some() {
        return Ok(None);
    }

    if workers == 0 {
        return Err(Error::InvalidConfig(
            "worker count must be greater than zero".to_string(),
        ));
    }

    let first = match super::bind_udp(config) {
        Ok(socket) => socket,
        Err(_) => return Ok(None),
    };
    let bind_addr = first.local_addr().map_err(Error::from)?;
    let mut sockets = Vec::with_capacity(workers);
    sockets.push(first);

    let mut worker_config = config.clone();
    worker_config.bind_addr = bind_addr;
    for _ in 1..workers {
        let socket = match super::bind_udp(&worker_config) {
            Ok(socket) => socket,
            Err(_) => return Ok(None),
        };
        sockets.push(socket);
    }

    if (attach_quic_reuseport_ebpf(&sockets[0], workers).is_ok()
        || attach_quic_reuseport_cbpf(&sockets[0], workers).is_ok())
        && quic_reuseport_selector_routes_probe(&sockets, bind_addr).is_ok()
    {
        Ok(Some(sockets))
    } else {
        Ok(None)
    }
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn bind_quic_udp_reuseport_workers(
    _config: &UdpServiceConfig,
    _workers: usize,
) -> Result<Option<Vec<UdpSocket>>, Error> {
    Ok(None)
}

pub(crate) fn supports_quic_reuseport_bpf() -> bool {
    cfg!(target_os = "linux")
}

pub(crate) fn supports_reuse_port_balancing() -> bool {
    true
}

#[cfg(target_os = "linux")]
fn quic_reuseport_selector_routes_probe(
    sockets: &[UdpSocket],
    bind_addr: SocketAddr,
) -> io::Result<()> {
    let target = routable_probe_addr(bind_addr);
    let sender = UdpSocket::bind(match target {
        SocketAddr::V4(_) => "127.0.0.1:0",
        SocketAddr::V6(_) => "[::1]:0",
    })?;
    if sockets.is_empty() || sockets.len() > u16::MAX as usize + 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "worker count must fit the QUIC CID worker index prefix",
        ));
    }

    for socket in sockets {
        socket.set_nonblocking(true)?;
    }

    for expected_socket in 0..sockets.len() {
        let probe_packet = quic_reuseport_probe_packet(expected_socket);
        sender.send_to(&probe_packet, target)?;
        quic_reuseport_selector_wait_for_probe(sockets, expected_socket, &probe_packet)?;
    }

    quic_reuseport_selector_drain_probe_packets(sockets)?;

    Ok(())
}

#[cfg(target_os = "linux")]
fn quic_reuseport_selector_wait_for_probe(
    sockets: &[UdpSocket],
    expected_socket: usize,
    probe_packet: &[u8],
) -> io::Result<()> {
    let deadline = Instant::now() + Duration::from_millis(100);
    let mut buffer = [0_u8; 16];
    while Instant::now() < deadline {
        for (index, socket) in sockets.iter().enumerate() {
            match socket.recv_from(&mut buffer) {
                Ok((len, _)) if buffer[..len] == probe_packet[..] && index == expected_socket => {
                    return Ok(());
                }
                Ok((len, _)) if buffer[..len] == probe_packet[..] => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "QUIC reuse-port selector probe routed to unexpected worker",
                    ));
                }
                Ok(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "QUIC reuse-port selector probe received unexpected packet",
                    ));
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(error),
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }

    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        "QUIC reuse-port selector probe timed out",
    ))
}

#[cfg(target_os = "linux")]
fn quic_reuseport_probe_packet(expected_socket: usize) -> Vec<u8> {
    let worker_index = expected_socket as u16;
    let [high, low] = worker_index.to_be_bytes();
    vec![0xe0, 0, 0, 0, 1, 4, high, low, 0xa5, low]
}

#[cfg(target_os = "linux")]
fn quic_reuseport_selector_drain_probe_packets(sockets: &[UdpSocket]) -> io::Result<()> {
    let mut buffer = [0_u8; 16];
    for socket in sockets {
        loop {
            match socket.recv_from(&mut buffer) {
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => return Err(error),
            }
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn routable_probe_addr(addr: SocketAddr) -> SocketAddr {
    if !addr.ip().is_unspecified() {
        return addr;
    }

    match addr {
        SocketAddr::V4(addr) => SocketAddr::new(IpAddr::from([127, 0, 0, 1]), addr.port()),
        SocketAddr::V6(addr) => {
            SocketAddr::new(IpAddr::from([0, 0, 0, 0, 0, 0, 0, 1]), addr.port())
        }
    }
}

#[cfg(target_os = "linux")]
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SockFilter {
    code: u16,
    jt: u8,
    jf: u8,
    k: u32,
}

#[cfg(target_os = "linux")]
#[repr(C)]
struct SockFprog {
    len: u16,
    filter: *const SockFilter,
}

#[cfg(target_os = "linux")]
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BpfInsn {
    code: u8,
    dst_src: u8,
    off: i16,
    imm: i32,
}

#[cfg(target_os = "linux")]
#[repr(C)]
#[derive(Default)]
struct BpfProgLoadAttr {
    prog_type: u32,
    insn_cnt: u32,
    insns: u64,
    license: u64,
    log_level: u32,
    log_size: u32,
    log_buf: u64,
    kern_version: u32,
    prog_flags: u32,
    prog_name: [u8; 16],
    prog_ifindex: u32,
    expected_attach_type: u32,
}

#[cfg(target_os = "linux")]
fn quic_reuseport_ebpf(workers: usize) -> Vec<BpfInsn> {
    const BPF_LDX: u8 = 0x01;
    const BPF_ALU64: u8 = 0x07;
    const BPF_JMP: u8 = 0x05;
    const BPF_DW: u8 = 0x18;
    const BPF_B: u8 = 0x10;
    const BPF_MEM: u8 = 0x60;
    const BPF_K: u8 = 0x00;
    const BPF_X: u8 = 0x08;
    const BPF_ADD: u8 = 0x00;
    const BPF_OR: u8 = 0x40;
    const BPF_AND: u8 = 0x50;
    const BPF_LSH: u8 = 0x60;
    const BPF_MOD: u8 = 0x90;
    const BPF_MOV: u8 = 0xb0;
    const BPF_JEQ: u8 = 0x10;
    const BPF_JGT: u8 = 0x20;
    const BPF_JLT: u8 = 0xa0;
    const BPF_EXIT: u8 = 0x90;

    vec![
        bpf_insn(BPF_ALU64 | BPF_MOV | BPF_X, 6, 1, 0, 0),
        bpf_insn(BPF_LDX | BPF_DW | BPF_MEM, 2, 6, 0, 0),
        bpf_insn(BPF_LDX | BPF_DW | BPF_MEM, 3, 6, 8, 0),
        bpf_insn(BPF_ALU64 | BPF_MOV | BPF_X, 4, 2, 0, 0),
        bpf_insn(BPF_ALU64 | BPF_ADD | BPF_K, 4, 0, 0, 1),
        bpf_insn(BPF_JMP | BPF_JGT | BPF_X, 4, 3, 26, 0),
        bpf_insn(BPF_LDX | BPF_B | BPF_MEM, 0, 2, 0, 0),
        bpf_insn(BPF_ALU64 | BPF_AND | BPF_K, 0, 0, 0, 0x80),
        bpf_insn(BPF_JMP | BPF_JEQ | BPF_K, 0, 0, 14, 0),
        bpf_insn(BPF_ALU64 | BPF_MOV | BPF_X, 4, 2, 0, 0),
        bpf_insn(BPF_ALU64 | BPF_ADD | BPF_K, 4, 0, 0, 6),
        bpf_insn(BPF_JMP | BPF_JGT | BPF_X, 4, 3, 20, 0),
        bpf_insn(BPF_LDX | BPF_B | BPF_MEM, 4, 2, 5, 0),
        bpf_insn(BPF_JMP | BPF_JLT | BPF_K, 4, 0, 18, 2),
        bpf_insn(BPF_ALU64 | BPF_MOV | BPF_X, 5, 2, 0, 0),
        bpf_insn(BPF_ALU64 | BPF_ADD | BPF_K, 5, 0, 0, 8),
        bpf_insn(BPF_JMP | BPF_JGT | BPF_X, 5, 3, 15, 0),
        bpf_insn(BPF_LDX | BPF_B | BPF_MEM, 0, 2, 6, 0),
        bpf_insn(BPF_ALU64 | BPF_LSH | BPF_K, 0, 0, 0, 8),
        bpf_insn(BPF_LDX | BPF_B | BPF_MEM, 4, 2, 7, 0),
        bpf_insn(BPF_ALU64 | BPF_OR | BPF_X, 0, 4, 0, 0),
        bpf_insn(BPF_ALU64 | BPF_MOD | BPF_K, 0, 0, 0, workers as i32),
        bpf_insn(BPF_JMP | BPF_EXIT, 0, 0, 0, 0),
        bpf_insn(BPF_ALU64 | BPF_MOV | BPF_X, 4, 2, 0, 0),
        bpf_insn(BPF_ALU64 | BPF_ADD | BPF_K, 4, 0, 0, 3),
        bpf_insn(BPF_JMP | BPF_JGT | BPF_X, 4, 3, 6, 0),
        bpf_insn(BPF_LDX | BPF_B | BPF_MEM, 0, 2, 1, 0),
        bpf_insn(BPF_ALU64 | BPF_LSH | BPF_K, 0, 0, 0, 8),
        bpf_insn(BPF_LDX | BPF_B | BPF_MEM, 4, 2, 2, 0),
        bpf_insn(BPF_ALU64 | BPF_OR | BPF_X, 0, 4, 0, 0),
        bpf_insn(BPF_ALU64 | BPF_MOD | BPF_K, 0, 0, 0, workers as i32),
        bpf_insn(BPF_JMP | BPF_EXIT, 0, 0, 0, 0),
        bpf_insn(BPF_ALU64 | BPF_MOV | BPF_K, 0, 0, 0, 0),
        bpf_insn(BPF_JMP | BPF_EXIT, 0, 0, 0, 0),
    ]
}

#[cfg(target_os = "linux")]
fn bpf_insn(code: u8, dst: u8, src: u8, off: i16, imm: i32) -> BpfInsn {
    BpfInsn {
        code,
        dst_src: dst | (src << 4),
        off,
        imm,
    }
}

#[cfg(target_os = "linux")]
fn quic_reuseport_cbpf(workers: usize) -> Vec<SockFilter> {
    const BPF_LD: u16 = 0x00;
    const BPF_ALU: u16 = 0x04;
    const BPF_JMP: u16 = 0x05;
    const BPF_RET: u16 = 0x06;
    const BPF_H: u16 = 0x08;
    const BPF_B: u16 = 0x10;
    const BPF_ABS: u16 = 0x20;
    const BPF_K: u16 = 0x00;
    const BPF_A: u16 = 0x10;
    const BPF_JA: u16 = 0x00;
    const BPF_JSET: u16 = 0x40;
    const BPF_MOD: u16 = 0x90;

    vec![
        sock_filter(BPF_LD | BPF_B | BPF_ABS, 0, 0, 0),
        sock_filter(BPF_JMP | BPF_JSET | BPF_K, 0, 2, 0x80),
        sock_filter(BPF_LD | BPF_H | BPF_ABS, 0, 0, 6),
        sock_filter(BPF_JMP | BPF_JA, 0, 0, 1),
        sock_filter(BPF_LD | BPF_H | BPF_ABS, 0, 0, 1),
        sock_filter(BPF_ALU | BPF_MOD | BPF_K, 0, 0, workers as u32),
        sock_filter(BPF_RET | BPF_A, 0, 0, 0),
    ]
}

#[cfg(target_os = "linux")]
fn sock_filter(code: u16, jt: u8, jf: u8, k: u32) -> SockFilter {
    SockFilter { code, jt, jf, k }
}

#[cfg(target_os = "linux")]
fn sys_bpf_number() -> Option<core::ffi::c_long> {
    #[cfg(target_arch = "x86_64")]
    {
        Some(321)
    }
    #[cfg(target_arch = "aarch64")]
    {
        Some(280)
    }
    #[cfg(target_arch = "riscv64")]
    {
        Some(280)
    }
    #[cfg(target_arch = "arm")]
    {
        Some(386)
    }
    #[cfg(target_arch = "powerpc64")]
    {
        Some(361)
    }
    #[cfg(target_arch = "s390x")]
    {
        Some(351)
    }
    #[cfg(not(any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "riscv64",
        target_arch = "arm",
        target_arch = "powerpc64",
        target_arch = "s390x"
    )))]
    {
        None
    }
}

#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
fn load_quic_reuseport_ebpf(workers: usize) -> Result<i32, Error> {
    use core::ffi::{c_long, c_void};
    use std::io;

    const BPF_PROG_LOAD: c_long = 5;
    const BPF_PROG_TYPE_SK_REUSEPORT: u32 = 21;
    const LICENSE: &[u8] = b"GPL\0";

    unsafe extern "C" {
        fn syscall(number: c_long, ...) -> c_long;
    }

    let Some(sys_bpf) = sys_bpf_number() else {
        return Err(Error::UnsupportedPlatformOption(
            "bpf syscall number is not defined for this target architecture".to_string(),
        ));
    };

    let program = quic_reuseport_ebpf(workers);
    let attr = BpfProgLoadAttr {
        prog_type: BPF_PROG_TYPE_SK_REUSEPORT,
        insn_cnt: program.len() as u32,
        insns: program.as_ptr() as u64,
        license: LICENSE.as_ptr() as u64,
        ..BpfProgLoadAttr::default()
    };

    let fd = unsafe {
        syscall(
            sys_bpf,
            BPF_PROG_LOAD,
            (&attr as *const BpfProgLoadAttr).cast::<c_void>(),
            std::mem::size_of::<BpfProgLoadAttr>(),
        )
    };
    if fd >= 0 {
        Ok(fd as i32)
    } else {
        Err(Error::from(io::Error::last_os_error()))
    }
}

#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
fn attach_quic_reuseport_ebpf(socket: &UdpSocket, workers: usize) -> Result<(), Error> {
    use core::ffi::{c_int, c_void};
    use std::io;
    use std::os::fd::AsRawFd;

    const SOL_SOCKET: c_int = 1;
    const SO_ATTACH_REUSEPORT_EBPF: c_int = 52;

    unsafe extern "C" {
        fn setsockopt(
            socket: c_int,
            level: c_int,
            option_name: c_int,
            option_value: *const c_void,
            option_len: u32,
        ) -> c_int;
        fn close(fd: c_int) -> c_int;
    }

    let program_fd = load_quic_reuseport_ebpf(workers)?;
    let result = unsafe {
        setsockopt(
            socket.as_raw_fd(),
            SOL_SOCKET,
            SO_ATTACH_REUSEPORT_EBPF,
            (&program_fd as *const c_int).cast(),
            std::mem::size_of::<c_int>() as u32,
        )
    };
    let error = io::Error::last_os_error();
    let _ = unsafe { close(program_fd) };
    if result == 0 {
        Ok(())
    } else {
        Err(Error::from(error))
    }
}

#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
fn attach_quic_reuseport_cbpf(socket: &UdpSocket, workers: usize) -> Result<(), Error> {
    use core::ffi::{c_int, c_void};
    use std::io;
    use std::os::fd::AsRawFd;

    const SOL_SOCKET: c_int = 1;
    const SO_ATTACH_REUSEPORT_CBPF: c_int = 51;

    unsafe extern "C" {
        fn setsockopt(
            socket: c_int,
            level: c_int,
            option_name: c_int,
            option_value: *const c_void,
            option_len: u32,
        ) -> c_int;
    }

    let filter = quic_reuseport_cbpf(workers);
    let program = SockFprog {
        len: filter.len() as u16,
        filter: filter.as_ptr(),
    };
    let result = unsafe {
        setsockopt(
            socket.as_raw_fd(),
            SOL_SOCKET,
            SO_ATTACH_REUSEPORT_CBPF,
            (&program as *const SockFprog).cast(),
            std::mem::size_of::<SockFprog>() as u32,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(Error::from(io::Error::last_os_error()))
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn quic_reuseport_cbpf_instruction_shape_is_covered() {
        let filter = quic_reuseport_cbpf(4);

        assert_eq!(filter[0], sock_filter(0x00 | 0x10 | 0x20, 0, 0, 0));
        assert_eq!(filter[1], sock_filter(0x05 | 0x40 | 0x00, 0, 2, 0x80));
        assert_eq!(filter[2], sock_filter(0x00 | 0x08 | 0x20, 0, 0, 6));
        assert_eq!(filter[4], sock_filter(0x00 | 0x08 | 0x20, 0, 0, 1));
        assert_eq!(filter[5], sock_filter(0x04 | 0x90 | 0x00, 0, 0, 4));
        assert_eq!(filter[6], sock_filter(0x06 | 0x10, 0, 0, 0));
    }

    #[test]
    fn quic_reuseport_ebpf_instruction_shape_is_covered() {
        let program = quic_reuseport_ebpf(4);

        assert_eq!(program[0], bpf_insn(0x07 | 0xb0 | 0x08, 6, 1, 0, 0));
        assert_eq!(program[8], bpf_insn(0x05 | 0x10 | 0x00, 0, 0, 14, 0));
        assert_eq!(program[17], bpf_insn(0x01 | 0x10 | 0x60, 0, 2, 6, 0));
        assert_eq!(program[21], bpf_insn(0x07 | 0x90 | 0x00, 0, 0, 0, 4));
        assert_eq!(program[26], bpf_insn(0x01 | 0x10 | 0x60, 0, 2, 1, 0));
        assert_eq!(program[30], bpf_insn(0x07 | 0x90 | 0x00, 0, 0, 0, 4));
        assert_eq!(program[33], bpf_insn(0x05 | 0x90, 0, 0, 0, 0));
    }

    #[test]
    fn quic_reuseport_probe_uses_fixed_two_byte_selector() {
        let expected_socket = 3;
        let packet = quic_reuseport_probe_packet(expected_socket);

        assert_eq!(packet[0], 0xe0);
        assert_eq!(packet[5], 4);
        assert_eq!(u16::from_be_bytes([packet[6], packet[7]]) as usize % 4, expected_socket);
    }
}
