use sfo_reuseport::{TcpStream, UdpSocket};

#[cfg(not(feature = "runtime-tokio-uring"))]
fn assert_send<T: Send>() {}

#[cfg(not(feature = "runtime-tokio-uring"))]
#[test]
fn runtime_socket_types_are_public_and_send() {
    assert_send::<TcpStream>();
    assert_send::<UdpSocket>();
}

#[cfg(feature = "runtime-tokio-uring")]
#[test]
fn tokio_uring_runtime_socket_types_are_public() {
    let _ = std::any::type_name::<TcpStream>();
    let _ = std::any::type_name::<UdpSocket>();
}
