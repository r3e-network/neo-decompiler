// Bytecode offset arithmetic requires isize↔usize casts for signed jump deltas.
// NEF scripts are bounded (~1 MB), so these conversions are structurally safe.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]

use crate::instruction::{Instruction, OpCode, Operand, OperandEncoding};

use super::super::HighLevelEmitter;

impl HighLevelEmitter {
    pub(super) fn forward_jump_target(
        &self,
        instruction: &Instruction,
        width: isize,
    ) -> Option<usize> {
        let target = match instruction.operand {
            Some(Operand::Jump(delta)) => instruction.offset as isize + width + delta as isize,
            Some(Operand::Jump32(delta)) => instruction.offset as isize + width + delta as isize,
            _ => return None,
        };
        if target < 0 {
            return None;
        }
        Some(target as usize)
    }

    pub(super) fn branch_width(opcode: OpCode) -> isize {
        match opcode.operand_encoding() {
            OperandEncoding::Jump8 => 2,
            OperandEncoding::Jump32 => 5,
            _ => 1,
        }
    }

    pub(super) fn jump_target(&self, instruction: &Instruction, width: isize) -> Option<usize> {
        let delta = match instruction.operand {
            Some(Operand::Jump(value)) => value as isize,
            Some(Operand::Jump32(value)) => value as isize,
            _ => return None,
        };
        let target = instruction.offset as isize + width + delta;
        if target < 0 {
            return None;
        }
        Some(target as usize)
    }
}
