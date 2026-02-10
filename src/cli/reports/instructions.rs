use serde::Serialize;

use crate::instruction::{Instruction, OpCode, Operand};
use crate::util;

#[derive(Serialize)]
pub(in crate::cli) struct InstructionReport {
    offset: usize,
    opcode: String,
    operand: Option<String>,
    operand_kind: Option<String>,
    operand_value: Option<OperandValueReport>,
    returns_value: Option<bool>,
}

impl From<&Instruction> for InstructionReport {
    fn from(instruction: &Instruction) -> Self {
        InstructionReport {
            offset: instruction.offset,
            opcode: instruction.opcode.mnemonic().to_string(),
            operand: instruction.operand.as_ref().map(ToString::to_string),
            operand_kind: instruction
                .operand
                .as_ref()
                .map(|op| operand_kind_name(op).to_string()),
            operand_value: instruction.operand.as_ref().map(operand_value_report),
            returns_value: returns_value_for_instruction(instruction),
        }
    }
}

fn operand_kind_name(operand: &Operand) -> &'static str {
    match operand {
        Operand::I8(_) => "I8",
        Operand::I16(_) => "I16",
        Operand::I32(_) => "I32",
        Operand::I64(_) => "I64",
        Operand::Bytes(_) => "Bytes",
        Operand::Jump(_) => "Jump8",
        Operand::Jump32(_) => "Jump32",
        Operand::Syscall(_) => "Syscall",
        Operand::U8(_) => "U8",
        Operand::U16(_) => "U16",
        Operand::U32(_) => "U32",
        Operand::Bool(_) => "Bool",
        Operand::Null => "Null",
    }
}

#[derive(Serialize)]
#[serde(tag = "type", content = "value")]
pub(in crate::cli) enum OperandValueReport {
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    U8(u8),
    U16(u16),
    U32(u32),
    Bool(bool),
    Bytes(String),
    Jump(i32),
    Jump32(i32),
    Syscall(u32),
    Null,
}

fn operand_value_report(operand: &Operand) -> OperandValueReport {
    match operand {
        Operand::I8(value) => OperandValueReport::I8(*value),
        Operand::I16(value) => OperandValueReport::I16(*value),
        Operand::I32(value) => OperandValueReport::I32(*value),
        Operand::I64(value) => OperandValueReport::I64(*value),
        Operand::U8(value) => OperandValueReport::U8(*value),
        Operand::U16(value) => OperandValueReport::U16(*value),
        Operand::U32(value) => OperandValueReport::U32(*value),
        Operand::Bool(value) => OperandValueReport::Bool(*value),
        Operand::Jump(value) => OperandValueReport::Jump(*value as i32),
        Operand::Jump32(value) => OperandValueReport::Jump32(*value),
        Operand::Syscall(value) => OperandValueReport::Syscall(*value),
        Operand::Bytes(bytes) => {
            OperandValueReport::Bytes(format!("0x{}", util::upper_hex_string(bytes)))
        }
        Operand::Null => OperandValueReport::Null,
    }
}

fn returns_value_for_instruction(instruction: &Instruction) -> Option<bool> {
    if let OpCode::Syscall = instruction.opcode {
        if let Some(Operand::Syscall(hash)) = instruction.operand {
            return crate::syscalls::lookup(hash).map(|info| info.returns_value);
        }
    }
    None
}
