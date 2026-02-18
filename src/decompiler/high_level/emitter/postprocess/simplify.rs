use super::super::HighLevelEmitter;

impl HighLevelEmitter {
    /// Collapses `if true { ... }` blocks into their body.
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
            statements.remove(end);
            statements.remove(index);
        }
    }

    /// Inverts `if cond { } else { ... }` → `if !(cond) { ... }`.
    /// The Neo compiler emits JMPNE/JMPEQ patterns that produce empty
    /// if-bodies with all logic in the else branch.
    pub(crate) fn invert_empty_if_else(statements: &mut Vec<String>) {
        let mut index = 0;
        while index < statements.len() {
            let trimmed = statements[index].trim();
            if !trimmed.starts_with("if ") || !trimmed.ends_with('{') {
                index += 1;
                continue;
            }
            // Check if body is empty (only comments between `if` and `}`)
            let mut j = index + 1;
            while j < statements.len() {
                let t = statements[j].trim();
                if !t.is_empty() && !t.starts_with("//") {
                    break;
                }
                j += 1;
            }
            if j >= statements.len() || statements[j].trim() != "}" {
                index += 1;
                continue;
            }
            let close_if = j;
            // Next line must be `else {`
            if close_if + 1 >= statements.len() || statements[close_if + 1].trim() != "else {" {
                index += 1;
                continue;
            }
            let else_line = close_if + 1;
            let Some(else_end) = Self::find_block_end(statements, else_line) else {
                index += 1;
                continue;
            };
            // Extract and negate condition
            let indent = &statements[index][..statements[index].len() - trimmed.len()];
            let cond = &trimmed[3..trimmed.len() - 2]; // strip "if " and " {"
            let negated = Self::negate_condition(cond);
            // Replace: remove empty if body + else wrapper, rewrite header
            statements[index] = format!("{indent}if {negated} {{");
            // Remove closing `}` of else block, then the `}` and `else {` lines.
            // Comments from the empty if-body are kept as bytecode annotations.
            statements.remove(else_end);
            statements.drain(close_if..=else_line);
            // Don't advance — re-check at same index
        }
    }

    /// Removes `if cond { }` blocks with no else branch (dead no-op conditionals).
    pub(crate) fn remove_empty_if(statements: &mut Vec<String>) {
        let mut index = 0;
        while index < statements.len() {
            let trimmed = statements[index].trim();
            if !trimmed.starts_with("if ") || !trimmed.ends_with('{') {
                index += 1;
                continue;
            }
            let mut j = index + 1;
            while j < statements.len() {
                let t = statements[j].trim();
                if !t.is_empty() && !t.starts_with("//") {
                    break;
                }
                j += 1;
            }
            if j >= statements.len() || statements[j].trim() != "}" {
                index += 1;
                continue;
            }
            // Must NOT be followed by else
            if j + 1 < statements.len() && statements[j + 1].trim().starts_with("else") {
                index += 1;
                continue;
            }
            statements.drain(index..=j);
        }
    }

    /// Eliminates identity assignments `let tN = tM;` by substituting tN→tM
    /// in all subsequent code. These arise from branch reconciliation (phi nodes)
    /// and DUP/OVER patterns where the copy is trivially aliased.
    pub(crate) fn eliminate_identity_temps(statements: &mut Vec<String>) {
        let mut index = 0;
        while index < statements.len() {
            let trimmed = statements[index].trim();
            let Some(assign) = Self::parse_assignment(trimmed) else {
                index += 1;
                continue;
            };
            // Only target `let tN = tM;` where both are temp identifiers
            if !trimmed.starts_with("let ") {
                index += 1;
                continue;
            }
            if !Self::is_temp_ident(&assign.lhs) || !Self::is_temp_ident(&assign.rhs) {
                index += 1;
                continue;
            }
            // Self-assignment (`let tN = tN;`) is dead code — just remove it
            if assign.lhs == assign.rhs {
                statements[index].clear();
                index += 1;
                continue;
            }
            // Substitute lhs → rhs in all subsequent lines
            let lhs = assign.lhs.clone();
            let rhs = assign.rhs.clone();
            for stmt in statements.iter_mut().skip(index + 1) {
                if Self::contains_identifier(stmt, &lhs) {
                    *stmt = Self::replace_identifier(stmt, &lhs, &rhs);
                }
            }
            statements[index].clear();
            index += 1;
        }
    }

    /// Strips VM-level stack operation comments that add noise to the output:
    /// - Removes standalone `// drop ...` and `// remove second stack value` lines
    /// - Strips trailing `// duplicate top of stack` and `// copy second stack value`
    pub(crate) fn strip_stack_comments(statements: &mut Vec<String>) {
        for stmt in statements.iter_mut() {
            let trimmed = stmt.trim();
            if trimmed.starts_with("// drop ") || trimmed.starts_with("// remove second") {
                stmt.clear();
                continue;
            }
            for suffix in [
                " // duplicate top of stack",
                " // copy second stack value",
            ] {
                if let Some(pos) = stmt.find(suffix) {
                    stmt.truncate(pos);
                }
            }
        }
    }

    fn is_temp_ident(s: &str) -> bool {
        s.strip_prefix('t')
            .is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()))
    }

    fn negate_condition(cond: &str) -> String {
        let cond = cond.trim();
        // Flip comparison operators
        for (op, neg) in [
            (" == ", " != "),
            (" != ", " == "),
            (" >= ", " < "),
            (" <= ", " > "),
            (" > ", " <= "),
            (" < ", " >= "),
        ] {
            if let Some(pos) = cond.find(op) {
                return format!("{}{}{}", &cond[..pos], neg, &cond[pos + op.len()..]);
            }
        }
        // Strip leading `!`
        if let Some(inner) = cond.strip_prefix('!') {
            return inner.to_string();
        }
        format!("!({cond})")
    }
}
