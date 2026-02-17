use super::super::HighLevelEmitter;

impl HighLevelEmitter {
    /// Collapses `if true { ... }` blocks into their body.
    /// The Neo C# compiler emits `PUSHT; JMPIFNOT` for unconditional
    /// default branches (e.g. switch defaults, enum parse fallbacks).
    pub(crate) fn collapse_if_true(statements: &mut Vec<String>) {
        let mut index = 0;
        while index < statements.len() {
            if statements[index].trim() != "if true {" {
                index += 1;
                continue;
            }
            let Some(end) = Self::find_block_end(statements, index) else {
                index += 1;
                continue;
            };
            if statements[end].trim() != "}" {
                index += 1;
                continue;
            }
            // Remove the `if true {` and closing `}`
            statements.remove(end);
            statements.remove(index);
            // Don't advance — re-check at same index
        }
    }
}
