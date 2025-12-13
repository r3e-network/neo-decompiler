//! Error types returned by the library.
//!
//! Most public APIs return [`crate::Result`], which uses [`Error`] as the error
//! type. The variants provide access to more specific error categories when
//! needed.

use std::io;

use thiserror::Error;

/// Convenient result alias for the library.
pub type Result<T> = std::result::Result<T, Error>;

/// Top level error surfaced by the library APIs.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// Errors encountered while parsing a NEF container.
    #[error(transparent)]
    Nef(#[from] NefError),

    /// Errors encountered while decoding Neo VM bytecode.
    #[error(transparent)]
    Disassembly(#[from] DisassemblyError),

    /// I/O failures when reading inputs.
    #[error(transparent)]
    Io(#[from] io::Error),

    /// Errors encountered while parsing a contract manifest.
    #[error(transparent)]
    Manifest(#[from] ManifestError),
}

/// Errors returned while parsing NEF containers.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum NefError {
    /// File too short to contain a NEF header and checksum.
    #[error("file too short to contain a NEF header")]
    TooShort,

    /// Magic bytes at the start of the file did not match the NEF format.
    #[error("invalid magic bytes: expected {expected:?}, got {actual:?}")]
    InvalidMagic {
        /// Expected magic sequence.
        expected: [u8; 4],
        /// Actual bytes found in the input.
        actual: [u8; 4],
    },

    /// Checksum mismatch between the stored and calculated values.
    #[error("checksum mismatch: expected {expected:#010x}, calculated {calculated:#010x}")]
    ChecksumMismatch {
        /// Checksum stored in the NEF file.
        expected: u32,
        /// Checksum calculated from the payload.
        calculated: u32,
    },

    /// Trailing bytes were present after the checksum.
    #[error("unexpected trailing data after checksum (extra {extra} bytes)")]
    TrailingData {
        /// Number of extra bytes after the checksum.
        extra: usize,
    },

    /// The compiler field contained invalid UTF-8.
    #[error("compiler field is not valid UTF-8")]
    InvalidCompiler,

    /// Reserved byte fields must be zero according to the NEF spec.
    #[error("reserved byte at offset {offset} must be zero (found {value:#04X})")]
    ReservedByteNonZero {
        /// Byte offset of the reserved field.
        offset: usize,
        /// Non-zero value that was found.
        value: u8,
    },

    /// Reserved word fields must be zero according to the NEF spec.
    #[error("reserved word at offset {offset} must be zero (found {value:#06X})")]
    ReservedWordNonZero {
        /// Byte offset of the reserved field.
        offset: usize,
        /// Non-zero value that was found.
        value: u16,
    },

    /// Input ended unexpectedly while parsing.
    #[error("unexpected end of data at offset {offset}")]
    UnexpectedEof {
        /// Byte offset where parsing expected more data.
        offset: usize,
    },

    /// A method token entry was malformed.
    #[error("invalid method token at index {index}")]
    InvalidMethodToken {
        /// Index of the method token entry.
        index: usize,
    },

    /// A variable-length integer exceeded the supported range.
    #[error("varint exceeds supported range at offset {offset}")]
    IntegerOverflow {
        /// Offset where the oversized integer was read.
        offset: usize,
    },

    /// A variable-length string contained invalid UTF-8.
    #[error("varstring contains invalid utf-8 at offset {offset}")]
    InvalidUtf8String {
        /// Offset where the invalid string was read.
        offset: usize,
    },

    /// Script section cannot be empty.
    #[error("script section cannot be empty")]
    EmptyScript,

    /// Source string exceeded the maximum supported length.
    #[error("source string exceeds maximum length ({length} > {max})")]
    SourceTooLong {
        /// Actual string length.
        length: usize,
        /// Maximum allowed length.
        max: usize,
    },

    /// Method token count exceeded the maximum supported value.
    #[error("method token count exceeds maximum ({count} > {max})")]
    TooManyMethodTokens {
        /// Actual method token count.
        count: usize,
        /// Maximum allowed method token count.
        max: usize,
    },

    /// Script section exceeded the maximum supported size.
    #[error("script exceeds maximum size ({length} > {max})")]
    ScriptTooLarge {
        /// Actual script length in bytes.
        length: usize,
        /// Maximum allowed script length.
        max: usize,
    },

    /// Method token name contained disallowed characters.
    #[error("method token name {name:?} is not permitted")]
    MethodNameInvalid {
        /// The method name that was rejected.
        name: String,
    },

    /// Call flags contained bits outside the allowed set.
    #[error("method token call flags 0x{flags:02X} contain unsupported bits (allowed mask 0x{allowed:02X})")]
    CallFlagsInvalid {
        /// The unsupported flags value.
        flags: u8,
        /// Mask of allowed flag bits.
        allowed: u8,
    },

    /// Input file exceeded the maximum supported size.
    #[error("file size {size} exceeds maximum ({max} bytes)")]
    FileTooLarge {
        /// Actual file size in bytes.
        size: u64,
        /// Maximum allowed file size.
        max: u64,
    },
}

/// Errors returned during bytecode disassembly.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DisassemblyError {
    /// Input ended unexpectedly while decoding an instruction operand.
    #[error("unexpected end of bytecode at offset {offset}")]
    UnexpectedEof {
        /// Offset of the opcode being decoded.
        offset: usize,
    },

    /// An opcode byte was not recognized by the opcode table.
    #[error("unknown opcode 0x{opcode:02X} at offset {offset}")]
    UnknownOpcode {
        /// The raw opcode byte.
        opcode: u8,
        /// Offset where the opcode byte was encountered.
        offset: usize,
    },

    /// A length-prefixed operand exceeded the supported maximum size.
    #[error("operand length {len} exceeds maximum at offset {offset}")]
    OperandTooLarge {
        /// Offset of the opcode being decoded.
        offset: usize,
        /// Requested operand length.
        len: usize,
    },
}

/// Errors returned while parsing Neo manifest files.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ManifestError {
    /// Failed to read the manifest file contents.
    #[error("failed to read manifest: {0}")]
    Io(#[from] io::Error),

    /// The manifest JSON failed to parse.
    #[error("manifest json parse error: {0}")]
    Json(#[from] serde_json::Error),

    /// The manifest bytes were not valid UTF-8.
    #[error("manifest contains invalid utf-8: {error}")]
    InvalidUtf8 {
        /// Stringified UTF-8 decoding error.
        error: String,
    },
}
