use sfo_reuseport::{Error, ServiceConfig};

#[test]
fn service_config_defaults_without_socket_init_callback() {
    let config = ServiceConfig::new("127.0.0.1:0".parse().unwrap());
    assert!(config.socket_init_callback.is_none());
}

#[test]
fn socket_init_callback_builder_sets_and_clears_callback() {
    let config = ServiceConfig::new("127.0.0.1:0".parse().unwrap())
        .with_socket_init_callback(|_socket| Ok(()));
    assert!(config.socket_init_callback.is_some());

    let config = config.without_socket_init_callback();
    assert!(config.socket_init_callback.is_none());
}

#[test]
fn socket_init_callback_error_is_distinguishable() {
    let error = Error::SocketInitCallback("custom option failed".to_string());
    assert!(error.to_string().contains("socket init callback"));
    assert!(error.to_string().contains("custom option failed"));
}
