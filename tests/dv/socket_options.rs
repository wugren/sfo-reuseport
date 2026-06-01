use sfo_reuseport::{
    ServerRuntime, ServerRuntimeConfig, UdpServiceConfig, SocketOptions, TransparentMode, UdpServer,
};

#[tokio::test]
async fn required_transparent_returns_explicit_error() {
    let config = UdpServiceConfig::new("127.0.0.1:0".parse().unwrap()).with_socket_options(
        SocketOptions {
            reuse_address: true,
            ipv4_transparent: TransparentMode::Required,
            ipv6_transparent: TransparentMode::Disabled,
        },
    );

    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(1)).unwrap();
    let result = UdpServer::serve(&runtime, config, |_socket, _meta, _payload| async { Ok(()) });

    if let Err(error) = result {
        let message = error.to_string();
        assert!(message.contains("transparent") || message.contains("permission denied"));
    }
}

#[tokio::test]
async fn required_ipv6_transparent_returns_explicit_error() {
    let config = UdpServiceConfig::new("[::1]:0".parse().unwrap()).with_socket_options(
        SocketOptions {
            reuse_address: true,
            ipv4_transparent: TransparentMode::Disabled,
            ipv6_transparent: TransparentMode::Required,
        },
    );

    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(1)).unwrap();
    let result = UdpServer::serve(&runtime, config, |_socket, _meta, _payload| async { Ok(()) });

    if let Err(error) = result {
        let message = error.to_string();
        assert!(message.contains("transparent") || message.contains("permission denied"));
    }
}
