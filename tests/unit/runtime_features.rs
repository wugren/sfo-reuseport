use sfo_reuseport::{TcpStream, UdpSocket};

fn assert_send<T: Send>() {}

#[test]
fn runtime_socket_types_are_public_and_send() {
    assert_send::<TcpStream>();
    assert_send::<UdpSocket>();
}
