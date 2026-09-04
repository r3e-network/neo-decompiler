use std::fmt;

use ripemd::Ripemd160;
use sha2::{Digest, Sha256};

/// Render untrusted text on a single physical output line.
///
/// Printable text is preserved, while control characters, Unicode line
/// separators, and bidirectional formatting controls are made visible. This
/// is intended for comments and human-readable output, not string literals.
pub(crate) fn escape_visible_text(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\0' => escaped.push_str("\\0"),
            '\u{0007}' => escaped.push_str("\\a"),
            '\u{0008}' => escaped.push_str("\\b"),
            '\t' => escaped.push_str("\\t"),
            '\n' => escaped.push_str("\\n"),
            '\u{000B}' => escaped.push_str("\\v"),
            '\u{000C}' => escaped.push_str("\\f"),
            '\r' => escaped.push_str("\\r"),
            '\u{2028}' | '\u{2029}' => push_visible_unicode_escape(&mut escaped, character),
            control if control.is_control() || is_bidi_control(control) => {
                push_visible_unicode_escape(&mut escaped, control);
            }
            other => escaped.push(other),
        }
    }
    escaped
}

fn push_visible_unicode_escape(output: &mut String, character: char) {
    use std::fmt::Write;
    write!(output, "\\u{{{:04X}}}", u32::from(character)).unwrap();
}

/// Return whether `character` changes bidirectional presentation without
/// being visible itself.
pub(crate) fn is_bidi_control(character: char) -> bool {
    matches!(
        character,
        '\u{061C}'
            | '\u{200E}'
            | '\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2066}'..='\u{2069}'
    )
}

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

/// Compute the Neo Hash160 (`RIPEMD160(SHA256(data))`) and return the
/// little-endian bytes.
///
/// The raw `RIPEMD160(SHA256(..))` digest IS Neo's internal little-endian
/// `UInt160` byte order — the explorer/display ("big-endian", `0x`-prefixed)
/// form is this digest reversed. So the digest is returned as-is: callers use
/// [`format_hash`] for the little-endian rendering and [`format_hash_be`] for
/// the display rendering. (Reversing here would invert both, which is exactly
/// the bug this comment exists to prevent.)
pub(crate) fn hash160(data: &[u8]) -> [u8; 20] {
    let sha = Sha256::digest(data);
    let ripemd = Ripemd160::digest(sha);
    let mut output = [0u8; 20];
    output.copy_from_slice(&ripemd);
    output
}

#[cfg(test)]
mod tests;
