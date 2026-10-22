//! Serialization of a parsed NEF container back into its byte form.
//!
//! The parser is the authority on the container layout, and this module
//! mirrors it exactly. A round trip -- parse a container, serialize it
//! again -- must reproduce the original bytes, because a container that
//! changes when it is read and written back cannot be checked, diffed, or
//! reproduced by anything that consumes it.
//!
//! The layout, in order:
//!
//! 1. Magic (4 bytes).
//! 2. Compiler identifier (64 bytes, NUL padded).
//! 3. Source (varstring).
//! 4. Reserved byte (must be zero).
//! 5. Method tokens (varint count, then one entry per token).
//! 6. Reserved word (must be zero).
//! 7. Script (varbytes).
//! 8. Checksum (4 bytes, little endian).

use super::parser::NefParser;
use super::types::NefFile;
use crate::error::NefError;

/// The size of the fixed-width compiler field.
const COMPILER_FIELD_SIZE: usize = 64;

/// Write a variable-length integer using the canonical encoding.
///
/// The parser accepts the non-canonical forms the reference implementation
/// also accepts, but the canonical form is written back so that a round trip
/// is stable.
fn write_varint(buf: &mut Vec<u8>, value: u32) {
    match value {
        0x00..=0xFC => buf.push(value as u8),
        0xFD..=0xFFFF => {
            buf.push(0xFD);
            buf.extend_from_slice(&(value as u16).to_le_bytes());
        }
        _ => {
            buf.push(0xFE);
            buf.extend_from_slice(&value.to_le_bytes());
        }
    }
}

/// Write a length-prefixed byte string.
fn write_varbytes(buf: &mut Vec<u8>, value: &[u8]) {
    write_varint(buf, value.len() as u32);
    buf.extend_from_slice(value);
}

impl NefFile {
    /// Serialize the container back into its byte form.
    ///
    /// The result is a container the parser accepts: the reserved fields are
    /// written as zero, and the checksum is recomputed over the payload.
    ///
    /// The round trip is stable for any container this crate produced. A
    /// container that carried a non-canonical varint, or bytes after the
    /// terminating NUL of the compiler field, is normalized to the canonical
    /// form, so its bytes change even though its meaning does not.
    ///
    /// A container whose script or source is longer than the format allows
    /// is written anyway, but the parser will reject it. Use
    /// [`NefFile::try_to_bytes`] to get the error instead.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        self.try_to_bytes().unwrap_or_else(|_| {
            // The caller asked for the bytes and the container does not fit
            // the format, so the bytes are written without the length
            // check. A container that the parser rejects is still a
            // faithful description of what the caller asked for.
            self.to_bytes_unchecked()
        })
    }

    /// Serialize the container, refusing a container the parser would
    /// reject.
    ///
    /// The parser enforces limits on the script length, the source length,
    /// and the total file size. A container that breaks one of those limits
    /// cannot be read back, so writing it produces bytes that look like a
    /// container and are not one.
    ///
    /// # Errors
    ///
    /// Returns an error when the script or the source is longer than the
    /// format allows, or when the container would be larger than
    /// [`crate::nef::MAX_NEF_FILE_SIZE`].
    pub fn try_to_bytes(&self) -> Result<Vec<u8>, NefError> {
        if self.script.len() > super::MAX_SCRIPT_LEN {
            return Err(NefError::ScriptTooLarge {
                length: self.script.len(),
                max: super::MAX_SCRIPT_LEN,
            });
        }
        if self.header.source.len() > super::MAX_SOURCE_LEN {
            return Err(NefError::SourceTooLong {
                length: self.header.source.len(),
                max: super::MAX_SOURCE_LEN,
            });
        }

        let bytes = self.to_bytes_unchecked();
        if bytes.len() as u64 > super::MAX_NEF_FILE_SIZE {
            return Err(NefError::FileTooLarge {
                size: bytes.len() as u64,
                max: super::MAX_NEF_FILE_SIZE,
            });
        }
        Ok(bytes)
    }

    /// Serialize the container without checking the length limits.
    fn to_bytes_unchecked(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(self.payload_len() + super::CHECKSUM_SIZE);

        // Magic.
        buf.extend_from_slice(&self.header.magic);

        // Compiler: a fixed 64-byte field, NUL padded and truncated at the
        // first NUL so a string that was read back is written the same way.
        let mut compiler = [0u8; COMPILER_FIELD_SIZE];
        let compiler_bytes = self.header.compiler.as_bytes();
        let compiler_len = compiler_bytes.len().min(COMPILER_FIELD_SIZE);
        compiler[..compiler_len].copy_from_slice(&compiler_bytes[..compiler_len]);
        buf.extend_from_slice(&compiler);

        // Source.
        write_varbytes(&mut buf, self.header.source.as_bytes());

        // Reserved byte.
        buf.push(0);

        // Method tokens.
        write_varint(&mut buf, self.method_tokens.len() as u32);
        for token in &self.method_tokens {
            buf.extend_from_slice(&token.hash);
            write_varbytes(&mut buf, token.method.as_bytes());
            buf.extend_from_slice(&token.parameters_count.to_le_bytes());
            buf.push(u8::from(token.has_return_value));
            buf.push(token.call_flags);
        }

        // Reserved word.
        buf.extend_from_slice(&0u16.to_le_bytes());

        // Script.
        write_varbytes(&mut buf, &self.script);

        // Checksum over everything written so far.
        let checksum = NefParser::calculate_checksum(&buf);
        buf.extend_from_slice(&checksum.to_le_bytes());

        buf
    }
}
