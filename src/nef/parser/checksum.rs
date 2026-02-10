use sha2::{Digest, Sha256};

use super::NefParser;

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
        u32::from_le_bytes(second[..4].try_into().unwrap())
    }
}
