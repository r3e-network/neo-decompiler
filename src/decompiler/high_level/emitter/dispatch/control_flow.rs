use crate::instruction::{Instruction, OpCode};

use super::super::HighLevelEmitter;

impl HighLevelEmitter {
    pub(super) fn try_emit_control_flow(&mut self, instruction: &Instruction) -> bool {
        use OpCode::*;

        match instruction.opcode {
            Ret => self.emit_return(instruction),
            Abort => {
                self.emit_call(instruction, "abort", 0, false);
                self.stack.clear();
            }
            Assert => self.emit_call(instruction, "assert", 1, false),
            Throw => {
                self.emit_call(instruction, "throw", 1, false);
                self.stack.clear();
            }
            Abortmsg => {
                self.emit_call(instruction, "abort", 1, false);
                self.stack.clear();
            }
            Assertmsg => self.emit_call(instruction, "assert", 2, false),
            Syscall => self.emit_syscall(instruction),
            Jmp => self.emit_jump(instruction),
            Jmp_L => self.emit_jump(instruction),
            Jmpif => {
                if !self.try_emit_do_while_tail(instruction) {
                    self.emit_jmpif_block(instruction);
                }
            }
            Jmpif_L => {
                if !self.try_emit_do_while_tail(instruction) {
                    self.emit_jmpif_block(instruction);
                }
            }
            Jmpifnot | Jmpifnot_L => {
                if !self.try_emit_do_while_negated_tail(instruction) {
                    self.emit_if_block(instruction);
                }
            }
            // Comparison jumps branch when the condition is TRUE, so the
            // fall-through (if-body) executes when the condition is FALSE.
            // We therefore pass the NEGATED operator to emit_comparison_if_block.
            // For backward jumps registered as do-while tails, we use the
            // ORIGINAL operator since the loop continues while the condition
            // is true.
            JmpEq | JmpEq_L => {
                if !self.try_emit_do_while_comparison_tail(instruction, "==") {
                    self.emit_comparison_if_block(instruction, "!=");
                }
            }
            JmpNe | JmpNe_L => {
                if !self.try_emit_do_while_comparison_tail(instruction, "!=") {
                    self.emit_comparison_if_block(instruction, "==");
                }
            }
            JmpGt | JmpGt_L => {
                if !self.try_emit_do_while_comparison_tail(instruction, ">") {
                    self.emit_comparison_if_block(instruction, "<=");
                }
            }
            JmpGe | JmpGe_L => {
                if !self.try_emit_do_while_comparison_tail(instruction, ">=") {
                    self.emit_comparison_if_block(instruction, "<");
                }
            }
            JmpLt | JmpLt_L => {
                if !self.try_emit_do_while_comparison_tail(instruction, "<") {
                    self.emit_comparison_if_block(instruction, ">=");
                }
            }
            JmpLe | JmpLe_L => {
                if !self.try_emit_do_while_comparison_tail(instruction, "<=") {
                    self.emit_comparison_if_block(instruction, ">");
                }
            }
            Endtry => self.emit_endtry(instruction),
            EndtryL => self.emit_endtry(instruction),
            Call => self.emit_relative_call(instruction),
            Call_L => self.emit_relative_call(instruction),
            CallA => self.emit_indirect_call(instruction, "calla"),
            CallT => self.emit_indirect_call(instruction, "callt"),
            Try | TryL => self.emit_try_block(instruction),
            Endfinally => {
                if !self.skip_jumps.remove(&instruction.offset) {
                    self.note(instruction, "endfinally");
                }
            }
            Nop => self.note(instruction, "noop"),
            _ => return false,
        }

        true
    }
}
