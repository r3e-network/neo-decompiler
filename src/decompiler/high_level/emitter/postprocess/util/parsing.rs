use super::super::super::HighLevelEmitter;
use super::Assignment;

impl HighLevelEmitter {
    pub(in super::super) fn extract_while_condition(line: &str) -> Option<String> {
        let trimmed = line.trim();
        if !trimmed.starts_with("while ") {
            return None;
        }
        let rest = trimmed.strip_prefix("while ")?;
        let condition = rest.strip_suffix(" {")?.trim();
        if condition.is_empty() {
            None
        } else {
            Some(condition.to_string())
        }
    }

    pub(in super::super) fn parse_assignment(line: &str) -> Option<Assignment> {
        let trimmed = line.trim();
        if trimmed.is_empty() || !trimmed.ends_with(';') {
            return None;
        }
        let body = trimmed.trim_end_matches(';').trim();
        let mut parts = body.splitn(2, '=');
        let lhs_raw = parts.next()?.trim();
        let rhs = parts.next()?.trim().to_string();
        if lhs_raw.is_empty() || rhs.is_empty() {
            return None;
        }
        // Reject if we split inside a compound operator (==, !=, <=, >=).
        if rhs.starts_with('=')
            || lhs_raw.ends_with('!')
            || lhs_raw.ends_with('<')
            || lhs_raw.ends_with('>')
        {
            return None;
        }
        let lhs = if let Some(stripped) = lhs_raw.strip_prefix("let ") {
            stripped.trim().to_string()
        } else {
            lhs_raw.to_string()
        };
        // LHS must be a valid identifier (e.g. `t12`, `loc0`), not an
        // arbitrary expression like `assert((t10` that results from
        // splitting on the `=` inside `==`.
        if !is_valid_lhs(&lhs) {
            return None;
        }
        Some(Assignment {
            full: body.to_string(),
            lhs,
            rhs,
        })
    }

    pub(in super::super) fn parse_for_parts(line: &str) -> Option<(String, String, String)> {
        let trimmed = line.trim();
        if !trimmed.starts_with("for (") || !trimmed.ends_with('{') {
            return None;
        }
        let body = trimmed
            .trim_start_matches("for (")
            .trim_end_matches('{')
            .trim();
        let body = body.strip_suffix(')')?.trim();
        let parts = body.splitn(3, ';').map(str::trim).collect::<Vec<_>>();
        if parts.len() != 3 {
            return None;
        }
        Some((
            parts[0].to_string(),
            parts[1].to_string(),
            parts[2].to_string(),
        ))
    }
}

/// A valid LHS is a simple identifier: starts with a letter or underscore,
/// followed by alphanumeric characters or underscores.
fn is_valid_lhs(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if first != '_' && !first.is_ascii_alphabetic() {
        return false;
    }
    chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}
