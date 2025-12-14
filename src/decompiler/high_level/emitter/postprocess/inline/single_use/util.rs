pub(super) fn is_safe_to_inline(expr: &str) -> bool {
    // Don't inline function calls (may have side effects).
    // Simple heuristic: if it contains '(' followed by something other than operators.
    if expr.contains('(') {
        // Allow simple parenthesized expressions.
        let trimmed = expr.trim();
        if trimmed.starts_with('(') && trimmed.ends_with(')') {
            return true;
        }
        // Likely a function call.
        return false;
    }
    true
}

pub(super) fn needs_parens(expr: &str) -> bool {
    // Add parens if expression contains operators that might affect precedence.
    expr.contains('+')
        || expr.contains('-')
        || expr.contains('*')
        || expr.contains('/')
        || expr.contains('%')
        || expr.contains("&&")
        || expr.contains("||")
        || expr.contains("==")
        || expr.contains("!=")
        || expr.contains('<')
        || expr.contains('>')
}

pub(super) fn is_control_flow_condition(statement: &str) -> bool {
    let trimmed = statement.trim();
    trimmed.starts_with("if ")
        || trimmed.starts_with("while ")
        || trimmed.starts_with("for ")
        || trimmed.starts_with("} else if ")
}

pub(super) fn is_trivial_inline_rhs(expr: &str) -> bool {
    let trimmed = expr.trim();
    if trimmed.is_empty() {
        return false;
    }

    if matches!(trimmed, "true" | "false" | "null") {
        return true;
    }

    if is_numeric_literal(trimmed) || is_string_literal(trimmed) {
        return true;
    }

    is_identifier(trimmed)
}

pub(super) fn is_temp_identifier(ident: &str) -> bool {
    let rest = ident.strip_prefix('t');
    let Some(rest) = rest else {
        return false;
    };
    !rest.is_empty() && rest.as_bytes().iter().all(u8::is_ascii_digit)
}

fn is_numeric_literal(text: &str) -> bool {
    let text = text.strip_prefix('-').unwrap_or(text);
    if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        return !hex.is_empty() && hex.as_bytes().iter().all(u8::is_ascii_hexdigit);
    }
    !text.is_empty() && text.as_bytes().iter().all(u8::is_ascii_digit)
}

fn is_string_literal(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() < 2 {
        return false;
    }
    (bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
        || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\'')
}

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
