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
mod tests;
