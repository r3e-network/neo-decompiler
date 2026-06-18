//! Rewrite collection ops into more idiomatic indexing syntax.

use super::super::HighLevelEmitter;
use super::util::{extract_else_if_condition, extract_if_condition};

impl HighLevelEmitter {
    /// Rewrite `get`/`set_item`/`has_key` patterns into bracket/function forms.
    pub(crate) fn rewrite_indexing_syntax(statements: &mut [String]) {
        for statement in statements {
            let trimmed = statement.trim();
            if trimmed.is_empty() || trimmed.starts_with("//") {
                continue;
            }

            if let Some(rewritten) = rewrite_set_item(trimmed) {
                *statement = rewritten;
                continue;
            }

            if trimmed.starts_with("for (") && trimmed.ends_with('{') {
                if let Some((init, condition, increment)) = Self::parse_for_parts(trimmed) {
                    let init = rewrite_expr(init.as_str());
                    let condition = rewrite_expr(condition.as_str());
                    let increment = rewrite_expr(increment.as_str());
                    *statement = format!("for ({init}; {condition}; {increment}) {{");
                }
                continue;
            }

            if let Some(condition) = extract_if_condition(trimmed) {
                let rewritten = rewrite_expr(condition);
                *statement = format!("if {rewritten} {{");
                continue;
            }

            if let Some(condition) = extract_else_if_condition(trimmed) {
                let rewritten = rewrite_expr(condition);
                let prefix = if trimmed.starts_with("} ") { "} " } else { "" };
                *statement = format!("{prefix}else if {rewritten} {{");
                continue;
            }

            if let Some(condition) = Self::extract_while_condition(trimmed) {
                let rewritten = rewrite_expr(condition.as_str());
                *statement = format!("while {rewritten} {{");
                continue;
            }

            if let Some(assign) = Self::parse_assignment(trimmed) {
                let rewritten_rhs = rewrite_expr(assign.rhs.as_str());
                if trimmed.starts_with("let ") {
                    *statement = format!("let {} = {};", assign.lhs, rewritten_rhs);
                } else {
                    *statement = format!("{} = {};", assign.lhs, rewritten_rhs);
                }
                continue;
            }

            if trimmed.ends_with(';') {
                *statement = rewrite_expr(trimmed.trim_end_matches(';')).to_string() + ";";
            }
        }
    }
}

fn rewrite_set_item(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let body = trimmed.strip_prefix("set_item(")?.strip_suffix(");")?;
    let args = split_args(body);
    if args.len() != 3 {
        return None;
    }
    let target = rewrite_expr(args[0].as_str());
    let key = rewrite_expr(args[1].as_str());
    let value = rewrite_expr(args[2].as_str());
    Some(format!("{target}[{key}] = {value};"))
}

fn rewrite_expr(expr: &str) -> String {
    let expr = expr.trim();
    if expr.is_empty() {
        return String::new();
    }

    let Some((pos, kind)) = find_expr_op(expr) else {
        return expr.to_string();
    };

    let (left, rest) = expr.split_at(pos);
    let right = match kind {
        "get" => rest.strip_prefix(" get ").unwrap_or_default(),
        _ => rest.strip_prefix(" has_key ").unwrap_or_default(),
    };

    // Recurse to handle nested infix uses.
    let left = rewrite_expr(left);
    let right = rewrite_expr(right);

    if kind == "get" {
        format!("{left}[{right}]")
    } else {
        format!("has_key({left}, {right})")
    }
}

/// Find the leftmost ` get ` / ` has_key ` operator that is not enclosed in a
/// string literal, so a literal such as `"a get b"` is not mistaken for an
/// index. Returns the byte offset (always at an ASCII space, hence a valid
/// char boundary) and the operator kind.
fn find_expr_op(expr: &str) -> Option<(usize, &'static str)> {
    let bytes = expr.as_bytes();
    let mut in_string = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if in_string {
            if b == b'\\' && i + 1 < bytes.len() {
                i += 2; // skip the escaped character
                continue;
            }
            if b == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if b == b'"' {
            in_string = true;
            i += 1;
            continue;
        }
        if bytes[i..].starts_with(b" get ") {
            return Some((i, "get"));
        }
        if bytes[i..].starts_with(b" has_key ") {
            return Some((i, "has_key"));
        }
        i += 1;
    }
    None
}

fn split_args(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();
    for ch in text.chars() {
        match ch {
            '(' | '[' | '{' => {
                depth += 1;
                current.push(ch);
            }
            ')' | ']' | '}' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ',' if depth == 0 => {
                out.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        out.push(current.trim().to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::super::HighLevelEmitter;

    #[test]
    fn rewrite_indexing_preserves_get_token_inside_string_literal() {
        // Regression: ` get ` / ` has_key ` inside a string literal must not be
        // mistaken for the index/has_key operators.
        let mut statements = vec![
            r#"return "a get b";"#.to_string(),
            r#"return "x has_key y";"#.to_string(),
        ];
        HighLevelEmitter::rewrite_indexing_syntax(&mut statements);
        assert_eq!(statements[0], r#"return "a get b";"#);
        assert_eq!(statements[1], r#"return "x has_key y";"#);
    }

    #[test]
    fn rewrite_indexing_still_rewrites_real_get_operator() {
        let mut statements = vec!["let t0 = loc0 get t1;".to_string()];
        HighLevelEmitter::rewrite_indexing_syntax(&mut statements);
        assert_eq!(statements[0], "let t0 = loc0[t1];");
    }
}
