use crate::error::{NefError, Result};
use crate::util;

use sha2::{Digest, Sha256};

const FIXED_HEADER_SIZE: usize = 68;
const CHECKSUM_SIZE: usize = 4;
const MAGIC: [u8; 4] = *b"NEF3";
const MAX_SOURCE_LEN: usize = 256;
const MAX_METHOD_TOKENS: usize = 128;
const MAX_SCRIPT_LEN: usize = 1_048_576; // ExecutionEngineLimits.Default.MaxItemSize
const CALL_FLAG_READ_STATES: u8 = 0x01;
const CALL_FLAG_WRITE_STATES: u8 = 0x02;
const CALL_FLAG_ALLOW_CALL: u8 = 0x04;
const CALL_FLAG_ALLOW_NOTIFY: u8 = 0x08;
const CALL_FLAGS_ALLOWED_MASK: u8 =
    CALL_FLAG_READ_STATES | CALL_FLAG_WRITE_STATES | CALL_FLAG_ALLOW_CALL | CALL_FLAG_ALLOW_NOTIFY;

/// Parsed NEF header information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NefHeader {
    pub magic: [u8; 4],
    pub compiler: String,
    pub source: String,
}

/// Method token entry present in the NEF container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodToken {
    pub hash: [u8; 20],
    pub method: String,
    pub parameters_count: u16,
    pub has_return_value: bool,
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
        let fixed_header_len = FIXED_HEADER_SIZE; // magic + fixed 64-byte compiler
        let source_bytes = self.header.source.as_bytes();
        let source_len = varint_encoded_len(source_bytes.len() as u32) + source_bytes.len();
        let tokens_len = encoded_method_tokens_size(&self.method_tokens);
        let script_len = varint_encoded_len(self.script.len() as u32) + self.script.len();

        fixed_header_len + source_len + 1 + tokens_len + 2 + script_len
    }

    /// Hash160 of the script (little-endian) as used for the contract script hash.
    pub fn script_hash(&self) -> [u8; 20] {
        util::hash160(&self.script)
    }

    /// Script hash in big-endian form (for explorer comparisons).
    pub fn script_hash_be(&self) -> [u8; 20] {
        let mut hash = self.script_hash();
        hash.reverse();
        hash
    }
}

/// Return the individual call flag labels set on the provided mask.
pub fn call_flag_labels(flags: u8) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if flags & CALL_FLAG_READ_STATES != 0 {
        labels.push("ReadStates");
    }
    if flags & CALL_FLAG_WRITE_STATES != 0 {
        labels.push("WriteStates");
    }
    if flags & CALL_FLAG_ALLOW_CALL != 0 {
        labels.push("AllowCall");
    }
    if flags & CALL_FLAG_ALLOW_NOTIFY != 0 {
        labels.push("AllowNotify");
    }
    labels
}

/// Return a human-readable list of call flag names for displaying method tokens.
pub fn describe_call_flags(flags: u8) -> String {
    if flags == 0 {
        return "None".into();
    }
    call_flag_labels(flags).join("|")
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
        if bytes.len() < FIXED_HEADER_SIZE + CHECKSUM_SIZE {
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

        let compiler_raw = &bytes[offset..offset + 64];
        let compiler_len = compiler_raw
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(compiler_raw.len());
        let compiler = std::str::from_utf8(&compiler_raw[..compiler_len])
            .map_err(|_| NefError::InvalidCompiler)?
            .to_string();
        offset += 64;

        let (source, consumed) = read_varstring(bytes, offset)?;
        offset += consumed;
        if source.len() > MAX_SOURCE_LEN {
            return Err(NefError::SourceTooLong {
                length: source.len(),
                max: MAX_SOURCE_LEN,
            }
            .into());
        }

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

        let (method_tokens, consumed) = self.parse_method_tokens(bytes, offset)?;
        offset += consumed;

        let reserved_u16_bytes = bytes
            .get(offset..offset + 2)
            .ok_or(NefError::UnexpectedEof { offset })?;
        let reserved_u16 = u16::from_le_bytes(reserved_u16_bytes.try_into().unwrap());
        if reserved_u16 != 0 {
            return Err(NefError::ReservedWordNonZero {
                offset,
                value: reserved_u16,
            }
            .into());
        }
        offset += 2;

        let (script, consumed) = read_varbytes(bytes, offset)?;
        offset += consumed;
        let script_len = script.len();
        if script_len == 0 {
            return Err(NefError::EmptyScript.into());
        }
        if script_len > MAX_SCRIPT_LEN {
            return Err(NefError::ScriptTooLarge {
                length: script_len,
                max: MAX_SCRIPT_LEN,
            }
            .into());
        }

        let payload_end = offset;
        let checksum_bytes = bytes[offset..offset + CHECKSUM_SIZE]
            .try_into()
            .expect("checksum slice");
        let checksum = u32::from_le_bytes(checksum_bytes);

        let calculated = Self::calculate_checksum(&bytes[..payload_end]);
        if checksum != calculated {
            return Err(NefError::ChecksumMismatch {
                expected: checksum,
                calculated,
            }
            .into());
        }

        offset = payload_end + CHECKSUM_SIZE;
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

    fn parse_method_tokens(
        &self,
        bytes: &[u8],
        mut offset: usize,
    ) -> Result<(Vec<MethodToken>, usize)> {
        let start = offset;
        let (count, varint_len) = read_varint(bytes, offset)?;
        offset += varint_len;

        if count as usize > MAX_METHOD_TOKENS {
            return Err(NefError::TooManyMethodTokens {
                count: count as usize,
                max: MAX_METHOD_TOKENS,
            }
            .into());
        }

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
            if method.starts_with('_') {
                return Err(NefError::MethodNameInvalid { name: method }.into());
            }
            offset = method_end;

            let params_bytes = bytes
                .get(offset..offset + 2)
                .ok_or(NefError::UnexpectedEof { offset })?;
            let params = u16::from_le_bytes(params_bytes.try_into().unwrap());
            offset += 2;

            let has_return_value = match bytes.get(offset) {
                Some(0) => {
                    offset += 1;
                    false
                }
                Some(1) => {
                    offset += 1;
                    true
                }
                Some(_) => {
                    return Err(NefError::InvalidMethodToken { index }.into());
                }
                None => return Err(NefError::UnexpectedEof { offset }.into()),
            };

            let call_flags = *bytes
                .get(offset)
                .ok_or(NefError::UnexpectedEof { offset })?;
            offset += 1;
            if call_flags & !CALL_FLAGS_ALLOWED_MASK != 0 {
                return Err(NefError::CallFlagsInvalid {
                    flags: call_flags,
                    allowed: CALL_FLAGS_ALLOWED_MASK,
                }
                .into());
            }

            tokens.push(MethodToken {
                hash,
                method,
                parameters_count: params,
                has_return_value,
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
        0xFF => {
            let slice = bytes
                .get(offset + 1..offset + 9)
                .ok_or(NefError::UnexpectedEof { offset })?;
            let value = u64::from_le_bytes(slice.try_into().unwrap());
            if value > u32::MAX as u64 {
                return Err(NefError::IntegerOverflow { offset }.into());
            }
            Ok((value as u32, 9))
        }
    }
}

fn encoded_method_tokens_size(tokens: &[MethodToken]) -> usize {
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

fn varint_encoded_len(value: u32) -> usize {
    match value {
        0x00..=0xFC => 1,
        0xFD..=0xFFFF => 3,
        _ => 5,
    }
}

fn read_varstring(bytes: &[u8], offset: usize) -> Result<(String, usize)> {
    let (len, consumed) = read_varint(bytes, offset)?;
    let start = offset + consumed;
    let end = start + len as usize;
    let slice = bytes
        .get(start..end)
        .ok_or(NefError::UnexpectedEof { offset: start })?;
    let value = std::str::from_utf8(slice)
        .map_err(|_| NefError::InvalidUtf8String { offset: start })?
        .to_string();
    Ok((value, consumed + slice.len()))
}

fn read_varbytes(bytes: &[u8], offset: usize) -> Result<(Vec<u8>, usize)> {
    let (len, consumed) = read_varint(bytes, offset)?;
    let start = offset + consumed;
    let end = start + len as usize;
    let slice = bytes
        .get(start..end)
        .ok_or(NefError::UnexpectedEof { offset: start })?;
    Ok((slice.to_vec(), consumed + slice.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn build_sample(payload_script: &[u8]) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&MAGIC);
        let mut compiler = [0u8; 64];
        let name = b"neo-sample";
        compiler[..name.len()].copy_from_slice(name);
        data.extend_from_slice(&compiler);
        // source (empty string)
        data.push(0);
        // reserved byte
        data.push(0);
        // method tokens: empty set
        data.push(0);
        // reserved word
        data.extend_from_slice(&0u16.to_le_bytes());
        // script
        write_varint(&mut data, payload_script.len() as u32);
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
        assert!(nef.header.source.is_empty());
        assert_eq!(nef.script, script);
        assert!(nef.method_tokens.is_empty());
        assert_eq!(
            util::format_hash(&nef.script_hash()),
            util::format_hash(&util::hash160(&[0x10, 0x11, 0x40]))
        );
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
        let mut compiler = [0u8; 64];
        compiler[..4].copy_from_slice(b"test");
        data.extend_from_slice(&compiler);
        // source (empty)
        data.push(0);
        // reserved byte
        data.push(0);

        // one method token
        data.push(1); // count
        data.extend_from_slice(&[0x11; 20]);
        write_varint(&mut data, 3);
        data.extend_from_slice(b"foo");
        // params
        data.extend_from_slice(&2u16.to_le_bytes());
        // return flag (true)
        data.push(1);
        // call flags (0x0F)
        data.push(0x0F);

        // reserved word
        data.extend_from_slice(&0u16.to_le_bytes());
        // script
        write_varint(&mut data, script.len() as u32);
        data.extend_from_slice(&script);

        let checksum = NefParser::calculate_checksum(&data);
        data.extend_from_slice(&checksum.to_le_bytes());

        let nef = NefParser::new().parse(&data).expect("parse succeeds");
        assert_eq!(nef.method_tokens.len(), 1);
        let token = &nef.method_tokens[0];
        assert_eq!(token.method, "foo");
        assert_eq!(token.parameters_count, 2);
        assert!(token.has_return_value);
        assert_eq!(token.call_flags, 0x0F);
    }

    #[test]
    fn rejects_method_name_with_leading_underscore() {
        let script = vec![0x40];
        let mut data = Vec::new();
        data.extend_from_slice(&MAGIC);
        data.extend_from_slice(&[0u8; 64]);
        data.push(0); // source
        data.push(0); // reserved
        data.push(1); // one token
        data.extend_from_slice(&[0x22; 20]);
        write_varint(&mut data, 2);
        data.extend_from_slice(b"_x");
        data.extend_from_slice(&0u16.to_le_bytes());
        data.push(0); // no return
        data.push(0x10); // call flags (AllowNotify)
        data.extend_from_slice(&0u16.to_le_bytes());
        write_varint(&mut data, script.len() as u32);
        data.extend_from_slice(&script);
        let checksum = NefParser::calculate_checksum(&data);
        data.extend_from_slice(&checksum.to_le_bytes());

        let err = NefParser::new().parse(&data).unwrap_err();
        assert!(matches!(
            err,
            crate::error::Error::Nef(NefError::MethodNameInvalid { .. })
        ));
    }

    #[test]
    fn rejects_call_flags_with_unsupported_bits() {
        let script = vec![0x40];
        let mut data = Vec::new();
        data.extend_from_slice(&MAGIC);
        data.extend_from_slice(&[0u8; 64]);
        data.push(0); // source
        data.push(0); // reserved
        data.push(1); // one token
        data.extend_from_slice(&[0x33; 20]);
        write_varint(&mut data, 3);
        data.extend_from_slice(b"foo");
        data.extend_from_slice(&0u16.to_le_bytes());
        data.push(0); // no return
        data.push(0x80); // unsupported flag bit
        data.extend_from_slice(&0u16.to_le_bytes());
        write_varint(&mut data, script.len() as u32);
        data.extend_from_slice(&script);
        let checksum = NefParser::calculate_checksum(&data);
        data.extend_from_slice(&checksum.to_le_bytes());

        let err = NefParser::new().parse(&data).unwrap_err();
        assert!(matches!(
            err,
            crate::error::Error::Nef(NefError::CallFlagsInvalid { .. })
        ));
    }

    #[test]
    fn rejects_source_too_long() {
        let script = vec![0x40];
        let mut data = Vec::new();
        data.extend_from_slice(&MAGIC);
        data.extend_from_slice(&[0u8; 64]);
        let long_source = "a".repeat(MAX_SOURCE_LEN + 1);
        write_varint(&mut data, long_source.len() as u32);
        data.extend_from_slice(long_source.as_bytes());
        data.push(0); // reserved byte
        data.push(0); // zero tokens
        data.extend_from_slice(&0u16.to_le_bytes());
        write_varint(&mut data, script.len() as u32);
        data.extend_from_slice(&script);
        let checksum = NefParser::calculate_checksum(&data);
        data.extend_from_slice(&checksum.to_le_bytes());

        let err = NefParser::new().parse(&data).unwrap_err();
        assert!(matches!(
            err,
            crate::error::Error::Nef(NefError::SourceTooLong { .. })
        ));
    }

    #[test]
    fn rejects_too_many_method_tokens() {
        let script = vec![0x40];
        let mut data = Vec::new();
        data.extend_from_slice(&MAGIC);
        data.extend_from_slice(&[0u8; 64]);
        data.push(0); // source
        data.push(0); // reserved
                      // declare more than allowed tokens
        write_varint(&mut data, (MAX_METHOD_TOKENS + 1) as u32);
        // no token payload needed because parser should error on count alone
        data.extend_from_slice(&0u16.to_le_bytes());
        write_varint(&mut data, script.len() as u32);
        data.extend_from_slice(&script);
        let checksum = NefParser::calculate_checksum(&data);
        data.extend_from_slice(&checksum.to_le_bytes());

        let err = NefParser::new().parse(&data).unwrap_err();
        assert!(matches!(
            err,
            crate::error::Error::Nef(NefError::TooManyMethodTokens { .. })
        ));
    }

    #[test]
    fn rejects_script_too_large() {
        let script = vec![0u8; MAX_SCRIPT_LEN + 1];
        let mut data = Vec::new();
        data.extend_from_slice(&MAGIC);
        data.extend_from_slice(&[0u8; 64]);
        data.push(0); // source
        data.push(0); // reserved
        data.push(0); // zero tokens
        data.extend_from_slice(&0u16.to_le_bytes());
        write_varint(&mut data, script.len() as u32);
        data.extend_from_slice(&script);
        let checksum = NefParser::calculate_checksum(&data);
        data.extend_from_slice(&checksum.to_le_bytes());

        let err = NefParser::new().parse(&data).unwrap_err();
        assert!(matches!(
            err,
            crate::error::Error::Nef(NefError::ScriptTooLarge { .. })
        ));
    }

    #[test]
    fn rejects_trailing_bytes() {
        let script = vec![0x40];
        let bytes = build_sample(&script);
        let mut with_extra = bytes.clone();
        with_extra.push(0x99);

        let err = NefParser::new().parse(&with_extra).unwrap_err();
        assert!(matches!(
            err,
            crate::error::Error::Nef(NefError::TrailingData { extra: 1 })
        ));
    }

    #[test]
    fn describes_call_flags() {
        assert_eq!(describe_call_flags(0x00), "None");
        assert_eq!(describe_call_flags(CALL_FLAG_READ_STATES), "ReadStates");
        assert_eq!(
            describe_call_flags(CALL_FLAG_READ_STATES | CALL_FLAG_ALLOW_CALL),
            "ReadStates|AllowCall"
        );
        assert_eq!(
            describe_call_flags(CALL_FLAGS_ALLOWED_MASK),
            "ReadStates|WriteStates|AllowCall|AllowNotify"
        );
    }

    #[test]
    fn call_flag_labels_report_individual_bits() {
        let labels = call_flag_labels(CALL_FLAG_READ_STATES | CALL_FLAG_ALLOW_NOTIFY);
        assert_eq!(labels, vec!["ReadStates", "AllowNotify"]);
    }
}
