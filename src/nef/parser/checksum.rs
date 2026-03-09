use sha2::{Digest, Sha256};

use super::NefParser;

fn checksum_prefix(bytes: &[u8]) -> u32 {
    let mut array = [0u8; 4];
    array.copy_from_slice(&bytes[..4]);
    u32::from_le_bytes(array)
}

impl NefParser {
    /// Calculate the NEF checksum over the payload bytes.
    ///
    /// This implements the double-SHA256 checksum used by the NEF container and
    /// returns the first 4 bytes of the resulting digest as a little-endian
    /// `u32`.
    #[must_use]
    pub fn calculate_checksum(payload: &[u8]) -> u32 {
        let first = Sha256::digest(payload);
        let second = Sha256::digest(first.as_slice());
        checksum_prefix(second.as_slice())
    }
}
