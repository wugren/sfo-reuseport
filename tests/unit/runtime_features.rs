use sfo_reuseport::{TcpStream, UdpSocket};

fn assert_send<T: Send>() {}

#[cfg(not(feature = "runtime-tokio-uring"))]
#[test]
fn runtime_socket_types_are_public_and_send() {
    assert_send::<TcpStream>();
    assert_send::<UdpSocket>();
}

#[cfg(feature = "runtime-tokio-uring")]
#[test]
fn tokio_uring_runtime_uses_tokio_network_socket_types() {
    assert_send::<TcpStream>();
    assert_send::<UdpSocket>();

    let tcp_stream_type = std::any::type_name::<TcpStream>();
    assert!(
        tcp_stream_type.contains("tokio::net"),
        "runtime-tokio-uring TcpStream should use tokio net, got {tcp_stream_type}"
    );
}
