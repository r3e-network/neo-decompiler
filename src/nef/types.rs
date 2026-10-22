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
    /// Build a container from its parts.
    ///
    /// The struct is `#[non_exhaustive]`, so a caller outside the crate
    /// cannot write the struct literal. This is the way to build one: the
    /// parts are named, and the checksum is left to the serializer, which
    /// computes it over the payload.
    ///
    /// The magic defaults to the NEF3 marker, because that is the only
    /// container the parser accepts.
    #[must_use]
    pub fn new(script: Vec<u8>) -> Self {
        Self {
            header: NefHeader {
                magic: *b"NEF3",
                compiler: String::new(),
                source: String::new(),
            },
            method_tokens: Vec::new(),
            script,
            checksum: 0,
        }
    }

    /// Set the compiler identifier of the container.
    ///
    /// The field is a fixed 64-byte slot in the container, so a longer
    /// value is truncated when the container is written.
    #[must_use]
    pub fn with_compiler(mut self, compiler: impl Into<String>) -> Self {
        self.header.compiler = compiler.into();
        self
    }

    /// Set the source string of the container.
    #[must_use]
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.header.source = source.into();
        self
    }

    /// Length of the payload included in the checksum calculation.
    #[must_use]
    pub fn payload_len(&self) -> usize {
        let fixed_header_len = super::FIXED_HEADER_SIZE; // magic + fixed 64-byte compiler
        let source_bytes = self.header.source.as_bytes();
        let source_len = varint_encoded_len(source_bytes.len() as u32) + source_bytes.len();
        let tokens_len = encoded_method_tokens_size(&self.method_tokens);
        let script_len = varint_encoded_len(self.script.len() as u32) + self.script.len();

        fixed_header_len + source_len + 1 + tokens_len + 2 + script_len
    }

    /// Hash160 of the script (little-endian) as used for the contract script hash.
    #[must_use]
    pub fn script_hash(&self) -> [u8; 20] {
        util::hash160(&self.script)
    }

    /// Hash160 of the script (little-endian) as used for the contract script hash.
    #[must_use]
    #[deprecated(
        since = "0.5.1",
        note = "use `script_hash()` instead, which is identical"
    )]
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
