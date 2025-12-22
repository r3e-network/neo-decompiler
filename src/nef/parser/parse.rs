use crate::error::{NefError, Result};

use super::super::encoding::{read_varbytes, read_varstring};
use super::super::types::{NefFile, NefHeader};
use super::super::{CHECKSUM_SIZE, FIXED_HEADER_SIZE, MAGIC, MAX_SCRIPT_LEN, MAX_SOURCE_LEN};
use super::NefParser;

mod header;
mod reserved;
mod script;
mod trailer;

impl NefParser {
    /// Parse a NEF container from a byte buffer.
    ///
    /// # Errors
    /// Returns an error if the input is not a valid NEF container, if the script
    /// exceeds configured limits, or if the checksum does not match.
    pub fn parse(&self, bytes: &[u8]) -> Result<NefFile> {
        if bytes.len() as u64 > super::super::MAX_NEF_FILE_SIZE {
            return Err(NefError::FileTooLarge {
                size: bytes.len() as u64,
                max: super::super::MAX_NEF_FILE_SIZE,
            }
            .into());
        }
        if bytes.len() < FIXED_HEADER_SIZE + CHECKSUM_SIZE {
            return Err(NefError::TooShort.into());
        }

        let mut offset = 0usize;

        let magic = header::read_magic(bytes, &mut offset)?;
        let compiler = header::read_compiler(bytes, &mut offset)?;
        let source = header::read_source(bytes, &mut offset)?;
        reserved::expect_reserved_byte_zero(bytes, &mut offset)?;

        let (method_tokens, tokens_len) = self.parse_method_tokens(bytes, offset)?;
        offset += tokens_len;

        reserved::expect_reserved_word_zero(bytes, &mut offset)?;
        let script = script::read_script(bytes, &mut offset)?;

        let checksum_start = offset;
        let checksum = trailer::read_checksum(bytes, checksum_start)?;
        trailer::verify_checksum(bytes, checksum_start, checksum)?;
        trailer::expect_end_of_file(bytes, checksum_start + CHECKSUM_SIZE)?;

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
}
