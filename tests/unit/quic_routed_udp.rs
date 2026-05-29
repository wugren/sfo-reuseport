use std::future::Future;

use sfo_reuseport::{Error, PacketMeta, QuicCidGenerator, UdpSocket};

fn assert_quic_udp_handler<F, Fut>(_handler: F)
where
    F: Fn(UdpSocket, PacketMeta, Vec<u8>) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<(), Error>> + Send + 'static,
{
}

#[test]
fn quic_server_is_a_udp_packet_routing_entrypoint() {
    assert_quic_udp_handler(|_socket, _meta, _payload| async { Ok(()) });
}

#[test]
fn quic_cid_generator_writes_two_byte_worker_prefix_and_random_suffix() {
    let generator = QuicCidGenerator::new(7).unwrap();

    let first = generator.generate().unwrap();
    let second = generator.generate().unwrap();

    assert_eq!(first.len(), QuicCidGenerator::DEFAULT_CID_LEN);
    assert_eq!(&first[..2], &[0, 7]);
    assert_eq!(&second[..2], &[0, 7]);
    assert_ne!(&first[2..], &second[2..]);
}

#[test]
fn quic_cid_generator_writes_full_16_bit_worker_prefix_and_random_suffix() {
    let generator = QuicCidGenerator::new(0x0103).unwrap();

    let first = generator.generate().unwrap();
    let second = generator.generate().unwrap();

    assert_eq!(first.len(), QuicCidGenerator::DEFAULT_CID_LEN);
    assert_eq!(&first[..2], &[0x01, 0x03]);
    assert_eq!(&second[..2], &[0x01, 0x03]);
    assert_ne!(&first[2..], &second[2..]);
}

#[test]
fn quic_cid_generator_supports_configured_length_and_generate_into() {
    let generator = QuicCidGenerator::for_worker(7)
        .unwrap()
        .with_cid_len(12)
        .unwrap();
    let mut cid = [0_u8; 12];

    generator.generate_into(&mut cid).unwrap();

    assert_eq!(generator.worker_index(), 7);
    assert_eq!(generator.cid_len(), 12);
    assert_eq!(&cid[..2], &[0, 7]);
}

#[test]
fn quic_cid_generator_rejects_invalid_lengths_and_worker_index() {
    assert!(QuicCidGenerator::new(1).unwrap().with_cid_len(7).is_err());
    assert!(QuicCidGenerator::new(1).unwrap().with_cid_len(21).is_err());
    assert!(QuicCidGenerator::for_worker(0x1_0000).is_err());

    let generator = QuicCidGenerator::new(1).unwrap();
    let mut short = [0_u8; 7];
    assert!(generator.generate_into(&mut short).is_err());
}
