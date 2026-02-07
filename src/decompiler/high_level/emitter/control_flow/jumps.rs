use crate::instruction::{Instruction, Operand};

use super::super::HighLevelEmitter;

impl HighLevelEmitter {
    pub(in super::super) fn emit_relative_call(&mut self, instruction: &Instruction, width: isize) {
        self.push_comment(instruction);
        if let Some(target) = self.jump_target(instruction, width) {
            // Target method signatures are not available here, so model the
            // call as a conservative placeholder that returns a value.
            let temp = self.next_temp();
            self.statements
                .push(format!("let {temp} = call_0x{target:04X}();"));
            self.stack.push(temp);
        } else {
            self.warn(instruction, "call with unsupported operand (skipping)");
        }
    }

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
            self.warn(
                instruction,
                &format!("{label} -> 0x{target:04X} (control flow not yet lifted)"),
            );
        } else {
            self.warn(
                instruction,
                &format!("{label} with unsupported operand (skipping)"),
            );
        }
    }

    pub(in super::super) fn emit_indirect_call(&mut self, instruction: &Instruction, label: &str) {
        self.push_comment(instruction);
        match instruction.operand {
            Some(Operand::U16(value)) => {
                // The callee ABI is unknown at this stage, so conservatively
                // assume a return value to avoid cascading stack underflow.
                let temp = self.next_temp();
                self.statements
                    .push(format!("let {temp} = {label}(0x{value:04X});"));
                self.stack.push(temp);
            }
            _ => self.warn(instruction, &format!("{label} (missing operand)")),
        }
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
                self.push_comment(instruction);
                if self.index_by_offset.contains_key(&target) {
                    self.transfer_labels.insert(target);
                }
                self.statements
                    .push(format!("goto {};", Self::transfer_label_name(target)));
            }
            None => self.warn(instruction, "jump with unsupported operand (skipping)"),
        }
    }

    pub(in super::super) fn emit_endtry(&mut self, instruction: &Instruction, width: isize) {
        if self.skip_jumps.remove(&instruction.offset) {
            return;
        }

        self.push_comment(instruction);
        match self.jump_target(instruction, width) {
            Some(target) => {
                if self.index_by_offset.contains_key(&target) {
                    self.transfer_labels.insert(target);
                }
                self.statements
                    .push(format!("leave {};", Self::transfer_label_name(target)));
            }
            None => self.warn(instruction, "end-try with unsupported operand (skipping)"),
        }
    }
}
