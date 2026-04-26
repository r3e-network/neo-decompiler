use super::super::HighLevelEmitter;

impl HighLevelEmitter {
    pub(crate) fn rewrite_compound_assignments(statements: &mut [String]) {
        for statement in statements {
            let trimmed = statement.trim();
            if trimmed.starts_with("for (") && trimmed.ends_with('{') {
                if let Some((init, condition, increment)) = Self::parse_for_parts(trimmed) {
                    if let Some(rewritten) = rewrite_increment(&increment) {
                        *statement = format!("for ({init}; {condition}; {rewritten}) {{");
                    }
                }
                continue;
            }

            // Strip redundant outer parens in `return (EXPR);` — the
            // statement-terminating semicolon already delimits the
            // expression, so the parens added by the single-use-temp
            // inliner are noise.
            if let Some(inner) = trimmed
                .strip_prefix("return ")
                .and_then(|s| s.strip_suffix(';'))
            {
                let stripped = strip_outer_parens(inner.trim());
                if stripped != inner {
                    let indent_len = statement.len() - statement.trim_start().len();
                    let indent = &statement[..indent_len];
                    *statement = format!("{indent}return {stripped};");
                    continue;
                }
            }

            if trimmed.starts_with("let ") {
                continue;
            }

            let Some(assign) = Self::parse_assignment(trimmed) else {
                continue;
            };

            if let Some((op, rhs_tail)) = rewrite_rhs(assign.lhs.as_str(), assign.rhs.as_str()) {
                *statement = format!("{} {op} {rhs_tail};", assign.lhs);
            }
        }
    }
}

fn rewrite_increment(increment: &str) -> Option<String> {
    let mut parts = increment.splitn(2, '=');
    let lhs = parts.next()?.trim();
    let rhs = parts.next()?.trim();
    if !is_identifier(lhs) {
        return None;
    }
    let (op, rhs_tail) = rewrite_rhs(lhs, rhs)?;
    Some(format!("{lhs} {op} {rhs_tail}"))
}

fn rewrite_rhs<'a>(lhs: &str, rhs: &'a str) -> Option<(&'static str, &'a str)> {
    // Inline-single-use-temps may wrap the RHS in outer parens
    // (e.g. `loc0 = (loc0 + 1);`). Strip a single matching pair so the
    // compound-assignment pattern still matches.
    let inner = strip_outer_parens(rhs);
    let plus_prefix = format!("{lhs} + ");
    if let Some(rest) = inner.strip_prefix(plus_prefix.as_str()) {
        return Some(("+=", rest));
    }
    let minus_prefix = format!("{lhs} - ");
    if let Some(rest) = inner.strip_prefix(minus_prefix.as_str()) {
        return Some(("-=", rest));
    }
    None
}

use super::super::helpers::strip_outer_parens;

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if first != '_' && !first.is_ascii_alphabetic() {
        return false;
    }
    chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_simple_assignment_into_compound_form() {
        let mut statements = vec!["loc0 = loc0 + 1;".to_string()];

        HighLevelEmitter::rewrite_compound_assignments(&mut statements);

        assert_eq!(statements[0], "loc0 += 1;");
    }

    #[test]
    fn does_not_rewrite_let_bindings() {
        let mut statements = vec!["let loc0 = loc0 + 1;".to_string()];

        HighLevelEmitter::rewrite_compound_assignments(&mut statements);

        assert_eq!(statements[0], "let loc0 = loc0 + 1;");
    }

    #[test]
    fn rewrites_for_header_increment_expression() {
        let mut statements = vec![
            "for (let loc0 = 0; loc0 < 3; loc0 = loc0 + 1) {".to_string(),
            "}".to_string(),
        ];

        HighLevelEmitter::rewrite_compound_assignments(&mut statements);

        assert_eq!(statements[0], "for (let loc0 = 0; loc0 < 3; loc0 += 1) {");
    }
}
