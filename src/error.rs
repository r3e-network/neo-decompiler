use std::io;

use thiserror::Error;

/// Convenient result alias for the library.
pub type Result<T> = std::result::Result<T, Error>;

/// Top level error surfaced by the library APIs.
#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Nef(#[from] NefError),

    #[error(transparent)]
    Disassembly(#[from] DisassemblyError),

    #[error(transparent)]
    Io(#[from] io::Error),

    #[error(transparent)]
    Manifest(#[from] ManifestError),
}

/// Errors returned while parsing NEF containers.
#[derive(Debug, Error)]
pub enum NefError {
    #[error("file too short to contain a NEF header")]
    TooShort,

    #[error("invalid magic bytes: expected {expected:?}, got {actual:?}")]
    InvalidMagic { expected: [u8; 4], actual: [u8; 4] },

    #[error("checksum mismatch: expected {expected:#010x}, calculated {calculated:#010x}")]
    ChecksumMismatch { expected: u32, calculated: u32 },

    #[error("unexpected trailing data after checksum (extra {extra} bytes)")]
    TrailingData { extra: usize },

    #[error("compiler field is not valid UTF-8")]
    InvalidCompiler,

    #[error("reserved byte at offset {offset} must be zero (found {value:#04X})")]
    ReservedByteNonZero { offset: usize, value: u8 },

    #[error("reserved word at offset {offset} must be zero (found {value:#06X})")]
    ReservedWordNonZero { offset: usize, value: u16 },

    #[error("unexpected end of data at offset {offset}")]
    UnexpectedEof { offset: usize },

    #[error("invalid method token at index {index}")]
    InvalidMethodToken { index: usize },

    #[error("varint exceeds supported range at offset {offset}")]
    IntegerOverflow { offset: usize },

    #[error("varstring contains invalid utf-8 at offset {offset}")]
    InvalidUtf8String { offset: usize },

    #[error("script section cannot be empty")]
    EmptyScript,

    #[error("source string exceeds maximum length ({length} > {max})")]
    SourceTooLong { length: usize, max: usize },

    #[error("method token count exceeds maximum ({count} > {max})")]
    TooManyMethodTokens { count: usize, max: usize },

    #[error("script exceeds maximum size ({length} > {max})")]
    ScriptTooLarge { length: usize, max: usize },

    #[error("method token name {name:?} is not permitted")]
    MethodNameInvalid { name: String },

    #[error("method token call flags 0x{flags:02X} contain unsupported bits (allowed mask 0x{allowed:02X})")]
    CallFlagsInvalid { flags: u8, allowed: u8 },
}

/// Errors returned during bytecode disassembly.
#[derive(Debug, Error)]
pub enum DisassemblyError {
    #[error("unexpected end of bytecode at offset {offset}")]
    UnexpectedEof { offset: usize },

    #[error("unknown opcode 0x{opcode:02X} at offset {offset}")]
    UnknownOpcode { opcode: u8, offset: usize },
}

/// Errors returned while parsing Neo manifest files.
#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("failed to read manifest: {0}")]
    Io(#[from] io::Error),

    #[error("manifest json parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("manifest contains invalid utf-8: {error}")]
    InvalidUtf8 { error: String },
}
