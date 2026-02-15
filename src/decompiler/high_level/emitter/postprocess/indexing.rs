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

    let get_pos = expr.find(" get ");
    let has_key_pos = expr.find(" has_key ");
    let (pos, kind) = match (get_pos, has_key_pos) {
        (Some(a), Some(b)) => {
            if a < b {
                (a, "get")
            } else {
                (b, "has_key")
            }
        }
        (Some(a), None) => (a, "get"),
        (None, Some(b)) => (b, "has_key"),
        (None, None) => return expr.to_string(),
    };

    let (left, rest) = expr.split_at(pos);
    let (op, right) = match kind {
        "get" => (" get ", rest.strip_prefix(" get ").unwrap_or_default()),
        _ => (
            " has_key ",
            rest.strip_prefix(" has_key ").unwrap_or_default(),
        ),
    };

    // Recurse to handle nested infix uses.
    let left = rewrite_expr(left);
    let right = rewrite_expr(right);

    if op == " get " {
        format!("{left}[{right}]")
    } else {
        format!("has_key({left}, {right})")
    }
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
