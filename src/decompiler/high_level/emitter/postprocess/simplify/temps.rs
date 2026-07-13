//! Temporary-value cleanup passes.

use super::super::super::HighLevelEmitter;

impl HighLevelEmitter {
    /// Removes `let tN = <pure_value>;` lines whose lhs is never referenced.
    /// Pure values are literals, simple identifiers, or known-pure expressions.
    pub(crate) fn eliminate_dead_temps(statements: &mut [String]) {
        let mut index = 0;
        while index < statements.len() {
            let trimmed = statements[index].trim();
            if !trimmed.starts_with("let ") {
                index += 1;
                continue;
            }
            let Some(assign) = Self::parse_assignment(trimmed) else {
                index += 1;
                continue;
            };
            if !Self::is_temp_ident(&assign.lhs) || !Self::is_pure_rhs(&assign.rhs) {
                index += 1;
                continue;
            }
            let lhs = &assign.lhs;
            let used_anywhere = statements
                .iter()
                .enumerate()
                .any(|(i, stmt)| i != index && Self::contains_identifier(stmt, lhs));
            if !used_anywhere {
                statements[index].clear();
            }
            index += 1;
        }
    }
}
