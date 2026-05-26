use std::net::{IpAddr, SocketAddr};

use crate::core::{Error, PacketMeta};

const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

pub(crate) fn linux_reuseport_select(meta: PacketMeta, workers: usize) -> Result<usize, Error> {
    if workers == 0 {
        return Err(Error::InvalidConfig(
            "worker count must be greater than zero".to_string(),
        ));
    }

    let mut hash = FNV_OFFSET;
    hash_socket_addr(&mut hash, meta.peer_addr);
    hash_socket_addr(&mut hash, meta.local_addr);
    Ok((hash as usize) % workers)
}

fn hash_socket_addr(hash: &mut u64, addr: Option<SocketAddr>) {
    let Some(addr) = addr else {
        write_u8(hash, 0);
        return;
    };

    match addr.ip() {
        IpAddr::V4(ip) => {
            write_u8(hash, 4);
            write_bytes(hash, &ip.octets());
        }
        IpAddr::V6(ip) => {
            write_u8(hash, 6);
            write_bytes(hash, &ip.octets());
        }
    }
    write_u16(hash, addr.port());
}

fn write_u8(hash: &mut u64, value: u8) {
    *hash ^= u64::from(value);
    *hash = hash.wrapping_mul(FNV_PRIME);
}

fn write_u16(hash: &mut u64, value: u16) {
    write_bytes(hash, &value.to_be_bytes());
}

fn write_bytes(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        write_u8(hash, *byte);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_reuseport_select_is_stable_for_fixed_metadata() {
        let meta = PacketMeta {
            peer_addr: Some("127.0.0.1:55000".parse().unwrap()),
            local_addr: Some("127.0.0.1:4433".parse().unwrap()),
        };

        assert_eq!(linux_reuseport_select(meta, 4).unwrap(), 1);
        assert_eq!(linux_reuseport_select(meta, 4).unwrap(), 1);
    }

    #[test]
    fn linux_reuseport_select_rejects_zero_workers() {
        let err = linux_reuseport_select(PacketMeta::default(), 0).unwrap_err();
        assert!(matches!(err, Error::InvalidConfig(_)));
    }
}
