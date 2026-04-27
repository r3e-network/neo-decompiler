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
    pub(crate) fn eliminate_identity_temps(statements: &mut [String]) {
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
            let lhs = assign.lhs.clone();
            let rhs = assign.rhs.clone();
            let lhs_seen_earlier = statements
                .iter()
                .take(index)
                .any(|stmt| Self::contains_identifier(stmt, &lhs));
            if lhs_seen_earlier {
                index += 1;
                continue;
            }
            // Substitute lhs → rhs in all subsequent lines
            for stmt in statements.iter_mut().skip(index + 1) {
                if Self::contains_identifier(stmt, &lhs) {
                    *stmt = Self::replace_identifier(stmt, &lhs, &rhs);
                }
            }
            statements[index].clear();
            index += 1;
        }
    }

    /// Collapses `let tN = <expr>; X = tN;` into `X = <expr>;` when tN is
    /// not used anywhere else.  This pattern arises from stack-based codegen
    /// where every VM instruction produces a temp that is immediately stored.
    pub(crate) fn collapse_temp_into_store(statements: &mut [String]) {
        let mut index = 0;
        while index + 1 < statements.len() {
            let trimmed = statements[index].trim();
            let Some(a1) = Self::parse_assignment(trimmed) else {
                index += 1;
                continue;
            };
            if !trimmed.starts_with("let ") || !Self::is_temp_ident(&a1.lhs) {
                index += 1;
                continue;
            }
            // Find next non-empty/non-comment line
            let mut next = index + 1;
            while next < statements.len() {
                let t = statements[next].trim();
                if !t.is_empty() && !t.starts_with("//") {
                    break;
                }
                next += 1;
            }
            if next >= statements.len() {
                index += 1;
                continue;
            }
            let trimmed_next = statements[next].trim();
            let temp = &a1.lhs;
            // Try assignment pattern: `[let] X = tN;`
            if let Some(a2) = Self::parse_assignment(trimmed_next) {
                if a2.rhs == *temp {
                    let used_later = statements
                        .iter()
                        .skip(next + 1)
                        .any(|s| Self::contains_identifier(s, temp));
                    if !used_later {
                        let indent =
                            &statements[next][..statements[next].len() - trimmed_next.len()];
                        let prefix = if trimmed_next.starts_with("let ") {
                            "let "
                        } else {
                            ""
                        };
                        statements[next] = format!("{indent}{prefix}{} = {};", a2.lhs, a1.rhs);
                        statements[index].clear();
                        index = next + 1;
                        continue;
                    }
                }
            }
            // Try `return tN;` pattern
            if trimmed_next == format!("return {};", temp) {
                let used_later = statements
                    .iter()
                    .skip(next + 1)
                    .any(|s| Self::contains_identifier(s, temp));
                if !used_later {
                    let indent = &statements[next][..statements[next].len() - trimmed_next.len()];
                    statements[next] = format!("{indent}return {};", a1.rhs);
                    statements[index].clear();
                    index = next + 1;
                    continue;
                }
            }
            index += 1;
        }
    }

    /// Removes `let tN = <pure_value>;` lines whose lhs is never referenced.
    /// Pure values are literals (numbers, booleans, null), simple identifiers,
    /// or string/byte literals — anything without a side-effecting call.
    /// This catches the common `let tN = 0; return;` leftover where the lifted
    /// stack push has no consumer in the lifted form.
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
            if !Self::is_temp_ident(&assign.lhs) {
                index += 1;
                continue;
            }
            if !Self::is_pure_rhs(&assign.rhs) {
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

    /// Returns true when `rhs` is safe to drop without altering side effects
    /// or hiding a runtime exception the original bytecode would have raised.
    ///
    /// Accepts:
    /// - literals (numbers, hex, strings, byte literals, true/false/null)
    /// - bare identifiers (loc0, arg1, static0, tN, …)
    /// - arithmetic over the above using the operators listed below — NEO
    ///   `ADD`/`SUB`/`MUL`/`AND`/`OR`/`XOR`/`SHL`/`SHR` and the comparison
    ///   ops are observably pure on the lifted view (read but never mutate)
    ///   and do not throw on valid operand types.
    ///
    /// Rejects:
    /// - expressions containing `/` or `%` — `DIV`/`MOD` throw on divide by
    ///   zero, so eliminating an unused temp would hide a real exception.
    /// - expressions containing `[` — `PICKITEM` throws on out-of-bounds
    ///   indexing or missing map keys; same hazard as above.
    /// - expressions containing `(` (calls), unless the call is a
    ///   known-pure helper. Side-effecting NEO calls (syscalls,
    ///   internal CALL, CALLA, CALLT, manifest method names) must
    ///   stay non-inlinable; without inlining them, two consumers
    ///   would each re-execute the side effect. The whitelist
    ///   covers the pure NEO arithmetic / buffer / type-check
    ///   helpers the lift emits — these can be inlined safely
    ///   because re-evaluation has no observable effect.
    fn is_pure_rhs(rhs: &str) -> bool {
        let trimmed = rhs.trim();
        if trimmed.is_empty() {
            return false;
        }
        // Reject divide-by-zero and indexing throws unconditionally.
        if trimmed
            .as_bytes()
            .iter()
            .any(|b| matches!(*b, b'/' | b'%' | b'['))
        {
            return false;
        }
        // No call → safe (literals, identifiers, arithmetic, etc.).
        if !trimmed.contains('(') {
            return true;
        }
        // Has a call — accept only when every call site in the
        // expression starts with a known-pure helper identifier.
        rhs_calls_only_pure_helpers(trimmed)
    }

    /// Collapse `((expr))` to `(expr)` whenever the inner parens form a
    /// matched pair surrounding the entire content. The single-use-temp
    /// inliner unconditionally wraps multi-token substitutions in parens
    /// for precedence safety; when the substitution lands inside an
    /// existing parenthesised context (e.g. a `assert((x > 0))` call
    /// argument), the result is doubly-parenthesised. Stripping the
    /// inner pair leaves the operator-precedence intact.
    pub(crate) fn reduce_double_parens(statements: &mut [String]) {
        for stmt in statements.iter_mut() {
            // Loop until no change so chains like `(((x)))` collapse fully.
            loop {
                let mut next: Option<String> = None;
                let bytes = stmt.as_bytes();
                let mut i = 0;
                while i + 1 < bytes.len() {
                    if bytes[i] == b'(' && bytes[i + 1] == b'(' {
                        // Walk the inner `(` to find its matching `)`.
                        let inner_open = i + 1;
                        let mut depth = 1usize;
                        let mut j = inner_open + 1;
                        while j < bytes.len() {
                            match bytes[j] {
                                b'(' => depth += 1,
                                b')' => {
                                    depth -= 1;
                                    if depth == 0 {
                                        break;
                                    }
                                }
                                _ => {}
                            }
                            j += 1;
                        }
                        if depth != 0 {
                            break;
                        }
                        // The character immediately after the inner `)`
                        // must itself be `)` for the outer pair to be
                        // redundant — i.e. the pattern is `((...))`
                        // where both parens close back-to-back.
                        if j + 1 < bytes.len() && bytes[j + 1] == b')' {
                            let mut rebuilt = String::with_capacity(stmt.len() - 2);
                            rebuilt.push_str(&stmt[..i]);
                            rebuilt.push('(');
                            rebuilt.push_str(&stmt[inner_open + 1..j]);
                            rebuilt.push(')');
                            rebuilt.push_str(&stmt[j + 2..]);
                            next = Some(rebuilt);
                            break;
                        }
                    }
                    i += 1;
                }
                match next {
                    Some(rebuilt) => *stmt = rebuilt,
                    None => break,
                }
            }
        }
    }

    /// Strips VM-level stack operation comments that add noise to the output:
    /// - Removes standalone `// drop ...`, `// remove second stack value`,
    ///   `// swapped top two stack values`, `// xdrop stack[...]`,
    ///   `// rotate top three stack values`, `// tuck top of stack`,
    ///   `// reverse top N stack values`, and `// clear stack` lines.
    ///   These describe the VM-level rearrangement; the actual data flow
    ///   is already captured in subsequent variable references, so the
    ///   comment is redundant once the lift completes.
    /// - Strips trailing `// duplicate top of stack` and `// copy second stack value`.
    pub(crate) fn strip_stack_comments(statements: &mut [String]) {
        for stmt in statements.iter_mut() {
            let trimmed = stmt.trim();
            if trimmed.starts_with("// drop ")
                || trimmed.starts_with("// remove second")
                || trimmed.starts_with("// swapped top")
                || trimmed.starts_with("// xdrop stack")
                || trimmed.starts_with("// rotate top")
                || trimmed.starts_with("// tuck top")
                || trimmed.starts_with("// reverse top")
                || trimmed == "// clear stack"
            {
                stmt.clear();
                continue;
            }
            for suffix in [" // duplicate top of stack", " // copy second stack value"] {
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

/// Return `true` when every `name(` call in `expr` starts with a
/// known-pure helper identifier — i.e. one of the NEO math /
/// buffer / type-check helpers the lift emits as `name(args)`.
/// Calls into syscalls, internal/indirect/token-call helpers, or
/// manifest method names are NOT pure and must keep the temp.
fn rhs_calls_only_pure_helpers(expr: &str) -> bool {
    let bytes = expr.as_bytes();
    let mut in_string: Option<u8> = None;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if let Some(quote) = in_string {
            if b == b'\\' && i + 1 < bytes.len() {
                i += 2;
                continue;
            }
            if b == quote {
                in_string = None;
            }
            i += 1;
            continue;
        }
        if b == b'"' || b == b'\'' {
            in_string = Some(b);
            i += 1;
            continue;
        }
        if b == b'(' {
            // Walk back to find the identifier preceding `(`.
            let mut start = i;
            while start > 0 {
                let prev = bytes[start - 1];
                if prev.is_ascii_alphanumeric() || prev == b'_' {
                    start -= 1;
                } else {
                    break;
                }
            }
            if start == i {
                // `(` with no preceding identifier — likely a
                // grouping paren (e.g. `(a + b)` inside the RHS).
                // That's pure; keep scanning.
                i += 1;
                continue;
            }
            let ident = &expr[start..i];
            if !is_pure_helper_identifier(ident) {
                return false;
            }
        }
        i += 1;
    }
    true
}

/// The whitelist of identifiers the high-level lift uses for
/// known-pure NEO operations. Anything outside this set is treated
/// as potentially side-effecting (so the inliner won't move calls
/// across each other or duplicate them by inlining a single-use
/// temp into a multi-use position).
fn is_pure_helper_identifier(ident: &str) -> bool {
    if matches!(
        ident,
        // Math / arithmetic helpers (`Math` / `Helper.X` in C#)
        "abs"
            | "sign"
            | "sqrt"
            | "min"
            | "max"
            | "pow"
            | "modpow"
            | "modmul"
            | "within"
            // Buffer / string helpers
            | "left"
            | "right"
            | "substr"
            // Type checks (`(x is null)` form lives outside the
            // call shape and is handled elsewhere; the call form
            // appears only when the lift falls through to the
            // generic helper)
            | "is_null"
            // Collection accessors that don't mutate state
            | "keys"
            | "values"
            | "has_key"
            | "len"
    ) {
        return true;
    }
    // Type-prefixed helpers: `is_type_bool`, `convert_to_integer`,
    // etc. (suffix is one of NEO's stack-item type names).
    ident.starts_with("is_type_") || ident.starts_with("convert_to_")
}
