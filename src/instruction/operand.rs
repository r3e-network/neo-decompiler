use std::fmt;

use crate::syscalls;
use crate::util;

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
