// Widening casts (i8/i16/i32/u8/u16/u32 → i64) are lossless; the i8→u8 cast
// in convert_target_name is a deliberate reinterpretation of a type-tag byte.
#![allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]

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
        Some(Operand::Bytes(bytes)) => try_decode_string_literal(bytes)
            .map(LiteralValue::String),
        _ => None,
    }
}

/// Try to decode a byte slice as a printable UTF-8 string literal.
/// Returns `Some(decoded)` only when the bytes are valid UTF-8 and every
/// character is printable ASCII (0x20..=0x7E) or common whitespace (\n, \r, \t).
/// A minimum length of 1 is required to avoid false positives with empty data.
pub(super) fn try_decode_string_literal(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() {
        return None;
    }
    let s = std::str::from_utf8(bytes).ok()?;
    let all_printable = s.chars().all(|c| matches!(c, ' '..='~' | '\n' | '\r' | '\t'));
    if all_printable {
        Some(s.to_string())
    } else {
        None
    }
}

/// Format a PUSHDATA operand for display: decode as a quoted string literal
/// when the bytes are printable UTF-8, otherwise fall back to hex.
pub(super) fn format_pushdata(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "\"\"".to_string();
    }
    if let Some(s) = try_decode_string_literal(bytes) {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        format!("0x{}", hex::encode_upper(bytes))
    }
}

pub(super) fn convert_target_name(operand: &Operand) -> Option<&'static str> {
    let byte = match operand {
        Operand::U8(v) => *v,
        Operand::I8(v) => *v as u8,
        _ => return None,
    };

    match byte {
        0x00 => Some("any"),
        0x10 => Some("pointer"),
        0x20 => Some("bool"),
        0x21 => Some("integer"),
        0x28 => Some("bytestring"),
        0x30 => Some("buffer"),
        0x40 => Some("array"),
        0x41 => Some("struct"),
        0x48 => Some("map"),
        0x60 => Some("interopinterface"),
        _ => None,
    }
}
