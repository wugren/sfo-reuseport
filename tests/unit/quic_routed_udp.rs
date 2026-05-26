use std::future::Future;

use sfo_reuseport::{Error, PacketMeta, QuicServer, UdpSocket};

fn assert_quic_udp_handler<F, Fut>(_handler: F)
where
    F: Fn(UdpSocket, PacketMeta, Vec<u8>) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<(), Error>> + Send + 'static,
{
}

#[test]
fn quic_server_is_a_udp_packet_routing_entrypoint() {
    let _server = QuicServer;
    assert_quic_udp_handler(|_socket, _meta, _payload| async { Ok(()) });
}
