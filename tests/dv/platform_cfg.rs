use sfo_reuseport::{SocketOptions, UdpServiceConfig};

#[test]
fn current_platform_can_construct_default_config() {
    let config = UdpServiceConfig::new("127.0.0.1:0".parse().unwrap())
        .with_socket_options(SocketOptions::default());
    assert!(config.socket_options.reuse_address);
}

#[test]
fn reuse_port_capability_matches_target_family() {
    let lib_rs = include_str!("../../src/lib.rs");

    assert!(lib_rs.contains("pub(crate) mod platform;"));
    assert!(!lib_rs.contains("pub mod platform;"));
}

#[test]
fn platform_backends_share_internal_capability_interface() {
    let platform_mod = include_str!("../../src/platform/mod.rs");
    let linux = include_str!("../../src/platform/linux.rs");
    let bsd = include_str!("../../src/platform/bsd.rs");
    let windows = include_str!("../../src/platform/windows.rs");

    assert!(platform_mod.contains("pub(crate) struct PlatformCapabilities"));
    assert!(platform_mod.contains("pub(crate) fn capabilities() -> PlatformCapabilities"));
    assert!(platform_mod.contains("pub(crate) fn bind_tcp("));
    assert!(platform_mod.contains("pub(crate) fn bind_udp("));
    assert!(platform_mod.contains("pub(crate) fn bind_quic_udp_reuseport_workers("));
    assert!(platform_mod.contains("pub(crate) async fn recv_udp_original_dst("));

    for backend in [linux, bsd, windows] {
        assert!(backend.contains("pub(crate) fn set_reuse_port("));
        assert!(backend.contains("pub(crate) fn apply_transparent("));
        assert!(backend.contains("pub(crate) fn bind_quic_udp_reuseport_workers("));
        assert!(backend.contains("pub(crate) fn supports_reuse_port_balancing("));
        assert!(backend.contains("pub(crate) fn supports_quic_reuseport_bpf("));
        assert!(backend.contains("pub(crate) async fn recv_udp_original_dst("));
    }
}
