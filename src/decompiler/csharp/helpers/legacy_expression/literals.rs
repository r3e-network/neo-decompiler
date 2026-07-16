//! Literal normalization for legacy lifted C# expressions.

/// Wrap a decimal integer literal that exceeds C#'s `ulong` range in
/// `BigInteger.Parse("…")`.
pub(super) fn match_big_integer_literal(s: &str) -> Option<(String, usize)> {
    let bytes = s.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_digit() {
        return None;
    }
    let mut j = 0;
    while j < bytes.len() && bytes[j].is_ascii_digit() {
        j += 1;
    }
    if j < bytes.len() {
        let after = bytes[j];
        // `0x…`, an identifier continuation, or a fractional/member `.`
        // means this digit run is not a standalone decimal literal.
        if after == b'x'
            || after == b'X'
            || after.is_ascii_alphabetic()
            || after == b'_'
            || after == b'.'
        {
            return None;
        }
    }
    let digits = &s[..j];
    if !decimal_exceeds_u64(digits) {
        return None;
    }
    Some((format!("BigInteger.Parse(\"{digits}\")"), j))
}

/// Rewrite an oversized `0x…` blob into a C# byte-array literal.
pub(super) fn match_big_byte_literal(s: &str) -> Option<(String, usize)> {
    let bytes = s.as_bytes();
    if bytes.len() < 2 || bytes[0] != b'0' || !(bytes[1] == b'x' || bytes[1] == b'X') {
        return None;
    }
    let mut j = 2;
    while j < bytes.len() && bytes[j].is_ascii_hexdigit() {
        j += 1;
    }
    let hex = &s[2..j];
    // > 16 nibbles (beyond ulong) and whole bytes only.
    if hex.len() <= 16 || hex.len() % 2 != 0 {
        return None;
    }
    // Must be a complete token, not the prefix of a longer identifier.
    if j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
        return None;
    }
    let mut rendered = String::from("new byte[] { ");
    for (idx, pair) in hex.as_bytes().chunks_exact(2).enumerate() {
        if idx > 0 {
            rendered.push_str(", ");
        }
        rendered.push_str("0x");
        rendered.push(pair[0] as char);
        rendered.push(pair[1] as char);
    }
    rendered.push_str(" }");
    Some((rendered, j))
}

/// Whether a decimal digit run exceeds `u64::MAX` without overflowing a
/// fixed-width integer while comparing it.
pub(super) fn decimal_exceeds_u64(digits: &str) -> bool {
    const U64_MAX: &str = "18446744073709551615";
    let trimmed = digits.trim_start_matches('0');
    let significant = if trimmed.is_empty() { "0" } else { trimmed };
    match significant.len().cmp(&U64_MAX.len()) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => significant > U64_MAX,
    }
}
