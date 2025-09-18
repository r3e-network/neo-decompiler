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
}

/// Errors returned while parsing NEF containers.
#[derive(Debug, Error)]
pub enum NefError {
    #[error("file too short to contain a NEF header")]
    TooShort,

    #[error("invalid magic bytes: expected {expected:?}, got {actual:?}")]
    InvalidMagic { expected: [u8; 4], actual: [u8; 4] },

    #[error("declared script length {declared} exceeds available payload ({available})")]
    ScriptLengthMismatch { declared: usize, available: usize },

    #[error("checksum mismatch: expected {expected:#010x}, calculated {calculated:#010x}")]
    ChecksumMismatch { expected: u32, calculated: u32 },

    #[error("compiler field is not valid UTF-8")]
    InvalidCompiler,

    #[error("unexpected end of data at offset {offset}")]
    UnexpectedEof { offset: usize },

    #[error("invalid method token at index {index}")]
    InvalidMethodToken { index: usize },
}

/// Errors returned during bytecode disassembly.
#[derive(Debug, Error)]
pub enum DisassemblyError {
    #[error("unexpected end of bytecode at offset {offset}")]
    UnexpectedEof { offset: usize },

    #[error("unknown opcode 0x{opcode:02X} at offset {offset}")]
    UnknownOpcode { opcode: u8, offset: usize },
}
