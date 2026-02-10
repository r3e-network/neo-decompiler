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
        _ => None,
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
