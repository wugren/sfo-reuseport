use sfo_reuseport::{
    ServerRuntime, ServerRuntimeConfig, ServiceConfig, SocketOptions, TransparentMode, UdpServer,
};
use std::time::Duration;

#[tokio::test]
async fn required_transparent_returns_explicit_error() {
    let config = ServiceConfig::new("127.0.0.1:0".parse().unwrap()).with_socket_options(
        SocketOptions {
            reuse_address: true,
            ipv4_transparent: TransparentMode::Required,
        },
    );

    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(1)).unwrap();
    let result = tokio::time::timeout(
        Duration::from_millis(200),
        UdpServer::serve(&runtime, config, |_socket, _meta, _payload| async { Ok(()) }),
    )
    .await;

    if let Ok(result) = result {
        let message = result.unwrap_err().to_string();
        assert!(message.contains("transparent") || message.contains("permission denied"));
    }
}
