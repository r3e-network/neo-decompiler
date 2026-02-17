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
        Some(Operand::Bytes(bytes)) => try_decode_string_literal(bytes).map(LiteralValue::String),
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
    let all_printable = s
        .chars()
        .all(|c| matches!(c, ' '..='~' | '\n' | '\r' | '\t'));
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

/// Convert a little-endian signed two's-complement byte slice into a decimal
/// string.  Handles 16-byte (i128) natively and falls back to manual big-int
/// division for 32-byte (PUSHINT256) values.
pub(super) fn format_int_bytes_as_decimal(bytes: &[u8]) -> String {
    if bytes.len() == 16 {
        let value = i128::from_le_bytes(bytes.try_into().unwrap());
        return value.to_string();
    }
    // General case: arbitrary-length little-endian signed two's complement.
    let is_negative = bytes.last().map_or(false, |b| b & 0x80 != 0);
    // Convert to big-endian unsigned magnitude.
    let mut magnitude: Vec<u8> = if is_negative {
        // Two's complement negate: invert all bits, then add 1.
        let mut carry: u16 = 1;
        bytes
            .iter()
            .map(|&b| {
                let sum = (!b) as u16 + carry;
                carry = sum >> 8;
                sum as u8
            })
            .collect::<Vec<u8>>()
            .into_iter()
            .rev()
            .collect()
    } else {
        bytes.iter().copied().rev().collect()
    };
    // Strip leading zeros.
    while magnitude.len() > 1 && magnitude[0] == 0 {
        magnitude.remove(0);
    }
    if magnitude == [0] {
        return "0".to_string();
    }
    // Repeated division by 10 to extract decimal digits.
    let mut digits = Vec::new();
    while magnitude != [0] {
        let mut remainder: u16 = 0;
        for byte in magnitude.iter_mut() {
            let dividend = (remainder << 8) | (*byte as u16);
            *byte = (dividend / 10) as u8;
            remainder = dividend % 10;
        }
        digits.push((remainder as u8) + b'0');
        while magnitude.len() > 1 && magnitude[0] == 0 {
            magnitude.remove(0);
        }
    }
    digits.reverse();
    let s = String::from_utf8(digits).unwrap();
    if is_negative {
        format!("-{s}")
    } else {
        s
    }
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
