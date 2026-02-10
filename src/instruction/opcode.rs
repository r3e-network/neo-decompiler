use std::fmt;

/// Metadata describing how to decode operands for a specific opcode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperandEncoding {
    /// No operand bytes.
    None,
    /// Signed 8-bit integer.
    I8,
    /// Signed 16-bit integer.
    I16,
    /// Signed 32-bit integer.
    I32,
    /// Signed 64-bit integer.
    I64,
    /// Fixed-length byte payload.
    Bytes(usize),
    /// One-byte length prefix followed by payload bytes.
    Data1,
    /// Two-byte length prefix followed by payload bytes.
    Data2,
    /// Four-byte length prefix followed by payload bytes.
    Data4,
    /// Signed 8-bit relative jump.
    Jump8,
    /// Signed 32-bit relative jump.
    Jump32,
    /// Unsigned 8-bit integer.
    U8,
    /// Unsigned 16-bit integer.
    U16,
    /// Unsigned 32-bit integer.
    U32,
    /// Syscall identifier.
    Syscall,
}

#[allow(missing_docs)]
mod opcodes_generated {
    use super::OperandEncoding;

    include!("../opcodes_generated.rs");
}

pub use opcodes_generated::OpCode;

impl OpCode {
    /// Return every opcode variant known to the generated table ordered by opcode byte.
    #[must_use]
    pub fn all_known() -> Vec<OpCode> {
        let mut entries = Vec::new();
        for byte in u8::MIN..=u8::MAX {
            let opcode = OpCode::from_byte(byte);
            if matches!(opcode, OpCode::Unknown(_)) {
                continue;
            }
            if !entries.contains(&opcode) {
                entries.push(opcode);
            }
        }
        entries
    }
}

impl fmt::Display for OpCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpCode::Unknown(value) => write!(f, "UNKNOWN_0x{value:02X}"),
            _ => f.write_str(self.mnemonic()),
        }
    }
}
