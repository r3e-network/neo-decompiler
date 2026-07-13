// Widening casts (i8/i16/i32/u8/u16/u32 → i64) are lossless; the i8→u8 cast
// in convert_target_name is a deliberate reinterpretation of a type-tag byte.
#![allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]

use crate::decompiler::analysis::types::ValueType;
use crate::decompiler::helpers::{
    printable_utf8, signed_le_bytes_to_decimal, value_type_from_operand,
};
use crate::instruction::Operand;

use super::LiteralValue;

pub(super) fn literal_from_operand(operand: Option<&Operand>) -> Option<LiteralValue> {
    match operand {
        Some(Operand::I8(v)) => Some(LiteralValue::Integer(*v as i64)),
        Some(Operand::I16(v)) => Some(LiteralValue::Integer(*v as i64)),
        Some(Operand::I32(v)) => Some(LiteralValue::Integer(*v as i64)),
        Some(Operand::I64(v)) => Some(LiteralValue::Integer(*v)),
        Some(Operand::U8(v)) => Some(LiteralValue::Integer(*v as i64)),
        Some(Operand::U16(v)) => Some(LiteralValue::Integer(*v as i64)),
        Some(Operand::U32(v)) => Some(LiteralValue::Integer(*v as i64)),
        Some(Operand::Bool(v)) => Some(LiteralValue::Boolean(*v)),
        Some(Operand::Bytes(bytes)) => printable_utf8(bytes).map(LiteralValue::String),
        _ => None,
    }
}

/// Format a PUSHDATA operand for display: decode as a quoted string literal
/// when the bytes are printable UTF-8, otherwise fall back to hex.
pub(super) fn format_pushdata(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "\"\"".to_string();
    }
    if let Some(s) = printable_utf8(bytes) {
        // Escape control characters so the quoted literal is unambiguous and,
        // in the C# render path, compiles (a raw newline in a "..." constant is
        // C# error CS1010). Backslash first so escapes aren't double-escaped.
        format!(
            "\"{}\"",
            s.replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n")
                .replace('\r', "\\r")
                .replace('\t', "\\t")
        )
    } else {
        format!("0x{}", hex::encode_upper(bytes))
    }
}

pub(super) fn convert_target_name(operand: &Operand) -> Option<&'static str> {
    Some(match value_type_from_operand(operand)? {
        ValueType::Any => "any",
        ValueType::Pointer => "pointer",
        ValueType::Boolean => "bool",
        ValueType::Integer => "integer",
        ValueType::ByteString => "bytestring",
        ValueType::Buffer => "buffer",
        ValueType::Array => "array",
        ValueType::Struct => "struct",
        ValueType::Map => "map",
        ValueType::InteropInterface => "interopinterface",
        ValueType::Unknown | ValueType::Null => return None,
    })
}

/// Convert a little-endian signed two's-complement byte slice into a decimal
/// string.  Handles 16-byte (i128) natively and falls back to manual big-int
/// division for 32-byte (PUSHINT256) values.
pub(super) fn format_int_bytes_as_decimal(bytes: &[u8]) -> String {
    signed_le_bytes_to_decimal(bytes)
}

pub(super) fn format_type_operand(operand: &Operand) -> String {
    match operand {
        Operand::U8(value) => format!("0x{value:02X}"),
        Operand::I8(value) => format!("0x{value:02X}"),
        Operand::U16(value) => format!("{value}"),
        Operand::I16(value) => format!("{value}"),
        Operand::U32(value) => format!("{value}"),
        Operand::I32(value) => format!("{value}"),
        Operand::I64(value) => format!("{value}"),
        Operand::Bytes(bytes) => format!("0x{}", hex::encode_upper(bytes)),
        _ => operand.to_string(),
    }
}

/// Strip a single matching pair of outer parentheses, but only if they
/// actually balance — `(a) + (b)` must NOT be stripped to `a) + (b`.
pub(super) fn strip_outer_parens(text: &str) -> &str {
    let bytes = text.as_bytes();
    if bytes.len() < 2 || bytes[0] != b'(' || bytes[bytes.len() - 1] != b')' {
        return text;
    }
    let mut depth = 0i32;
    for (i, byte) in bytes.iter().enumerate() {
        match byte {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 && i < bytes.len() - 1 {
                    return text;
                }
            }
            _ => {}
        }
    }
    &text[1..bytes.len() - 1]
}
