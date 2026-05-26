use sfo_reuseport::{ServiceConfig, SocketOptions};

#[test]
fn current_platform_can_construct_default_config() {
    let config = ServiceConfig::new("127.0.0.1:0".parse().unwrap())
        .with_socket_options(SocketOptions::default());
    assert!(config.socket_options.reuse_address);
}

#[test]
fn reuse_port_capability_matches_target_family() {
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "freebsd"))]
    assert!(sfo_reuseport::platform::supports_reuse_port_balancing());

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "freebsd")))]
    assert!(!sfo_reuseport::platform::supports_reuse_port_balancing());
}

#[test]
fn quic_reuseport_bpf_capability_matches_linux_target() {
    #[cfg(target_os = "linux")]
    assert!(sfo_reuseport::platform::supports_quic_reuseport_bpf());

    #[cfg(not(target_os = "linux"))]
    assert!(!sfo_reuseport::platform::supports_quic_reuseport_bpf());
}
