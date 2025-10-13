use crate::error::{NefError, Result};

use sha2::{Digest, Sha256};

const HEADER_SIZE: usize = 44;
const CHECKSUM_SIZE: usize = 4;
const MAGIC: [u8; 4] = *b"NEF3";

/// Parsed NEF header information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NefHeader {
    pub magic: [u8; 4],
    pub compiler: String,
    pub version: u32,
    pub script_length: u32,
}

/// Method token entry present in the NEF container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodToken {
    pub hash: [u8; 20],
    pub method: String,
    pub params: u8,
    pub return_type: u8,
    pub call_flags: u8,
}

/// Parsed NEF container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NefFile {
    pub header: NefHeader,
    pub method_tokens: Vec<MethodToken>,
    pub script: Vec<u8>,
    pub checksum: u32,
}

impl NefFile {
    /// Length of the payload included in the checksum calculation.
    pub fn payload_len(&self) -> usize {
        HEADER_SIZE + encoded_method_tokens_size(&self.method_tokens) + self.script.len()
    }
}

/// Parser for the Neo N3 NEF format.
#[derive(Debug, Default, Clone, Copy)]
pub struct NefParser;

impl NefParser {
    pub fn new() -> Self {
        Self
    }

    /// Parse a NEF file from raw bytes.
    pub fn parse(&self, bytes: &[u8]) -> Result<NefFile> {
        if bytes.len() < HEADER_SIZE + CHECKSUM_SIZE {
            return Err(NefError::TooShort.into());
        }

        let mut offset = 0usize;

        let magic: [u8; 4] = bytes[offset..offset + 4]
            .try_into()
            .expect("slice with correct length");
        if magic != MAGIC {
            return Err(NefError::InvalidMagic {
                expected: MAGIC,
                actual: magic,
            }
            .into());
        }
        offset += 4;

        let compiler_raw = &bytes[offset..offset + 32];
        let compiler_len = compiler_raw
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(compiler_raw.len());
        let compiler = std::str::from_utf8(&compiler_raw[..compiler_len])
            .map_err(|_| NefError::InvalidCompiler)?
            .trim()
            .to_string();
        offset += 32;

        let version = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        offset += 4;

        let script_length =
            u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
        offset += 4;

        let (method_tokens, consumed) = self.parse_method_tokens(bytes, offset)?;
        offset += consumed;

        let script_end = offset + script_length;
        if script_end + CHECKSUM_SIZE > bytes.len() {
            return Err(NefError::ScriptLengthMismatch {
                declared: script_length,
                available: bytes.len().saturating_sub(offset + CHECKSUM_SIZE),
            }
            .into());
        }

        let script = bytes[offset..script_end].to_vec();
        offset = script_end;

        let checksum_bytes = bytes[offset..offset + CHECKSUM_SIZE]
            .try_into()
            .expect("checksum slice");
        let checksum = u32::from_le_bytes(checksum_bytes);

        let calculated = Self::calculate_checksum(&bytes[..offset]);
        if checksum != calculated {
            return Err(NefError::ChecksumMismatch {
                expected: checksum,
                calculated,
            }
            .into());
        }

        Ok(NefFile {
            header: NefHeader {
                magic,
                compiler,
                version,
                script_length: script_length as u32,
            },
            method_tokens,
            script,
            checksum,
        })
    }

    fn parse_method_tokens(
        &self,
        bytes: &[u8],
        mut offset: usize,
    ) -> Result<(Vec<MethodToken>, usize)> {
        let start = offset;
        let (count, varint_len) = read_varint(bytes, offset)?;
        offset += varint_len;

        let mut tokens = Vec::with_capacity(count as usize);
        for index in 0..count as usize {
            let hash_start = offset;
            let hash_end = hash_start + 20;
            let hash_slice = bytes
                .get(hash_start..hash_end)
                .ok_or(NefError::UnexpectedEof { offset: hash_start })?;
            let mut hash = [0u8; 20];
            hash.copy_from_slice(hash_slice);
            offset = hash_end;

            let (method_len, method_varint) = read_varint(bytes, offset)?;
            offset += method_varint;
            let method_end = offset + method_len as usize;
            let method_bytes = bytes
                .get(offset..method_end)
                .ok_or(NefError::UnexpectedEof { offset })?;
            let method = std::str::from_utf8(method_bytes)
                .map_err(|_| NefError::InvalidMethodToken { index })?
                .to_string();
            offset = method_end;

            let params = *bytes
                .get(offset)
                .ok_or(NefError::UnexpectedEof { offset })?;
            offset += 1;

            let return_type = *bytes
                .get(offset)
                .ok_or(NefError::UnexpectedEof { offset })?;
            offset += 1;

            let call_flags = *bytes
                .get(offset)
                .ok_or(NefError::UnexpectedEof { offset })?;
            offset += 1;

            tokens.push(MethodToken {
                hash,
                method,
                params,
                return_type,
                call_flags,
            });
        }

        Ok((tokens, offset - start))
    }

    /// Calculate the checksum defined by the NEF specification (first four bytes of double SHA-256).
    pub fn calculate_checksum(payload: &[u8]) -> u32 {
        let first = Sha256::digest(payload);
        let second = Sha256::digest(first);
        u32::from_le_bytes(second[..4].try_into().unwrap())
    }
}

fn read_varint(bytes: &[u8], offset: usize) -> Result<(u32, usize)> {
    let first = *bytes
        .get(offset)
        .ok_or(NefError::UnexpectedEof { offset })?;
    match first {
        0x00..=0xFC => Ok((first as u32, 1)),
        0xFD => {
            let slice = bytes
                .get(offset + 1..offset + 3)
                .ok_or(NefError::UnexpectedEof { offset })?;
            let value = u16::from_le_bytes(slice.try_into().unwrap());
            Ok((value as u32, 3))
        }
        0xFE => {
            let slice = bytes
                .get(offset + 1..offset + 5)
                .ok_or(NefError::UnexpectedEof { offset })?;
            let value = u32::from_le_bytes(slice.try_into().unwrap());
            Ok((value, 5))
        }
        0xFF => Err(NefError::InvalidMethodToken { index: usize::MAX }.into()),
    }
}

fn encoded_method_tokens_size(tokens: &[MethodToken]) -> usize {
    let mut size = varint_encoded_len(tokens.len() as u32);
    for token in tokens {
        size += 20; // hash
        size += varint_encoded_len(token.method.len() as u32);
        size += token.method.len();
        size += 3; // params + return type + call flags
    }
    size
}

fn varint_encoded_len(value: u32) -> usize {
    match value {
        0x00..=0xFC => 1,
        0xFD..=0xFFFF => 3,
        _ => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_sample(payload_script: &[u8]) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&MAGIC);
        let mut compiler = [0u8; 32];
        let name = b"neo-sample";
        compiler[..name.len()].copy_from_slice(name);
        data.extend_from_slice(&compiler);
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&(payload_script.len() as u32).to_le_bytes());

        // Method tokens: empty set
        data.push(0); // varint-encoded zero

        data.extend_from_slice(payload_script);
        let checksum = NefParser::calculate_checksum(&data);
        data.extend_from_slice(&checksum.to_le_bytes());
        data
    }

    #[test]
    fn parses_valid_nef() {
        let script = vec![0x10, 0x11, 0x40];
        let bytes = build_sample(&script);
        let nef = NefParser::new().parse(&bytes).expect("parse succeeds");

        assert_eq!(nef.header.magic, MAGIC);
        assert_eq!(nef.header.compiler, "neo-sample");
        assert_eq!(nef.script, script);
        assert!(nef.method_tokens.is_empty());
        assert_eq!(nef.header.script_length, 3);
    }

    #[test]
    fn rejects_bad_magic() {
        let mut bytes = build_sample(&[0x40]);
        bytes[0] = b'X';
        let err = NefParser::new().parse(&bytes).unwrap_err();
        assert!(matches!(
            err,
            crate::error::Error::Nef(NefError::InvalidMagic { .. })
        ));
    }

    #[test]
    fn rejects_bad_checksum() {
        let mut bytes = build_sample(&[0x40]);
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;
        let err = NefParser::new().parse(&bytes).unwrap_err();
        assert!(matches!(
            err,
            crate::error::Error::Nef(NefError::ChecksumMismatch { .. })
        ));
    }

    #[test]
    fn parses_method_tokens() {
        let script = vec![0x40];
        let mut data = Vec::new();
        data.extend_from_slice(&MAGIC);
        let mut compiler = [0u8; 32];
        compiler[..4].copy_from_slice(b"test");
        data.extend_from_slice(&compiler);
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&(script.len() as u32).to_le_bytes());

        // one method token
        data.push(1);
        // hash
        data.extend_from_slice(&[0x11; 20]);
        // method name length (3) as varint
        data.push(3);
        data.extend_from_slice(b"foo");
        // params
        data.push(2);
        // return type
        data.push(0x21);
        // call flags
        data.push(0x0F);

        data.extend_from_slice(&script);
        let checksum = NefParser::calculate_checksum(&data);
        data.extend_from_slice(&checksum.to_le_bytes());

        let nef = NefParser::new().parse(&data).expect("parse succeeds");
        assert_eq!(nef.method_tokens.len(), 1);
        let token = &nef.method_tokens[0];
        assert_eq!(token.method, "foo");
        assert_eq!(token.params, 2);
        assert_eq!(token.return_type, 0x21);
        assert_eq!(token.call_flags, 0x0F);
    }
}
