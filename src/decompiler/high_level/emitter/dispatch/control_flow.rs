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
            Jmp => self.emit_jump(instruction, 2),
            Jmp_L => self.emit_jump(instruction, 5),
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
            Jmpifnot | Jmpifnot_L => self.emit_if_block(instruction),
            JmpEq | JmpEq_L => self.emit_comparison_if_block(instruction, "=="),
            JmpNe | JmpNe_L => self.emit_comparison_if_block(instruction, "!="),
            JmpGt | JmpGt_L => self.emit_comparison_if_block(instruction, ">"),
            JmpGe | JmpGe_L => self.emit_comparison_if_block(instruction, ">="),
            JmpLt | JmpLt_L => self.emit_comparison_if_block(instruction, "<"),
            JmpLe | JmpLe_L => self.emit_comparison_if_block(instruction, "<="),
            Endtry => self.emit_endtry(instruction, 2),
            EndtryL => self.emit_endtry(instruction, 5),
            Call => self.emit_relative_call(instruction, 2),
            Call_L => self.emit_relative_call(instruction, 5),
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
