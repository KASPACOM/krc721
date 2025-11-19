use crate::imports::*;
use crate::snapshot::{DATA_OFFSET, HEADER_MAGIC};
use workflow_core::hex::*;

#[derive(BorshSerialize, BorshDeserialize)]
pub struct Header {
    pub magic: [u8; 8],
    pub version: u64,
    pub chunks: u64,
    pub offset: u64,
    pub hash: [u8; 32],
}

impl Default for Header {
    fn default() -> Self {
        Self {
            magic: *HEADER_MAGIC,
            version: 1,
            chunks: 0,
            offset: DATA_OFFSET,
            hash: [0; 32],
        }
    }
}

impl std::fmt::Debug for Header {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Header {{ magic: {}, version: {}, chunks: {}, offset: {}, hash: {} }}",
            self.magic.as_ref().to_hex(),
            self.version,
            self.chunks,
            self.offset,
            self.hash.as_ref().to_hex()
        )
    }
}

impl std::fmt::Display for Header {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Version: {} Hash: {}",
            self.version,
            self.hash.as_ref().to_hex()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_size() {
        let bytes = borsh::to_vec(&Header::default()).unwrap();
        assert!(
            bytes.len() as u64 <= DATA_OFFSET,
            "Header size {} exceeds 128 bytes",
            bytes.len()
        );
    }
}
