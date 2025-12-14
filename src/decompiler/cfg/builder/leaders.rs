use crate::instruction::OpCode;

use super::CfgBuilder;

impl<'a> CfgBuilder<'a> {
    pub(super) fn find_leaders(&mut self) {
        if let Some(first) = self.instructions.first() {
            self.leaders.insert(first.offset);
        }

        for (i, instr) in self.instructions.iter().enumerate() {
            match instr.opcode {
                OpCode::Jmp | OpCode::Jmp_L | OpCode::Endtry | OpCode::EndtryL => {
                    if let Some(target) = self.jump_target(i, instr) {
                        self.leaders.insert(target);
                    }
                    if let Some(next) = self.instructions.get(i + 1) {
                        self.leaders.insert(next.offset);
                    }
                }

                OpCode::Jmpif
                | OpCode::Jmpif_L
                | OpCode::Jmpifnot
                | OpCode::Jmpifnot_L
                | OpCode::JmpEq
                | OpCode::JmpEq_L
                | OpCode::JmpNe
                | OpCode::JmpNe_L
                | OpCode::JmpGt
                | OpCode::JmpGt_L
                | OpCode::JmpGe
                | OpCode::JmpGe_L
                | OpCode::JmpLt
                | OpCode::JmpLt_L
                | OpCode::JmpLe
                | OpCode::JmpLe_L => {
                    if let Some(target) = self.jump_target(i, instr) {
                        self.leaders.insert(target);
                    }
                    if let Some(next) = self.instructions.get(i + 1) {
                        self.leaders.insert(next.offset);
                    }
                }

                OpCode::Try | OpCode::TryL => {
                    if let Some((catch_off, finally_off)) = self.try_targets(i, instr) {
                        if let Some(c) = catch_off {
                            self.leaders.insert(c);
                        }
                        if let Some(f) = finally_off {
                            self.leaders.insert(f);
                        }
                    }
                    if let Some(next) = self.instructions.get(i + 1) {
                        self.leaders.insert(next.offset);
                    }
                }

                OpCode::Ret
                | OpCode::Throw
                | OpCode::Abort
                | OpCode::Abortmsg
                | OpCode::Endfinally => {
                    if let Some(next) = self.instructions.get(i + 1) {
                        self.leaders.insert(next.offset);
                    }
                }

                OpCode::Call | OpCode::Call_L | OpCode::CallA | OpCode::Syscall => {}

                _ => {}
            }
        }
    }
}
