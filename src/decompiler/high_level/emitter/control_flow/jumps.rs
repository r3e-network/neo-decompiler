use crate::instruction::{Instruction, Operand};

use super::super::HighLevelEmitter;

impl HighLevelEmitter {
    pub(in super::super) fn emit_relative(
        &mut self,
        instruction: &Instruction,
        width: isize,
        label: &str,
    ) {
        if self.skip_jumps.remove(&instruction.offset) {
            return;
        }
        if let Some(target) = self.jump_target(instruction, width) {
            self.note(
                instruction,
                &format!("{label} -> 0x{target:04X} (control flow not yet lifted)"),
            );
        } else {
            self.note(
                instruction,
                &format!("{label} with unsupported operand (skipping)"),
            );
        }
    }

    pub(in super::super) fn emit_indirect_call(&mut self, instruction: &Instruction, label: &str) {
        let detail = match instruction.operand {
            Some(Operand::U16(value)) => format!("{label} 0x{value:04X}"),
            _ => format!("{label} (missing operand)"),
        };
        self.note(instruction, &format!("{detail} (not yet translated)"));
    }

    pub(in super::super) fn emit_jump(&mut self, instruction: &Instruction, width: isize) {
        if self.skip_jumps.remove(&instruction.offset) {
            // jump consumed by structured if/else handling
            return;
        }
        match self.jump_target(instruction, width) {
            Some(target) => {
                if self.try_emit_loop_jump(instruction, target) {
                    return;
                }
                self.note(
                    instruction,
                    &format!("jump -> 0x{target:04X} (control flow not yet lifted)"),
                );
            }
            None => self.note(instruction, "jump with unsupported operand (skipping)"),
        }
    }
}
