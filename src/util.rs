use std::fmt;

use ripemd::Ripemd160;
use sha2::{Digest, Sha256};

/// Write the provided bytes as uppercase hexadecimal into the supplied formatter.
pub(crate) fn write_upper_hex(f: &mut fmt::Formatter<'_>, bytes: &[u8]) -> fmt::Result {
    f.write_str(&hex::encode_upper(bytes))
}

/// Return the provided bytes encoded as an uppercase hexadecimal string.
pub(crate) fn upper_hex_string(bytes: &[u8]) -> String {
    hex::encode_upper(bytes)
}

/// Convenience helper used when rendering script hashes or method token hashes.
pub(crate) fn format_hash(bytes: &[u8]) -> String {
    upper_hex_string(bytes)
}

/// Return the provided bytes encoded as uppercase hexadecimal after flipping endianness.
pub(crate) fn format_hash_be(bytes: &[u8]) -> String {
    let mut reversed = bytes.to_vec();
    reversed.reverse();
    upper_hex_string(&reversed)
}

/// Compute the Neo Hash160 (RIPEMD160(SHA256(data))) and return the little-endian bytes.
pub(crate) fn hash160(data: &[u8]) -> [u8; 20] {
    let sha = Sha256::digest(data);
    let ripemd = Ripemd160::digest(sha);
    let mut output = [0u8; 20];
    output.copy_from_slice(&ripemd);
    output.reverse(); // align with Neo's little-endian UInt160 representation
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_upper_hex() {
        let bytes = [0xDE, 0xAD, 0xBE, 0xEF];
        assert_eq!(upper_hex_string(&bytes), "DEADBEEF");
    }

    #[test]
    fn formats_hashes_in_both_endianness() {
        let bytes = [0x01, 0x23, 0x45, 0x67];
        assert_eq!(format_hash(&bytes), "01234567");
        assert_eq!(format_hash_be(&bytes), "67452301");
    }

    #[test]
    fn computes_hash160_little_endian() {
        let script = [0x10, 0x11, 0x9E, 0x40];
        let hash = hash160(&script);
        assert_eq!(
            format_hash(&hash),
            "9DE87DC65A6A581E502CAE845C6F13645B10C5EA"
        );
        assert_eq!(
            format_hash_be(&hash),
            "EAC5105B64136F5C84AE2C501E586A5AC67DE89D"
        );
    }
}
