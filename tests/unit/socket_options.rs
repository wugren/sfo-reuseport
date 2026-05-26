use sfo_reuseport::{SocketOptions, TransparentMode};

#[test]
fn socket_options_default_to_reuse_address_without_transparent() {
    let options = SocketOptions::default();
    assert!(options.reuse_address);
    assert_eq!(options.ipv4_transparent, TransparentMode::Disabled);
}
