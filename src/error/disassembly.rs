use thiserror::Error;

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
