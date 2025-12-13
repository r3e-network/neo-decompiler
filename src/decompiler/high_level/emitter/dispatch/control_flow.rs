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
                    self.emit_relative(instruction, 2, "jump-if");
                }
            }
            Jmpif_L => {
                if !self.try_emit_do_while_tail(instruction) {
                    self.emit_relative(instruction, 5, "jump-if");
                }
            }
            Jmpifnot | Jmpifnot_L => self.emit_if_block(instruction),
            JmpEq => self.emit_relative(instruction, 2, "jump-if-eq"),
            JmpEq_L => self.emit_relative(instruction, 5, "jump-if-eq"),
            JmpNe => self.emit_relative(instruction, 2, "jump-if-ne"),
            JmpNe_L => self.emit_relative(instruction, 5, "jump-if-ne"),
            JmpGt => self.emit_relative(instruction, 2, "jump-if-gt"),
            JmpGt_L => self.emit_relative(instruction, 5, "jump-if-gt"),
            JmpGe => self.emit_relative(instruction, 2, "jump-if-ge"),
            JmpGe_L => self.emit_relative(instruction, 5, "jump-if-ge"),
            JmpLt => self.emit_relative(instruction, 2, "jump-if-lt"),
            JmpLt_L => self.emit_relative(instruction, 5, "jump-if-lt"),
            JmpLe => self.emit_relative(instruction, 2, "jump-if-le"),
            JmpLe_L => self.emit_relative(instruction, 5, "jump-if-le"),
            Endtry => self.emit_relative(instruction, 2, "end-try"),
            EndtryL => self.emit_relative(instruction, 5, "end-try"),
            Call => self.emit_relative(instruction, 2, "call"),
            Call_L => self.emit_relative(instruction, 5, "call"),
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
