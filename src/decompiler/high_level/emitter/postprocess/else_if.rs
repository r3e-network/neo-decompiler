//! Collapse `} else { if condition {` into `} else if condition {`.

use super::super::HighLevelEmitter;
use super::util::{extract_if_condition, is_else_open, is_if_open};

impl HighLevelEmitter {
    /// Rewrite else-if chains to use idiomatic `else if` syntax.
    ///
    /// Detects patterns like:
    /// ```text
    /// } else {
    /// if condition {
    /// ```
    /// And rewrites them to:
    /// ```text
    /// } else if condition {
    /// ```
    pub(crate) fn rewrite_else_if_chains(statements: &mut Vec<String>) {
        let mut index = 0;
        while index + 1 < statements.len() {
            // Look for "else {" followed by "if condition {"
            if is_else_open(&statements[index]) && is_if_open(&statements[index + 1]) {
                // Extract the condition from the if statement
                if let Some(condition) = extract_if_condition(&statements[index + 1]) {
                    // Replace "else {" with "else if condition {"
                    statements[index] = format!("else if {condition} {{");
                    // Remove the standalone "if condition {"
                    statements.remove(index + 1);
                    // Find and remove the extra closing brace
                    if let Some(close_idx) = Self::find_matching_close(statements, index) {
                        // We need to remove one closing brace since we merged two blocks
                        Self::remove_one_closer(statements, close_idx);
                    }
                    // Don't increment - check if this new else-if can chain further
                    continue;
                }
            }
            index += 1;
        }
    }

    fn find_matching_close(statements: &[String], start: usize) -> Option<usize> {
        let mut depth = 1;
        for (i, stmt) in statements.iter().enumerate().skip(start + 1) {
            let trimmed = stmt.trim();
            if trimmed.ends_with('{') {
                depth += 1;
            }
            if trimmed == "}" || trimmed.starts_with("} ") {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
        }
        None
    }

    fn remove_one_closer(statements: &mut Vec<String>, close_idx: usize) {
        // Look for consecutive closing braces and remove one
        // This handles the case where we have "} }" that should become "}"
        if close_idx + 1 < statements.len() {
            let current = statements[close_idx].trim();
            let next = statements[close_idx + 1].trim();
            if current == "}" && next == "}" {
                statements.remove(close_idx + 1);
                return;
            }
        }
        // If the current line is just "}", check if we need to remove it
        // based on the block structure
        if statements[close_idx].trim() == "}" && close_idx > 0 {
            // Check if previous non-empty line also ends a block
            for i in (0..close_idx).rev() {
                let prev = statements[i].trim();
                if !prev.is_empty() {
                    if prev == "}" {
                        statements.remove(close_idx);
                    }
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
