use super::super::HighLevelEmitter;

impl HighLevelEmitter {
    /// Removes `label_0xXXXX:` lines that have no matching `goto`, `leave`,
    /// or inline `if ... { goto label_X; }` reference. These artifacts arise
    /// when control-flow lifting falls through to straight-line emission and
    /// emits a label whose only intended target was inlined away.
    pub(crate) fn remove_orphaned_labels(statements: &mut [String]) {
        let mut referenced: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        for stmt in statements.iter() {
            let trimmed = stmt.trim();

            if let Some(name) = trimmed
                .strip_prefix("goto ")
                .and_then(|s| s.strip_suffix(';'))
                .map(str::trim)
            {
                if name.starts_with("label_0x") {
                    referenced.insert(name.to_string());
                }
            }

            if let Some(name) = trimmed
                .strip_prefix("leave ")
                .and_then(|s| s.strip_suffix(';'))
                .map(str::trim)
            {
                if name.starts_with("label_0x") {
                    referenced.insert(name.to_string());
                }
            }

            if trimmed.starts_with("if ") && trimmed.ends_with('}') {
                if let Some(brace_idx) = trimmed.find('{') {
                    let body = trimmed[brace_idx + 1..trimmed.len() - 1].trim();
                    if let Some(name) = body
                        .strip_prefix("goto ")
                        .and_then(|s| s.strip_suffix(';'))
                        .map(str::trim)
                    {
                        if name.starts_with("label_0x") {
                            referenced.insert(name.to_string());
                        }
                    }
                }
            }
        }

        for stmt in statements.iter_mut() {
            let trimmed = stmt.trim();
            if let Some(name) = trimmed.strip_suffix(':') {
                if name.starts_with("label_0x") && !referenced.contains(name) {
                    stmt.clear();
                }
            }
        }
    }
}
