use std::collections::BTreeSet;

use crate::decompiler::analysis::types::ValueType;
use crate::decompiler::ir::{Expr, Literal};

use super::expr::{render_expr_prec, ExprContext};

pub(super) fn render_expr_list(
    expressions: &[Expr],
    context: &ExprContext,
    expanding: &mut BTreeSet<String>,
) -> String {
    expressions
        .iter()
        .map(|expression| render_expr_prec(expression, 0, context, expanding))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) fn render_object_array_literal(elements: &[Expr], context: &ExprContext) -> String {
    let mut expanding = BTreeSet::new();
    format!(
        "new object[] {{ {} }}",
        render_expr_list(elements, context, &mut expanding)
    )
}

pub(super) fn int_cast(
    expression: &Expr,
    context: &ExprContext,
    expanding: &mut BTreeSet<String>,
) -> String {
    let rendered = render_expr_prec(expression, 0, context, expanding);
    if context.is_statically_exact_csharp_type(expression, "int") {
        rendered
    } else {
        format!("(int)({rendered})")
    }
}

pub(super) fn render_new_array(
    length: &Expr,
    element_type: Option<ValueType>,
    context: &ExprContext,
    expanding: &mut BTreeSet<String>,
) -> String {
    let length = int_cast(length, context, expanding);
    match element_type.unwrap_or(ValueType::Any) {
        ValueType::Boolean => format!("new bool[{length}]"),
        ValueType::Integer => format!("new BigInteger[{length}]"),
        ValueType::ByteString => format!("new ByteString[{length}]"),
        ValueType::Buffer => format!("new byte[{length}][]"),
        ValueType::Array | ValueType::Struct => format!("new object[{length}][]"),
        ValueType::Map => format!("new Map<object, object>[{length}]"),
        ValueType::Unknown
        | ValueType::Any
        | ValueType::Null
        | ValueType::InteropInterface
        | ValueType::Pointer => format!("new object[{length}]"),
    }
}

pub(super) fn render_literal(literal: &Literal) -> String {
    match literal {
        Literal::Int(value) => value.to_string(),
        Literal::BigInt(value) => {
            format!("BigInteger.Parse(\"{}\")", escape_csharp_string(value))
        }
        Literal::Bool(value) => value.to_string(),
        Literal::String(value) => format!("\"{}\"", escape_csharp_string(value)),
        Literal::Bytes(bytes) => format!(
            "(ByteString)new byte[] {{ {} }}",
            bytes
                .iter()
                .map(|byte| format!("0x{byte:02X}"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Literal::Null => "null".to_string(),
    }
}

pub(super) fn escape_csharp_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\0' => escaped.push_str("\\0"),
            '\u{0007}' => escaped.push_str("\\a"),
            '\u{0008}' => escaped.push_str("\\b"),
            '\u{000C}' => escaped.push_str("\\f"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\u{000B}' => escaped.push_str("\\v"),
            '\u{2028}' => escaped.push_str("\\u2028"),
            '\u{2029}' => escaped.push_str("\\u2029"),
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            control if control.is_control() => {
                use std::fmt::Write;
                write!(escaped, "\\u{:04X}", u32::from(control)).unwrap();
            }
            other => escaped.push(other),
        }
    }
    escaped
}
