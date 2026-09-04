use std::collections::HashSet;

const METHOD_TOKEN_LABEL_PREFIX: &str = "__callt_token_";

/// Sanitize an arbitrary manifest or user-provided identifier into a stable
/// snake-ish form suitable for high-level output.
pub(in super::super) fn sanitize_identifier(input: &str) -> String {
    let mut ident = String::new();
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            ident.push(ch);
        } else if ch == '_' || (ch.is_whitespace() || ch == '-') && !ident.ends_with('_') {
            ident.push('_');
        }
    }
    while ident.ends_with('_') {
        ident.pop();
    }
    if ident.is_empty() {
        ident.push_str("param");
    }
    if ident
        .chars()
        .next()
        .map(|ch| ch.is_ascii_digit())
        .unwrap_or(false)
    {
        ident.insert(0, '_');
    }
    // This prefix is reserved for index-addressed CALLT labels. Keeping
    // manifest/user identifiers out of the namespace prevents an internal
    // method call from being mistaken for a token call by downstream renderers.
    if ident.starts_with(METHOD_TOKEN_LABEL_PREFIX) {
        ident.insert_str(0, "method_");
    }
    ident
}

pub(in super::super) fn make_unique_identifier(base: String, used: &mut HashSet<String>) -> String {
    if used.insert(base.clone()) {
        return base;
    }
    // Preserve a leading `@` prefix (C# verbatim identifiers) so that
    // the suffix is appended to the stem, not after the prefix.
    let (prefix, stem) = match base.strip_prefix('@') {
        Some(stem) => ("@", stem),
        None => ("", base.as_str()),
    };
    let mut index = 1usize;
    loop {
        let candidate = format!("{prefix}{stem}_{index}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
        index += 1;
    }
}
