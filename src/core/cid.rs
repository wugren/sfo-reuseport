use crate::core::Error;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuicCidGenerator {
    worker_index: usize,
    cid_len: usize,
}

impl QuicCidGenerator {
    pub const DEFAULT_CID_LEN: usize = 8;
    pub const MIN_CID_LEN: usize = 8;
    pub const MAX_CID_LEN: usize = 20;
    pub const MAX_WORKER_INDEX: usize = 0xffff;

    pub fn new(worker_index: usize) -> Result<Self, Error> {
        validate_worker_index(worker_index)?;
        Ok(Self {
            worker_index,
            cid_len: Self::DEFAULT_CID_LEN,
        })
    }

    pub fn for_worker(worker_index: usize) -> Result<Self, Error> {
        Self::new(worker_index)
    }

    pub fn with_cid_len(mut self, cid_len: usize) -> Result<Self, Error> {
        validate_cid_len(cid_len)?;
        self.cid_len = cid_len;
        Ok(self)
    }

    pub fn worker_index(&self) -> usize {
        self.worker_index
    }

    pub fn cid_len(&self) -> usize {
        self.cid_len
    }

    pub fn generate(&self) -> Result<Vec<u8>, Error> {
        let mut cid = vec![0_u8; self.cid_len];
        self.generate_into(&mut cid)?;
        Ok(cid)
    }

    pub fn generate_into(&self, buffer: &mut [u8]) -> Result<(), Error> {
        if buffer.len() != self.cid_len {
            return Err(Error::InvalidConfig(format!(
                "QUIC CID buffer length must equal configured CID length {}",
                self.cid_len
            )));
        }
        validate_cid_len(buffer.len())?;
        getrandom::fill(buffer)
            .map_err(|error| Error::Runtime(format!("QUIC CID random generation failed: {error}")))?;
        encode_worker_index_prefix(self.worker_index, buffer)?;
        Ok(())
    }
}

fn validate_worker_index(worker_index: usize) -> Result<(), Error> {
    if worker_index <= QuicCidGenerator::MAX_WORKER_INDEX {
        Ok(())
    } else {
        Err(Error::InvalidConfig(format!(
            "QUIC CID worker index must be between 0 and {}",
            QuicCidGenerator::MAX_WORKER_INDEX
        )))
    }
}

fn encode_worker_index_prefix(worker_index: usize, buffer: &mut [u8]) -> Result<(), Error> {
    validate_worker_index(worker_index)?;
    if buffer.len() < 2 {
        return Err(Error::InvalidConfig(
            "QUIC CID buffer must contain a two-byte worker index prefix".to_string(),
        ));
    }
    buffer[0] = (worker_index >> 8) as u8;
    buffer[1] = worker_index as u8;
    Ok(())
}

fn validate_cid_len(cid_len: usize) -> Result<(), Error> {
    if (QuicCidGenerator::MIN_CID_LEN..=QuicCidGenerator::MAX_CID_LEN).contains(&cid_len) {
        Ok(())
    } else {
        Err(Error::InvalidConfig(format!(
            "QUIC CID length must be between {} and {} bytes",
            QuicCidGenerator::MIN_CID_LEN,
            QuicCidGenerator::MAX_CID_LEN
        )))
    }
}
