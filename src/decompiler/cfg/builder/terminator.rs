use crate::instruction::OpCode;

use super::super::basic_block::Terminator;
use super::CfgBuilder;

impl<'a> CfgBuilder<'a> {
    pub(super) fn compute_terminator(
        &self,
        _start_index: usize,
        end_index: usize,
        leaders: &[usize],
    ) -> Terminator {
        if end_index == 0 || end_index > self.instructions.len() {
            return Terminator::Unknown;
        }

        let last_instr = &self.instructions[end_index - 1];

        match last_instr.opcode {
            OpCode::Ret => Terminator::Return,
            OpCode::Throw => Terminator::Throw,
            OpCode::Abort | OpCode::Abortmsg => Terminator::Abort,
            OpCode::Endfinally => Terminator::Unknown,

            OpCode::Jmp | OpCode::Jmp_L => {
                if let Some(target) = self.jump_target(end_index - 1, last_instr) {
                    let target_block = self.offset_to_block_id(target, leaders);
                    Terminator::Jump {
                        target: target_block,
                    }
                } else {
                    Terminator::Unknown
                }
            }

            OpCode::Endtry | OpCode::EndtryL => {
                if let Some(target) = self.jump_target(end_index - 1, last_instr) {
                    let continuation = self.offset_to_block_id(target, leaders);
                    Terminator::EndTry { continuation }
                } else {
                    Terminator::Unknown
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
                if let Some(target) = self.jump_target(end_index - 1, last_instr) {
                    let then_block = self.offset_to_block_id(target, leaders);
                    let else_block = self
                        .instructions
                        .get(end_index)
                        .map(|ins| self.offset_to_block_id(ins.offset, leaders))
                        .unwrap_or_else(|| self.offset_to_block_id(self.end_offset(), leaders));
                    Terminator::Branch {
                        then_target: then_block,
                        else_target: else_block,
                    }
                } else {
                    Terminator::Unknown
                }
            }

            OpCode::Try | OpCode::TryL => {
                let body_offset = self
                    .instruction_end_offset(end_index - 1)
                    .unwrap_or_else(|| self.end_offset());
                let body_target = self.offset_to_block_id(body_offset, leaders);

                let (catch_target, finally_target) = self
                    .try_targets(end_index - 1, last_instr)
                    .unwrap_or((None, None));
                let catch_target = catch_target.map(|off| self.offset_to_block_id(off, leaders));
                let finally_target =
                    finally_target.map(|off| self.offset_to_block_id(off, leaders));

                Terminator::TryEntry {
                    body_target,
                    catch_target,
                    finally_target,
                }
            }

            _ => {
                if let Some(next) = self.instructions.get(end_index) {
                    let target_block = self.offset_to_block_id(next.offset, leaders);
                    Terminator::Fallthrough {
                        target: target_block,
                    }
                } else {
                    Terminator::Unknown
                }
            }
        }
    }
}
