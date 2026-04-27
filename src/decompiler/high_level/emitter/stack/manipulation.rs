//! Stack manipulation opcode handlers.
//!
//! These helpers mutate the emitter value stack to track VM stack operations
//! such as `DUP`, `DROP`, `PICK`, and reversal opcodes.

mod basic;
mod indexed;
mod reorder;
mod reverse;

/// Match the JS port's `SIMPLE_IDENT_OR_LITERAL_RE`: a value is safe to
/// duplicate without temp materialization if it is a bare integer
/// literal (decimal or hex), the keyword `true`/`false`/`null`, a
/// quoted string literal, or a plain identifier.  Re-evaluating any of
/// those yields the same value with no observable side effects, so
/// stack-duplicating opcodes (DUP, OVER, TUCK) can collapse to two
/// copies of the expression string instead of `let tN = expr;` plus
/// two `tN` references.
pub(super) fn is_simple_literal_or_identifier(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }
    let no_sign = trimmed.strip_prefix('-').unwrap_or(trimmed);
    if !no_sign.is_empty() && no_sign.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        if !hex.is_empty() && hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return true;
        }
    }
    if matches!(trimmed, "true" | "false" | "null") {
        return true;
    }
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        let inner = &trimmed[1..trimmed.len() - 1];
        if !inner.contains('"') || string_has_only_escaped_quotes(inner) {
            return true;
        }
    }
    let mut chars = trimmed.chars();
    if let Some(first) = chars.next() {
        if (first.is_ascii_alphabetic() || first == '_')
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return true;
        }
    }
    false
}

fn string_has_only_escaped_quotes(inner: &str) -> bool {
    let bytes = inner.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            let mut backslashes = 0;
            let mut j = i;
            while j > 0 && bytes[j - 1] == b'\\' {
                backslashes += 1;
                j -= 1;
            }
            if backslashes % 2 == 0 {
                return false;
            }
        }
        i += 1;
    }
    true
}
