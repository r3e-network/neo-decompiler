use std::fmt;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_upper_hex() {
        let bytes = [0xDE, 0xAD, 0xBE, 0xEF];
        assert_eq!(upper_hex_string(&bytes), "DEADBEEF");
    }
}
