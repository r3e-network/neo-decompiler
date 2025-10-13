use std::fmt;

use crate::syscalls;

/// A decoded Neo VM instruction with its bytecode offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instruction {
    pub offset: usize,
    pub opcode: OpCode,
    pub operand: Option<Operand>,
}

impl Instruction {
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
pub enum Operand {
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    Bytes(Vec<u8>),
    Jump(i8),
    Jump32(i32),
    Syscall(u32),
    U8(u8),
    U16(u16),
    U32(u32),
    Bool(bool),
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
                for byte in bytes {
                    write!(f, "{byte:02X}")?;
                }
                Ok(())
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
    None,
    I8,
    I16,
    I32,
    I64,
    Bytes(usize),
    Data1,
    Data2,
    Data4,
    Jump8,
    Jump32,
    U8,
    U16,
    U32,
    Syscall,
}

include!("opcodes_generated.rs");

impl fmt::Display for OpCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.mnemonic())
    }
}
