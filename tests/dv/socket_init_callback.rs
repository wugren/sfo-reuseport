use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use sfo_reuseport::{Error, ServerRuntime, ServerRuntimeConfig, ServiceConfig, TcpServer, UdpServer};

#[tokio::test]
async fn tcp_bind_invokes_socket_init_callback_and_propagates_error() {
    let calls = Arc::new(AtomicUsize::new(0));
    let callback_calls = calls.clone();
    let config = ServiceConfig::new("127.0.0.1:0".parse().unwrap()).with_socket_init_callback(
        move |_socket| {
            callback_calls.fetch_add(1, Ordering::SeqCst);
            Err(Error::InvalidConfig("tcp callback failure".to_string()))
        },
    );

    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(1)).unwrap();
    let result = TcpServer::serve(&runtime, config, |_stream| async { Ok(()) });

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let message = match result {
        Ok(server) => {
            server.close().unwrap();
            panic!("tcp callback failure should prevent server startup");
        }
        Err(error) => error.to_string(),
    };
    assert!(message.contains("socket init callback"));
    assert!(message.contains("tcp callback failure"));
}

#[tokio::test]
async fn udp_bind_invokes_socket_init_callback_and_propagates_error() {
    let calls = Arc::new(AtomicUsize::new(0));
    let callback_calls = calls.clone();
    let config = ServiceConfig::new("127.0.0.1:0".parse().unwrap()).with_socket_init_callback(
        move |_socket| {
            callback_calls.fetch_add(1, Ordering::SeqCst);
            Err(Error::InvalidConfig("udp callback failure".to_string()))
        },
    );

    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(1)).unwrap();
    let result = UdpServer::serve(&runtime, config, |_socket, _meta, _payload| async { Ok(()) });

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let message = match result {
        Ok(server) => {
            server.close().unwrap();
            panic!("udp callback failure should prevent server startup");
        }
        Err(error) => error.to_string(),
    };
    assert!(message.contains("socket init callback"));
    assert!(message.contains("udp callback failure"));
}
