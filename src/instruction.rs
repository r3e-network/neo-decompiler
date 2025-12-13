//! Neo VM instruction and operand types.
//!
//! This module defines the [`Instruction`] type returned by the disassembler,
//! along with [`OpCode`] and the supported operand representations.

use std::fmt;

use crate::syscalls;
use crate::util;

/// A decoded Neo VM instruction with its bytecode offset.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Instruction {
    /// Offset of the opcode byte within the original bytecode buffer.
    pub offset: usize,
    /// Decoded opcode.
    pub opcode: OpCode,
    /// Decoded operand, if the opcode carries one.
    pub operand: Option<Operand>,
}

impl Instruction {
    /// Construct an instruction at the given bytecode offset.
    ///
    /// Most users will obtain instructions from [`crate::Disassembler`] or
    /// [`crate::Decompiler`], but this constructor is useful when creating
    /// synthetic instruction streams for tests.
    ///
    /// # Examples
    /// ```
    /// use neo_decompiler::{Instruction, OpCode, Operand};
    ///
    /// let ins = Instruction::new(0, OpCode::Push0, Some(Operand::I32(0)));
    /// assert_eq!(ins.offset, 0);
    /// assert_eq!(ins.opcode, OpCode::Push0);
    /// ```
    pub fn new(offset: usize, opcode: OpCode, operand: Option<Operand>) -> Self {
        Self {
            offset,
            opcode,
            operand,
        }
    }
}

/// Instruction operands supported by the disassembler.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Operand {
    /// Signed 8-bit integer.
    I8(i8),
    /// Signed 16-bit integer.
    I16(i16),
    /// Signed 32-bit integer.
    I32(i32),
    /// Signed 64-bit integer.
    I64(i64),
    /// Raw byte payload (e.g. PUSHDATA/PUSHBYTES).
    Bytes(Vec<u8>),
    /// Signed 8-bit jump offset.
    Jump(i8),
    /// Signed 32-bit jump offset.
    Jump32(i32),
    /// Syscall identifier (little-endian u32).
    Syscall(u32),
    /// Unsigned 8-bit integer.
    U8(u8),
    /// Unsigned 16-bit integer.
    U16(u16),
    /// Unsigned 32-bit integer.
    U32(u32),
    /// Boolean value.
    Bool(bool),
    /// Null literal.
    Null,
}

impl fmt::Display for Operand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Operand::I8(v) => write!(f, "{v}"),
            Operand::I16(v) => write!(f, "{v}"),
            Operand::I32(v) => write!(f, "{v}"),
            Operand::I64(v) => write!(f, "{v}"),
            Operand::Bytes(bytes) => {
                write!(f, "0x")?;
                util::write_upper_hex(f, bytes)
            }
            Operand::Jump(offset) => write!(f, "{offset}"),
            Operand::Jump32(offset) => write!(f, "{offset}"),
            Operand::Syscall(hash) => {
                if let Some(info) = syscalls::lookup(*hash) {
                    write!(f, "{} (0x{hash:08X})", info.name)
                } else {
                    write!(f, "0x{hash:08X}")
                }
            }
            Operand::U8(value) => write!(f, "{value}"),
            Operand::U16(value) => write!(f, "{value}"),
            Operand::U32(value) => write!(f, "{value}"),
            Operand::Bool(value) => write!(f, "{value}"),
            Operand::Null => write!(f, "null"),
        }
    }
}

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

    include!("opcodes_generated.rs");
}

pub use opcodes_generated::OpCode;

impl OpCode {
    /// Return every opcode variant known to the generated table ordered by opcode byte.
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
