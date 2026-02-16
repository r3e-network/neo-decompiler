//! Collapse verbose Neo C# compiler overflow-check patterns into clean expressions.
//!
//! The Neo C# compiler emits ~20 instructions for checked/unchecked int32/int64
//! overflow handling.  The decompiler lifts these faithfully, producing deeply
//! nested if/else blocks with masking and sign-extension logic.  This pass
//! recognises the pattern and collapses it back to the original expression.
//!
//! **Unchecked** pattern (int32 example):
//! ```text
//! let t0 = a + b;
//! // 00XX: DUP
//! let t1 = t0;              // duplicate top of stack
//! // 00XX: PUSHINT32
//! let t2 = -2147483648;     // min bound
//! // 00XX: JMPGE
//! if t1 < t2 {              // range check
//! }
//! else {
//!     ...masking logic...
//! }
//! ```
//! Collapsed to: `let t0 = a + b;`
//!
//! **Checked** pattern:
//! ```text
//! let t0 = a + b;
//! let t1 = t0;              // duplicate top of stack
//! let t2 = -2147483648;     // min bound
//! if t1 < t2 {
//!     throw(t0);            // overflow → throw
//! }
//! ```
//! Collapsed to: `let t0 = checked(a + b);`

use super::super::HighLevelEmitter;

/// Known type-boundary constants that start an overflow check sequence.
const OVERFLOW_BOUNDS: &[&str] = &[
    "-2147483648",          // i32 min
    "0",                    // u32 min (unsigned range check)
    "-9223372036854775808", // i64 min
];

impl HighLevelEmitter {
    /// Collapse overflow-check wrappers emitted by the Neo C# compiler.
    ///
    /// Must run after `rewrite_else_if_chains` (which may restructure the
    /// blocks we need to match) and before `rewrite_compound_assignments`
    /// (which would obscure the DUP assignment pattern).
    pub(crate) fn collapse_overflow_checks(statements: &mut Vec<String>) {
        let mut index = 0;
        while index < statements.len() {
            if let Some(collapse) = try_match_overflow(statements, index) {
                apply_collapse(statements, &collapse);
                // Don't advance — the replacement may enable further matches
                // at the same index (e.g. nested overflow checks like negateAddInt).
                continue;
            }
            index += 1;
        }
    }
}

/// Describes a matched overflow-check pattern ready for collapse.
struct OverflowCollapse {
    /// Index of the `let tA = <expr>;` line (the actual operation).
    op_line: usize,
    /// The original expression on the RHS of the operation assignment.
    expr: String,
    /// The LHS variable name of the operation assignment.
    result_var: String,
    /// Index of the first line to blank (the line after the operation).
    blank_start: usize,
    /// Index of the last line to blank (the closing `}` of the overflow block).
    blank_end: usize,
    /// Whether this is a checked (throw on overflow) pattern.
    is_checked: bool,
}

/// Return the index of the next non-empty, non-comment line at or after `start`.
fn next_code_line(statements: &[String], start: usize) -> Option<usize> {
    for i in start..statements.len() {
        let trimmed = statements[i].trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        return Some(i);
    }
    None
}

/// Try to match the overflow-check pattern starting at `idx`.
///
/// The pattern in real decompiler output has comment lines interleaved between
/// code statements, so we skip comment/empty lines when scanning for the
/// 4-line header: operation → DUP → bound → if-check.
fn try_match_overflow(statements: &[String], idx: usize) -> Option<OverflowCollapse> {
    // Line 0: `let tA = <expr>;`
    let line0 = statements[idx].trim();
    if line0.is_empty() || line0.starts_with("//") {
        return None;
    }
    let (result_var, expr) = parse_let_assignment(line0)?;

    // Line 1 (skip comments): `let tB = tA;` or `let tB = tA; // duplicate top of stack`
    let dup_idx = next_code_line(statements, idx + 1)?;
    let line1 = statements[dup_idx].trim();
    let (dup_var, dup_rhs) = parse_let_assignment(line1)?;
    if dup_rhs != result_var {
        return None;
    }

    // Line 2 (skip comments): `let tC = <bound>;`
    let bound_idx = next_code_line(statements, dup_idx + 1)?;
    let line2 = statements[bound_idx].trim();
    let (_bound_var, bound_val) = parse_let_assignment(line2)?;
    if !OVERFLOW_BOUNDS.contains(&bound_val.as_str()) {
        return None;
    }

    // Line 3 (skip comments): `if tB < tC {` or `if tB == tC {`
    let if_idx = next_code_line(statements, bound_idx + 1)?;
    let line3 = statements[if_idx].trim();
    if !line3.starts_with("if ") || !line3.ends_with('{') {
        return None;
    }
    // Verify the condition references our DUP variable
    if !line3.contains(&format!("{dup_var} <")) && !line3.contains(&format!("{dup_var} ==")) {
        return None;
    }

    // Find the end of the entire overflow block (if + optional else).
    let block_end = find_overflow_block_end(statements, if_idx)?;

    // Determine checked vs unchecked by inspecting the first code statement
    // inside the if body.
    let first_body = ((if_idx + 1)..statements.len())
        .find(|&i| {
            let t = statements[i].trim();
            !t.is_empty() && !t.starts_with("//")
        })
        .map(|i| statements[i].trim().to_string());

    let is_checked = first_body
        .as_deref()
        .map_or(false, |s| s.starts_with("throw("));

    // Blank from the line after the operation through the block end.
    // This includes the DUP, bound, if-check, and all nested blocks,
    // plus any interleaved comment lines.
    Some(OverflowCollapse {
        op_line: idx,
        expr: expr.to_string(),
        result_var: result_var.to_string(),
        blank_start: idx + 1,
        blank_end: block_end,
        is_checked,
    })
}

/// Apply the collapse: rewrite the operation line and blank the wrapper lines.
fn apply_collapse(statements: &mut Vec<String>, collapse: &OverflowCollapse) {
    // Preserve the leading whitespace (indentation) of the operation line.
    let indent = leading_whitespace(&statements[collapse.op_line]);

    // Rewrite the operation line with optional `checked()` wrapper.
    if collapse.is_checked {
        statements[collapse.op_line] =
            format!("{indent}let {} = checked({});", collapse.result_var, collapse.expr);
    }
    // For unchecked, the original `let tA = <expr>;` is already correct.

    // Blank all lines from the DUP through the closing brace.
    for i in collapse.blank_start..=collapse.blank_end {
        statements[i].clear();
    }

    // Fix up dangling references: the STLOC after the overflow block may
    // reference a temp variable that was defined inside the now-blanked
    // wrapper (e.g. `loc0 = t15;` where t15 was the sign-extension result).
    // Replace it with the operation's result variable.
    if !collapse.is_checked {
        fixup_downstream_reference(statements, collapse.blank_end + 1, &collapse.result_var);
    }
}

/// Fix up a bare assignment after the collapsed block whose RHS references a
/// variable that was defined inside the now-blanked wrapper.
///
/// Scans forward from `start` for the first non-empty, non-comment line.
/// If it is a bare assignment `<var> = <rhs>;` where `<rhs>` is a single
/// identifier different from `result_var`, rewrite it to use `result_var`.
fn fixup_downstream_reference(statements: &mut [String], start: usize, result_var: &str) {
    let Some(idx) = next_code_line(statements, start) else {
        return;
    };
    let line = statements[idx].trim();
    // Must be a bare assignment (no `let` prefix), not a control-flow statement.
    if line.starts_with("let ") || line.starts_with("if ") || line.starts_with("//") {
        return;
    }
    if let Some((lhs, rhs)) = parse_bare_assignment(line) {
        // Only fix up single-identifier RHS that differs from result_var.
        if rhs != result_var && is_temp_identifier(&rhs) {
            let indent = leading_whitespace(&statements[idx]);
            statements[idx] = format!("{indent}{lhs} = {result_var};");
        }
    }
}

/// Parse `<var> = <rhs>;` (bare assignment, no `let`) and return `(var, rhs)`.
fn parse_bare_assignment(line: &str) -> Option<(String, String)> {
    let semi_pos = line.find(';')?;
    let body = &line[..semi_pos];
    let eq_pos = body.find(" = ")?;
    let lhs = body[..eq_pos].trim();
    // Reject `let` assignments — those are handled separately.
    if lhs.starts_with("let ") {
        return None;
    }
    let rhs = body[eq_pos + 3..].trim();
    Some((lhs.to_string(), rhs.to_string()))
}

/// Check if a string looks like a compiler-generated temp variable (e.g. `t15`).
fn is_temp_identifier(s: &str) -> bool {
    s.starts_with('t') && s.len() > 1 && s[1..].chars().all(|c| c.is_ascii_digit())
}

/// Extract leading whitespace from a string.
fn leading_whitespace(s: &str) -> &str {
    let trimmed = s.trim_start();
    &s[..s.len() - trimmed.len()]
}

/// Parse `let <var> = <rhs>;` and return `(var, rhs)`.
///
/// Handles trailing comments after the semicolon, e.g.:
/// `let t6 = t5; // duplicate top of stack`
fn parse_let_assignment(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("let ")?;
    // Find the first semicolon — everything after it is a comment.
    let semi_pos = rest.find(';')?;
    let rest = &rest[..semi_pos];
    let eq_pos = rest.find(" = ")?;
    let var = rest[..eq_pos].trim().to_string();
    let rhs = rest[eq_pos + 3..].trim().to_string();
    Some((var, rhs))
}

/// Find the end of the overflow block starting at the `if ... {` line.
///
/// This handles the common pattern where the if-block is followed by an
/// `else { ... }` block containing the masking/sign-extension logic.
/// Returns the index of the final closing `}`.
fn find_overflow_block_end(statements: &[String], if_idx: usize) -> Option<usize> {
    let mut end = find_matching_brace(statements, if_idx)?;

    // Check if the next non-empty/non-comment line is `else {`.
    // If so, the else block is part of the overflow pattern too.
    if let Some(next) = next_code_line(statements, end + 1) {
        let trimmed = statements[next].trim();
        if trimmed == "else {" || trimmed == "} else {" {
            if let Some(else_end) = find_matching_brace(statements, next) {
                end = else_end;
            }
        }
    }

    Some(end)
}

/// Find the index of the closing `}` that matches the `{` at `open_idx`.
fn find_matching_brace(statements: &[String], open_idx: usize) -> Option<usize> {
    let mut depth = 1i32;
    for i in (open_idx + 1)..statements.len() {
        let trimmed = statements[i].trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        // Count brace opens (lines ending with `{`)
        if trimmed.ends_with('{') {
            depth += 1;
        }
        // Count brace closes
        if trimmed == "}" || trimmed.starts_with("} ") {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stmts(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn collapses_unchecked_int32_add() {
        let mut s = stmts(&[
            "let t0 = a + b;",
            "let t1 = t0;",
            "let t2 = -2147483648;",
            "if t1 < t2 {",
            "goto label_0x001A;",
            "let t3 = t0;",
            "let t4 = 2147483647;",
            "if t3 > t4 {",
            "let t5 = 4294967295;",
            "let t6 = t0 & t5;",
            "}",
            "}",
        ]);
        HighLevelEmitter::collapse_overflow_checks(&mut s);
        assert_eq!(s[0], "let t0 = a + b;");
        for i in 1..=11 {
            assert!(s[i].is_empty(), "line {i} should be blank: {:?}", s[i]);
        }
    }

    #[test]
    fn collapses_checked_int32_add() {
        let mut s = stmts(&[
            "let t0 = a + b;",
            "let t1 = t0;",
            "let t2 = -2147483648;",
            "if t1 < t2 {",
            "throw(t0);",
            "let t3 = 2147483647;",
            "throw(t3);",
            "return;",
            "}",
        ]);
        HighLevelEmitter::collapse_overflow_checks(&mut s);
        assert_eq!(s[0], "let t0 = checked(a + b);");
        for i in 1..=8 {
            assert!(s[i].is_empty(), "line {i} should be blank: {:?}", s[i]);
        }
    }

    #[test]
    fn collapses_unsigned_range_check() {
        let mut s = stmts(&[
            "let t0 = a + b;",
            "let t1 = t0;",
            "let t2 = 0;",
            "if t1 < t2 {",
            "goto label_0x0084;",
            "let t3 = t0;",
            "let t4 = 4294967295;",
            "if t3 > t4 {",
            "let t5 = 4294967295;",
            "let t6 = t0 & t5;",
            "return t6;",
            "}",
            "}",
        ]);
        HighLevelEmitter::collapse_overflow_checks(&mut s);
        assert_eq!(s[0], "let t0 = a + b;");
        for i in 1..=12 {
            assert!(s[i].is_empty(), "line {i} should be blank: {:?}", s[i]);
        }
    }

    #[test]
    fn collapses_int64_range_check() {
        let mut s = stmts(&[
            "let t0 = a + b;",
            "let t1 = t0;",
            "let t2 = -9223372036854775808;",
            "if t1 < t2 {",
            "goto label_0x01AC;",
            "}",
        ]);
        HighLevelEmitter::collapse_overflow_checks(&mut s);
        assert_eq!(s[0], "let t0 = a + b;");
        for i in 1..=5 {
            assert!(s[i].is_empty(), "line {i} should be blank: {:?}", s[i]);
        }
    }

    #[test]
    fn does_not_match_unrelated_if() {
        let mut s = stmts(&[
            "let t0 = a + b;",
            "let t1 = t0;",
            "let t2 = 42;",
            "if t1 < t2 {",
            "return t0;",
            "}",
        ]);
        let original = s.clone();
        HighLevelEmitter::collapse_overflow_checks(&mut s);
        assert_eq!(s, original);
    }

    #[test]
    fn handles_negate_equality_check() {
        // Pattern: `if tB == <bound> {` (negate overflow check)
        let mut s = stmts(&[
            "let t0 = a;",
            "let t1 = t0;",
            "let t2 = -2147483648;",
            "if t1 == t2 {",
            "throw(a);",
            "return;",
            "}",
        ]);
        HighLevelEmitter::collapse_overflow_checks(&mut s);
        assert_eq!(s[0], "let t0 = checked(a);");
        for i in 1..=6 {
            assert!(s[i].is_empty(), "line {i} should be blank: {:?}", s[i]);
        }
    }

    #[test]
    fn skips_interleaved_comments() {
        // Real decompiler output has comment lines between code statements.
        let mut s = stmts(&[
            "let t5 = t4 + 1;",
            "// 0029: DUP",
            "let t6 = t5; // duplicate top of stack",
            "// 002A: PUSHINT32",
            "let t7 = -2147483648;",
            "// 002F: JMPGE",
            "if t6 < t7 {",
            "}",
            "else {",
            "// 0033: DUP",
            "let t8 = t5; // duplicate top of stack",
            "// 0034: PUSHINT32",
            "let t9 = 2147483647;",
            "// 0039: JMPLE",
            "if t8 > t9 {",
            "}",
            "// 003B: PUSHINT64",
            "let t10 = 4294967295;",
            "// 0044: AND",
            "let t11 = t5 & t10;",
            "// 0045: DUP",
            "let t12 = t11; // duplicate top of stack",
            "// 0046: PUSHINT32",
            "let t13 = 2147483647;",
            "// 004B: JMPLE",
            "if t12 > t13 {",
            "// 004D: PUSHINT64",
            "let t14 = 4294967296;",
            "// 0056: SUB",
            "let t15 = t11 - t14;",
            "}",
            "let t5 = t15;",
            "}",
        ]);
        HighLevelEmitter::collapse_overflow_checks(&mut s);
        assert_eq!(s[0], "let t5 = t4 + 1;");
        // Everything from line 1 through the final `}` should be blanked.
        for i in 1..s.len() {
            assert!(s[i].is_empty(), "line {i} should be blank: {:?}", s[i]);
        }
    }

    #[test]
    fn handles_if_else_without_comments() {
        // Simplified if/else pattern without interleaved comments.
        let mut s = stmts(&[
            "let t0 = a + b;",
            "let t1 = t0;",
            "let t2 = -2147483648;",
            "if t1 < t2 {",
            "}",
            "else {",
            "let t3 = t0;",
            "let t4 = 2147483647;",
            "if t3 > t4 {",
            "}",
            "let t5 = 4294967295;",
            "let t6 = t0 & t5;",
            "}",
        ]);
        HighLevelEmitter::collapse_overflow_checks(&mut s);
        assert_eq!(s[0], "let t0 = a + b;");
        for i in 1..s.len() {
            assert!(s[i].is_empty(), "line {i} should be blank: {:?}", s[i]);
        }
    }

    #[test]
    fn preserves_indentation() {
        let mut s = stmts(&[
            "        let t0 = a + b;",
            "        let t1 = t0;",
            "        let t2 = -2147483648;",
            "        if t1 < t2 {",
            "            throw(t0);",
            "        }",
        ]);
        HighLevelEmitter::collapse_overflow_checks(&mut s);
        assert_eq!(s[0], "        let t0 = checked(a + b);");
        for i in 1..=5 {
            assert!(s[i].is_empty(), "line {i} should be blank: {:?}", s[i]);
        }
    }
}
