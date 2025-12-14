use crate::instruction::{OpCode, Operand};

pub(super) fn immediate_constant(opcode: OpCode) -> Option<Operand> {
    match opcode {
        OpCode::PushM1 => Some(Operand::I32(-1)),
        OpCode::Push0 => Some(Operand::I32(0)),
        OpCode::Push1 => Some(Operand::I32(1)),
        OpCode::Push2 => Some(Operand::I32(2)),
        OpCode::Push3 => Some(Operand::I32(3)),
        OpCode::Push4 => Some(Operand::I32(4)),
        OpCode::Push5 => Some(Operand::I32(5)),
        OpCode::Push6 => Some(Operand::I32(6)),
        OpCode::Push7 => Some(Operand::I32(7)),
        OpCode::Push8 => Some(Operand::I32(8)),
        OpCode::Push9 => Some(Operand::I32(9)),
        OpCode::Push10 => Some(Operand::I32(10)),
        OpCode::Push11 => Some(Operand::I32(11)),
        OpCode::Push12 => Some(Operand::I32(12)),
        OpCode::Push13 => Some(Operand::I32(13)),
        OpCode::Push14 => Some(Operand::I32(14)),
        OpCode::Push15 => Some(Operand::I32(15)),
        OpCode::Push16 => Some(Operand::I32(16)),
        OpCode::PushT => Some(Operand::Bool(true)),
        OpCode::PushF => Some(Operand::Bool(false)),
        OpCode::PushNull => Some(Operand::Null),
        _ => None,
    }
}
