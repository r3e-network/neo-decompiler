use std::fmt;

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

/// Neo VM opcodes covered by this project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OpCode {
    PushInt8,
    PushInt16,
    PushInt32,
    PushInt64,
    PushInt128,
    PushInt256,
    PushData1,
    PushData2,
    PushData4,
    PushM1,
    Push0,
    Push1,
    Nop,
    Add,
    Sub,
    Mul,
    Div,
    Ret,
    Syscall,
    Jump,
    JumpLong,
    JumpIf,
    JumpIfLong,
    Call,
    CallLong,
    CallA,
    CallT,
    Unknown(u8),
}

impl OpCode {
    /// Map a raw opcode byte into the supported enum.
    pub fn from_byte(byte: u8) -> Self {
        match byte {
            0x00 => OpCode::PushInt8,
            0x01 => OpCode::PushInt16,
            0x02 => OpCode::PushInt32,
            0x03 => OpCode::PushInt64,
            0x04 => OpCode::PushInt128,
            0x05 => OpCode::PushInt256,
            0x0C => OpCode::PushData1,
            0x0D => OpCode::PushData2,
            0x0E => OpCode::PushData4,
            0x0F => OpCode::PushM1,
            0x10 => OpCode::Push0,
            0x11 => OpCode::Push1,
            0x21 => OpCode::Nop,
            0x22 => OpCode::Jump,
            0x23 => OpCode::JumpLong,
            0x24 => OpCode::JumpIf,
            0x25 => OpCode::JumpIfLong,
            0x34 => OpCode::Call,
            0x35 => OpCode::CallLong,
            0x36 => OpCode::CallA,
            0x37 => OpCode::CallT,
            0x40 => OpCode::Ret,
            0x41 => OpCode::Syscall,
            0x9E => OpCode::Add,
            0x9F => OpCode::Sub,
            0xA0 => OpCode::Mul,
            0xA1 => OpCode::Div,
            _ => OpCode::Unknown(byte),
        }
    }

    /// Human-readable mnemonic.
    pub fn mnemonic(self) -> String {
        match self {
            OpCode::PushInt8 => "PUSHINT8".into(),
            OpCode::PushInt16 => "PUSHINT16".into(),
            OpCode::PushInt32 => "PUSHINT32".into(),
            OpCode::PushInt64 => "PUSHINT64".into(),
            OpCode::PushInt128 => "PUSHINT128".into(),
            OpCode::PushInt256 => "PUSHINT256".into(),
            OpCode::PushData1 => "PUSHDATA1".into(),
            OpCode::PushData2 => "PUSHDATA2".into(),
            OpCode::PushData4 => "PUSHDATA4".into(),
            OpCode::PushM1 => "PUSHM1".into(),
            OpCode::Push0 => "PUSH0".into(),
            OpCode::Push1 => "PUSH1".into(),
            OpCode::Nop => "NOP".into(),
            OpCode::Add => "ADD".into(),
            OpCode::Sub => "SUB".into(),
            OpCode::Mul => "MUL".into(),
            OpCode::Div => "DIV".into(),
            OpCode::Ret => "RET".into(),
            OpCode::Syscall => "SYSCALL".into(),
            OpCode::Jump => "JMP".into(),
            OpCode::JumpLong => "JMP_L".into(),
            OpCode::JumpIf => "JMPIF".into(),
            OpCode::JumpIfLong => "JMPIF_L".into(),
            OpCode::Call => "CALL".into(),
            OpCode::CallLong => "CALL_L".into(),
            OpCode::CallA => "CALLA".into(),
            OpCode::CallT => "CALLT".into(),
            OpCode::Unknown(byte) => format!("UNKNOWN_0x{byte:02X}"),
        }
    }
}

impl fmt::Display for OpCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.mnemonic())
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
    U16(u16),
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
            Operand::Syscall(hash) => write!(f, "0x{hash:08X}"),
            Operand::U16(value) => write!(f, "{value}"),
        }
    }
}
