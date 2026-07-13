use crate::decompiler::analysis::types::ValueType;
use crate::instruction::Operand;

/// Decode a Neo VM StackItemType operand into the language-neutral type model.
pub(crate) fn value_type_from_operand(operand: &Operand) -> Option<ValueType> {
    let byte = match operand {
        Operand::U8(value) => *value,
        Operand::I8(value) => *value as u8,
        _ => return None,
    };

    match byte {
        0x00 => Some(ValueType::Any),
        0x10 => Some(ValueType::Pointer),
        0x20 => Some(ValueType::Boolean),
        0x21 => Some(ValueType::Integer),
        0x28 => Some(ValueType::ByteString),
        0x30 => Some(ValueType::Buffer),
        0x40 => Some(ValueType::Array),
        0x41 => Some(ValueType::Struct),
        0x48 => Some(ValueType::Map),
        0x60 => Some(ValueType::InteropInterface),
        _ => None,
    }
}

/// Encode a language-neutral type as a Neo VM StackItemType operand byte.
pub(crate) fn stack_item_type_tag(value_type: ValueType) -> Option<u8> {
    Some(match value_type {
        ValueType::Any => 0x00,
        ValueType::Pointer => 0x10,
        ValueType::Boolean => 0x20,
        ValueType::Integer => 0x21,
        ValueType::ByteString => 0x28,
        ValueType::Buffer => 0x30,
        ValueType::Array => 0x40,
        ValueType::Struct => 0x41,
        ValueType::Map => 0x48,
        ValueType::InteropInterface => 0x60,
        ValueType::Unknown | ValueType::Null => return None,
    })
}

/// Decode printable UTF-8 data without reclassifying arbitrary binary payloads.
pub(crate) fn printable_utf8(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() {
        return None;
    }
    let value = std::str::from_utf8(bytes).ok()?;
    value
        .chars()
        .all(|character| matches!(character, ' '..='~' | '\n' | '\r' | '\t'))
        .then(|| value.to_string())
}

/// Convert little-endian signed two's-complement bytes to canonical decimal.
pub(crate) fn signed_le_bytes_to_decimal(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "0".to_string();
    }
    if bytes.len() == 16 {
        let value = i128::from_le_bytes(bytes.try_into().expect("length checked above"));
        return value.to_string();
    }

    let is_negative = bytes.last().is_some_and(|byte| byte & 0x80 != 0);
    let mut magnitude: Vec<u8> = if is_negative {
        let mut carry: u16 = 1;
        bytes
            .iter()
            .map(|byte| {
                let sum = u16::from(!byte) + carry;
                carry = sum >> 8;
                sum as u8
            })
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    } else {
        bytes.iter().copied().rev().collect()
    };

    while magnitude.len() > 1 && magnitude[0] == 0 {
        magnitude.remove(0);
    }
    if magnitude == [0] {
        return "0".to_string();
    }

    let mut digits = Vec::new();
    while magnitude != [0] {
        let mut remainder: u16 = 0;
        for byte in &mut magnitude {
            let dividend = (remainder << 8) | u16::from(*byte);
            *byte = (dividend / 10) as u8;
            remainder = dividend % 10;
        }
        digits.push((remainder as u8) + b'0');
        while magnitude.len() > 1 && magnitude[0] == 0 {
            magnitude.remove(0);
        }
    }
    digits.reverse();
    let decimal: String = digits.into_iter().map(char::from).collect();
    if is_negative {
        format!("-{decimal}")
    } else {
        decimal
    }
}

#[cfg(test)]
mod tests {
    use super::{stack_item_type_tag, value_type_from_operand};
    use crate::decompiler::analysis::types::ValueType;
    use crate::instruction::Operand;

    #[test]
    fn stack_item_type_tags_round_trip() {
        for value_type in [
            ValueType::Any,
            ValueType::Pointer,
            ValueType::Boolean,
            ValueType::Integer,
            ValueType::ByteString,
            ValueType::Buffer,
            ValueType::Array,
            ValueType::Struct,
            ValueType::Map,
            ValueType::InteropInterface,
        ] {
            let tag = stack_item_type_tag(value_type).expect("VM type has an operand tag");
            assert_eq!(value_type_from_operand(&Operand::U8(tag)), Some(value_type));
        }
        assert_eq!(stack_item_type_tag(ValueType::Unknown), None);
        assert_eq!(stack_item_type_tag(ValueType::Null), None);
    }
}
