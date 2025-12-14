use super::super::super::HighLevelEmitter;

impl HighLevelEmitter {
    /// Check if an identifier appears in a string (as a whole token).
    ///
    /// This is used by post-processing passes that operate on stringified pseudo-code.
    pub(in super::super) fn contains_identifier(text: &str, ident: &str) -> bool {
        if ident.is_empty() {
            return false;
        }

        let bytes = text.as_bytes();
        let ident_len = ident.len();

        for (pos, _) in text.match_indices(ident) {
            if is_identifier_boundary(bytes, pos, ident_len) {
                return true;
            }
        }

        false
    }

    /// Replace an identifier in `text` with `replacement` when it appears as a whole token.
    pub(in super::super) fn replace_identifier(
        text: &str,
        ident: &str,
        replacement: &str,
    ) -> String {
        if ident.is_empty() {
            return text.to_string();
        }

        let bytes = text.as_bytes();
        let ident_len = ident.len();
        let mut result = String::with_capacity(text.len());
        let mut cursor = 0;

        for (pos, _) in text.match_indices(ident) {
            if !is_identifier_boundary(bytes, pos, ident_len) {
                continue;
            }
            result.push_str(&text[cursor..pos]);
            result.push_str(replacement);
            cursor = pos + ident_len;
        }

        result.push_str(&text[cursor..]);
        result
    }
}

fn is_identifier_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn is_identifier_boundary(text: &[u8], start: usize, len: usize) -> bool {
    let before_ok = start == 0 || !is_identifier_char(text[start - 1]);
    let end = start + len;
    let after_ok = end == text.len() || !is_identifier_char(text[end]);
    before_ok && after_ok
}
