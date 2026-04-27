pub(super) fn is_safe_to_inline(expr: &str) -> bool {
    // No call → safe (literals, identifiers, arithmetic, etc.).
    if !expr.contains('(') {
        return true;
    }
    let trimmed = expr.trim();
    // Plain parenthesised expression (`(a + b)`, `(x is null)`, etc.) — safe.
    if trimmed.starts_with('(') && trimmed.ends_with(')') {
        return true;
    }
    // Has a call: only safe when every call site is into a known
    // pure NEO helper (math, buffer, type-check). Side-effecting
    // calls (syscall, sub_0x, calla, callt, manifest method names)
    // must not be inlined — moving them across other operations
    // would reorder side effects.
    rhs_calls_only_pure_helpers(trimmed)
}

/// Walk `expr` and return `true` only if every `name(` it contains
/// names a known-pure NEO helper. Mirrors the logic in
/// `simplify::rhs_calls_only_pure_helpers` for the dead-temp pass.
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

fn is_pure_helper_identifier(ident: &str) -> bool {
    if matches!(
        ident,
        "abs"
            | "sign"
            | "sqrt"
            | "min"
            | "max"
            | "pow"
            | "modpow"
            | "modmul"
            | "within"
            | "left"
            | "right"
            | "substr"
            | "is_null"
            | "keys"
            | "values"
            | "has_key"
            | "len"
    ) {
        return true;
    }
    ident.starts_with("is_type_") || ident.starts_with("convert_to_")
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
