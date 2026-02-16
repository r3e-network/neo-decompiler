// Bytecode offset arithmetic requires isize↔usize casts for signed jump deltas.
// NEF scripts are bounded (~1 MB), so these conversions are structurally safe.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]

use crate::instruction::{Instruction, OpCode};

use super::super::{DoWhileLoop, HighLevelEmitter, LoopJump};

impl HighLevelEmitter {
    pub(in super::super) fn analyze_do_while_loops(&mut self) {
        for instruction in &self.program {
            if !matches!(
                instruction.opcode,
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
                    | OpCode::JmpLe_L
            ) {
                continue;
            }
            let width = Self::branch_width(instruction.opcode);
            if let Some(target) = self.forward_jump_target(instruction) {
                if target < instruction.offset {
                    let break_offset = instruction.offset as isize + width;
                    if break_offset >= 0 {
                        self.do_while_headers
                            .entry(target)
                            .or_default()
                            .push(DoWhileLoop {
                                tail_offset: instruction.offset,
                                break_offset: break_offset as usize,
                            });
                    }
                }
            }
        }
    }

    pub(in super::super) fn try_emit_do_while_tail(&mut self, instruction: &Instruction) -> bool {
        if !self.active_do_while_tails.remove(&instruction.offset) {
            return false;
        }
        let Some(condition) = self.stack.pop() else {
            self.push_comment(instruction);
            self.stack_underflow(instruction, 1);
            return true;
        };
        self.push_comment(instruction);
        self.statements.push(format!("}} while ({condition});"));
        self.pop_loops_with_continue(instruction.offset);
        true
    }

    /// Like `try_emit_do_while_tail` but for `Jmpifnot` backward jumps.
    /// The jump fires when the condition is FALSE, so the loop continues
    /// while the condition is false → `} while (!condition);`.
    pub(in super::super) fn try_emit_do_while_negated_tail(
        &mut self,
        instruction: &Instruction,
    ) -> bool {
        if !self.active_do_while_tails.remove(&instruction.offset) {
            return false;
        }
        let Some(condition) = self.stack.pop() else {
            self.push_comment(instruction);
            self.stack_underflow(instruction, 1);
            return true;
        };
        self.push_comment(instruction);
        self.statements.push(format!("}} while (!{condition});"));
        self.pop_loops_with_continue(instruction.offset);
        true
    }

    /// Like `try_emit_do_while_tail` but for comparison jumps that pop two
    /// operands.  `original_op` is the UN-negated operator (the jump condition),
    /// e.g. `"<"` for `JmpLt`.  The loop continues while the condition is true.
    pub(in super::super) fn try_emit_do_while_comparison_tail(
        &mut self,
        instruction: &Instruction,
        original_op: &str,
    ) -> bool {
        if !self.active_do_while_tails.remove(&instruction.offset) {
            return false;
        }
        if self.stack.len() < 2 {
            self.push_comment(instruction);
            self.stack_underflow(instruction, 2);
            return true;
        }
        let (Some(right), Some(left)) = (self.stack.pop(), self.stack.pop()) else {
            return true;
        };
        self.push_comment(instruction);
        self.statements
            .push(format!("}} while ({left} {original_op} {right});"));
        self.pop_loops_with_continue(instruction.offset);
        true
    }

    pub(super) fn detect_loop_back(
        &self,
        false_offset: usize,
        condition_offset: usize,
    ) -> Option<LoopJump> {
        let (_, &index) = self.index_by_offset.range(..false_offset).next_back()?;
        let jump_instruction = self.program.get(index)?;
        match jump_instruction.opcode {
            OpCode::Jmp | OpCode::Jmp_L => {
                let target = self.forward_jump_target(jump_instruction)?;
                if target <= condition_offset
                    && !self
                        .loop_stack
                        .iter()
                        .any(|ctx| ctx.continue_offset == target)
                {
                    return Some(LoopJump {
                        jump_offset: jump_instruction.offset,
                        target,
                    });
                }
            }
            _ => {}
        }
        None
    }

    pub(super) fn try_emit_loop_jump(&mut self, instruction: &Instruction, target: usize) -> bool {
        if self
            .loop_stack
            .iter()
            .rev()
            .any(|ctx| ctx.break_offset == target)
        {
            self.push_comment(instruction);
            self.statements.push("break;".into());
            self.stack.clear();
            return true;
        }
        if self
            .loop_stack
            .iter()
            .rev()
            .any(|ctx| ctx.continue_offset == target)
        {
            self.push_comment(instruction);
            self.statements.push("continue;".into());
            self.stack.clear();
            return true;
        }
        false
    }

    pub(in super::super) fn close_loops_at(&mut self, offset: usize) {
        while self
            .loop_stack
            .last()
            .map(|ctx| ctx.break_offset == offset)
            .unwrap_or(false)
        {
            self.loop_stack.pop();
        }
    }

    fn pop_loops_with_continue(&mut self, continue_offset: usize) {
        while self
            .loop_stack
            .last()
            .map(|ctx| ctx.continue_offset == continue_offset)
            .unwrap_or(false)
        {
            self.loop_stack.pop();
        }
    }

    pub(super) fn is_loop_control_target(&self, target: usize) -> bool {
        self.loop_stack
            .iter()
            .any(|ctx| ctx.break_offset == target || ctx.continue_offset == target)
    }
}
