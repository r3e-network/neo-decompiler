//! Shared pattern-matching helpers for recognising `if`/`else`/`else if`
//! statement shapes in lifted pseudo-code.

/// Check if a line opens an `if` block: `if <condition> {`
pub(in super::super) fn is_if_open(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("if ") && trimmed.ends_with(" {")
}

/// Check if a line opens an `else` block: `else {` or `} else {`
pub(in super::super) fn is_else_open(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed == "else {" || trimmed == "} else {"
}

/// Check if a line opens an `else if` block: `[} ]else if <condition> {`
pub(in super::super) fn is_else_if_open(line: &str) -> bool {
    let trimmed = line.trim();
    let trimmed = trimmed.strip_prefix("} ").unwrap_or(trimmed);
    trimmed.starts_with("else if ") && trimmed.ends_with(" {")
}

/// Extract the condition from a plain `if <condition> {` line.
pub(in super::super) fn extract_if_condition(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    let without_prefix = trimmed.strip_prefix("if ")?;
    without_prefix.strip_suffix(" {")
}

/// Extract the condition from an `else if <condition> {` or
/// `} else if <condition> {` line.
pub(in super::super) fn extract_else_if_condition(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    let trimmed = trimmed.strip_prefix("} ").unwrap_or(trimmed);
    trimmed.strip_prefix("else if ")?.strip_suffix(" {")
}

/// Extract the condition from either an `if` or `else if` line, optionally
/// preceded by `} `.  Used by the switch rewriter which must handle both forms.
pub(in super::super) fn extract_any_if_condition(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    let trimmed = trimmed.strip_prefix("} ").unwrap_or(trimmed);
    let without_prefix = trimmed
        .strip_prefix("if ")
        .or_else(|| trimmed.strip_prefix("else if "))?;
    without_prefix.strip_suffix(" {")
}
