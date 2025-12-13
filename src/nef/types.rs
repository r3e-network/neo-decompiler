use crate::util;

use super::encoding::{encoded_method_tokens_size, varint_encoded_len};

/// Parsed NEF header information.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct NefHeader {
    /// Magic bytes at the start of the NEF container.
    pub magic: [u8; 4],
    /// Compiler identifier string (often includes tool name/version).
    pub compiler: String,
    /// Optional source string embedded in the container.
    pub source: String,
}

/// Method token entry present in the NEF container.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct MethodToken {
    /// Script hash (little-endian bytes) for the called contract.
    pub hash: [u8; 20],
    /// Target method name.
    pub method: String,
    /// Declared parameter count.
    pub parameters_count: u16,
    /// Whether the method has a return value.
    pub has_return_value: bool,
    /// Call flags bitfield.
    pub call_flags: u8,
}

/// Parsed NEF container.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct NefFile {
    /// Header section.
    pub header: NefHeader,
    /// Method token table.
    pub method_tokens: Vec<MethodToken>,
    /// Script bytecode.
    pub script: Vec<u8>,
    /// Checksum stored in the container.
    pub checksum: u32,
}

impl NefFile {
    /// Length of the payload included in the checksum calculation.
    pub fn payload_len(&self) -> usize {
        let fixed_header_len = super::FIXED_HEADER_SIZE; // magic + fixed 64-byte compiler
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

    /// Hash160 of the script (little-endian) as used for the contract script hash.
    #[must_use]
    pub fn script_hash_le(&self) -> [u8; 20] {
        self.script_hash()
    }

    /// Script hash in big-endian form (for explorer comparisons).
    #[must_use]
    pub fn script_hash_be(&self) -> [u8; 20] {
        let mut hash = self.script_hash();
        hash.reverse();
        hash
    }
}
