use super::{NefError, Result};

fn read_u16_le(bytes: &[u8]) -> u16 {
    let mut array = [0u8; 2];
    array.copy_from_slice(bytes);
    u16::from_le_bytes(array)
}

pub(super) fn expect_reserved_byte_zero(bytes: &[u8], offset: &mut usize) -> Result<()> {
    let reserved = *bytes
        .get(*offset)
        .ok_or(NefError::UnexpectedEof { offset: *offset })?;
    if reserved != 0 {
        return Err(NefError::ReservedByteNonZero {
            offset: *offset,
            value: reserved,
        }
        .into());
    }
    *offset += 1;
    Ok(())
}

pub(super) fn expect_reserved_word_zero(bytes: &[u8], offset: &mut usize) -> Result<()> {
    let reserved_word_bytes = bytes
        .get(*offset..*offset + 2)
        .ok_or(NefError::UnexpectedEof { offset: *offset })?;
    let reserved_word = read_u16_le(reserved_word_bytes);
    if reserved_word != 0 {
        return Err(NefError::ReservedWordNonZero {
            offset: *offset,
            value: reserved_word,
        }
        .into());
    }
    *offset += 2;
    Ok(())
}
