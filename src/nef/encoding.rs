use crate::error::{NefError, Result};

use super::types::MethodToken;

fn read_u16_le(bytes: &[u8]) -> u16 {
    let mut array = [0u8; 2];
    array.copy_from_slice(bytes);
    u16::from_le_bytes(array)
}

fn read_u32_le(bytes: &[u8]) -> u32 {
    let mut array = [0u8; 4];
    array.copy_from_slice(bytes);
    u32::from_le_bytes(array)
}

fn read_u64_le(bytes: &[u8]) -> u64 {
    let mut array = [0u8; 8];
    array.copy_from_slice(bytes);
    u64::from_le_bytes(array)
}

pub(super) fn read_varint(bytes: &[u8], offset: usize) -> Result<(u32, usize)> {
    let first = *bytes
        .get(offset)
        .ok_or(NefError::UnexpectedEof { offset })?;
    let (value, consumed) = match first {
        0x00..=0xFC => (first as u32, 1),
        0xFD => {
            let slice = bytes
                .get(offset + 1..offset + 3)
                .ok_or(NefError::UnexpectedEof { offset })?;
            let value = read_u16_le(slice);
            (value as u32, 3)
        }
        0xFE => {
            let slice = bytes
                .get(offset + 1..offset + 5)
                .ok_or(NefError::UnexpectedEof { offset })?;
            let value = read_u32_le(slice);
            (value, 5)
        }
        0xFF => {
            let slice = bytes
                .get(offset + 1..offset + 9)
                .ok_or(NefError::UnexpectedEof { offset })?;
            let value = read_u64_le(slice);
            if value > u32::MAX as u64 {
                return Err(NefError::IntegerOverflow { offset }.into());
            }
            (value as u32, 9)
        }
    };

    if consumed != varint_encoded_len(value) {
        return Err(NefError::NonCanonicalVarInt { offset }.into());
    }

    Ok((value, consumed))
}

pub(super) fn encoded_method_tokens_size(tokens: &[MethodToken]) -> usize {
    let mut size = varint_encoded_len(tokens.len() as u32);
    for token in tokens {
        size += 20; // hash
        size += varint_encoded_len(token.method.len() as u32);
        size += token.method.len();
        size += 2; // parameters count
        size += 1; // return value flag
        size += 1; // call flags
    }
    size
}

pub(super) fn varint_encoded_len(value: u32) -> usize {
    match value {
        0x00..=0xFC => 1,
        0xFD..=0xFFFF => 3,
        _ => 5,
    }
}

pub(super) fn read_varstring(
    bytes: &[u8],
    offset: usize,
    max_len: usize,
) -> Result<(String, usize)> {
    let (len, consumed) = read_varint(bytes, offset)?;
    let len = len as usize;
    if len > max_len {
        return Err(NefError::SourceTooLong {
            length: len,
            max: max_len,
        }
        .into());
    }
    let start = offset + consumed;
    let end = start + len;
    let slice = bytes
        .get(start..end)
        .ok_or(NefError::UnexpectedEof { offset: start })?;
    let value = std::str::from_utf8(slice)
        .map_err(|_| NefError::InvalidUtf8String { offset: start })?
        .to_string();
    Ok((value, consumed + slice.len()))
}

pub(super) fn read_varbytes(
    bytes: &[u8],
    offset: usize,
    max_len: usize,
) -> Result<(Vec<u8>, usize)> {
    let (len, consumed) = read_varint(bytes, offset)?;
    let len = len as usize;
    if len > max_len {
        return Err(NefError::ScriptTooLarge {
            length: len,
            max: max_len,
        }
        .into());
    }
    let start = offset + consumed;
    let end = start + len;
    let slice = bytes
        .get(start..end)
        .ok_or(NefError::UnexpectedEof { offset: start })?;
    Ok((slice.to_vec(), consumed + slice.len()))
}
