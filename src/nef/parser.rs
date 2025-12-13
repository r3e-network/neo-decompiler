use crate::error::{NefError, Result};

use sha2::{Digest, Sha256};

use super::encoding::{read_varbytes, read_varstring};
use super::types::{NefFile, NefHeader};
use super::{CHECKSUM_SIZE, FIXED_HEADER_SIZE, MAGIC, MAX_SCRIPT_LEN, MAX_SOURCE_LEN};

mod method_tokens;

/// Parser for Neo N3 NEF containers.
///
/// This type is stateless and can be reused across many parse calls.
#[derive(Debug, Default, Clone, Copy)]
pub struct NefParser;

impl NefParser {
    /// Create a new NEF parser.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Parse a NEF container from a byte buffer.
    ///
    /// # Errors
    /// Returns an error if the input is not a valid NEF container, if the script
    /// exceeds configured limits, or if the checksum does not match.
    pub fn parse(&self, bytes: &[u8]) -> Result<NefFile> {
        if bytes.len() < FIXED_HEADER_SIZE + CHECKSUM_SIZE {
            return Err(NefError::TooShort.into());
        }

        let mut offset = 0usize;

        let magic_slice = bytes
            .get(offset..offset + 4)
            .ok_or(NefError::UnexpectedEof { offset })?;
        let mut magic = [0u8; 4];
        magic.copy_from_slice(magic_slice);
        offset += 4;

        if magic != MAGIC {
            return Err(NefError::InvalidMagic {
                expected: MAGIC,
                actual: magic,
            }
            .into());
        }

        let compiler_start = offset;
        let compiler_end = compiler_start + 64;
        let compiler_bytes =
            bytes
                .get(compiler_start..compiler_end)
                .ok_or(NefError::UnexpectedEof {
                    offset: compiler_start,
                })?;
        offset = compiler_end;

        let compiler_len = compiler_bytes
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(compiler_bytes.len());
        let compiler = std::str::from_utf8(&compiler_bytes[..compiler_len])
            .map_err(|_| NefError::InvalidCompiler)?
            .to_string();

        let (source, source_len) = read_varstring(bytes, offset, MAX_SOURCE_LEN)?;
        offset += source_len;

        let reserved = *bytes
            .get(offset)
            .ok_or(NefError::UnexpectedEof { offset })?;
        if reserved != 0 {
            return Err(NefError::ReservedByteNonZero {
                offset,
                value: reserved,
            }
            .into());
        }
        offset += 1;

        let (method_tokens, tokens_len) = self.parse_method_tokens(bytes, offset)?;
        offset += tokens_len;

        let reserved_word_bytes = bytes
            .get(offset..offset + 2)
            .ok_or(NefError::UnexpectedEof { offset })?;
        let reserved_word = u16::from_le_bytes(reserved_word_bytes.try_into().unwrap());
        if reserved_word != 0 {
            return Err(NefError::ReservedWordNonZero {
                offset,
                value: reserved_word,
            }
            .into());
        }
        offset += 2;

        let (script, script_len) = read_varbytes(bytes, offset, MAX_SCRIPT_LEN)?;
        if script.is_empty() {
            return Err(NefError::EmptyScript.into());
        }
        offset += script_len;

        let checksum_start = offset;
        let checksum_end = checksum_start + 4;
        let checksum_bytes =
            bytes
                .get(checksum_start..checksum_end)
                .ok_or(NefError::UnexpectedEof {
                    offset: checksum_start,
                })?;
        let checksum = u32::from_le_bytes(checksum_bytes.try_into().unwrap());

        let payload = bytes
            .get(..checksum_start)
            .ok_or(NefError::UnexpectedEof { offset: 0 })?;
        let calculated = Self::calculate_checksum(payload);
        if checksum != calculated {
            return Err(NefError::ChecksumMismatch {
                expected: checksum,
                calculated,
            }
            .into());
        }

        offset = checksum_end;
        if offset != bytes.len() {
            return Err(NefError::TrailingData {
                extra: bytes.len() - offset,
            }
            .into());
        }

        Ok(NefFile {
            header: NefHeader {
                magic,
                compiler,
                source,
            },
            method_tokens,
            script,
            checksum,
        })
    }

    /// Calculate the NEF checksum over the payload bytes.
    ///
    /// This implements the double-SHA256 checksum used by the NEF container and
    /// returns the first 4 bytes of the resulting digest as a little-endian
    /// `u32`.
    pub fn calculate_checksum(payload: &[u8]) -> u32 {
        let first = Sha256::digest(payload);
        let second = Sha256::digest(first.as_slice());
        u32::from_le_bytes(second[..4].try_into().unwrap())
    }
}
