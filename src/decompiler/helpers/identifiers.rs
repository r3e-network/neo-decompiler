use std::collections::HashSet;

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
    ident
}

pub(in super::super) fn make_unique_identifier(
    base: String,
    used: &mut HashSet<String>,
) -> String {
    if used.insert(base.clone()) {
        return base;
    }
    let mut index = 1usize;
    loop {
        let candidate = format!("{base}_{index}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
        index += 1;
    }
}
