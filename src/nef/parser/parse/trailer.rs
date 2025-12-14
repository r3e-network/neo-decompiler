use super::super::super::CHECKSUM_SIZE;
use super::{NefError, NefParser, Result};

pub(super) fn read_checksum(bytes: &[u8], checksum_start: usize) -> Result<u32> {
    let checksum_end = checksum_start + CHECKSUM_SIZE;
    let checksum_bytes =
        bytes
            .get(checksum_start..checksum_end)
            .ok_or(NefError::UnexpectedEof {
                offset: checksum_start,
            })?;
    Ok(u32::from_le_bytes(checksum_bytes.try_into().unwrap()))
}

pub(super) fn verify_checksum(bytes: &[u8], checksum_start: usize, checksum: u32) -> Result<()> {
    let payload = bytes
        .get(..checksum_start)
        .ok_or(NefError::UnexpectedEof { offset: 0 })?;
    let calculated = NefParser::calculate_checksum(payload);
    if checksum != calculated {
        return Err(NefError::ChecksumMismatch {
            expected: checksum,
            calculated,
        }
        .into());
    }
    Ok(())
}

pub(super) fn expect_end_of_file(bytes: &[u8], offset: usize) -> Result<()> {
    if offset != bytes.len() {
        return Err(NefError::TrailingData {
            extra: bytes.len() - offset,
        }
        .into());
    }
    Ok(())
}
