// Bytecode offset arithmetic requires isize↔usize casts for signed jump deltas.
// NEF scripts are bounded (~1 MB), so these conversions are structurally safe.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]

use crate::instruction::{Instruction, Operand};

use super::super::{HighLevelEmitter, LoopContext};

impl HighLevelEmitter {
    pub(in super::super) fn emit_comparison_if_block(
        &mut self,
        instruction: &Instruction,
        symbol: &str,
    ) {
        let width = Self::branch_width(instruction.opcode);
        let delta = match instruction.operand {
            Some(Operand::Jump(value)) => value as isize,
            Some(Operand::Jump32(value)) => value as isize,
            _ => {
                self.emit_relative(instruction, width, &format!("jump-if-{symbol}"));
                return;
            }
        };
        let target = instruction.offset as isize + width + delta;
        if target <= instruction.offset as isize {
            self.emit_relative(instruction, width, &format!("jump-if-{symbol}"));
            return;
        }

        if self.stack.len() < 2 {
            self.push_comment(instruction);
            self.stack_underflow(instruction, 2);
            return;
        }

        let (Some(right), Some(left)) = (self.stack.pop(), self.stack.pop()) else {
            return;
        };
        let condition = format!("{left} {symbol} {right}");

        self.push_comment(instruction);
        self.statements.push(format!("if {condition} {{"));

        let false_target = target as usize;
        let closer_entry = self.pending_closers.entry(false_target).or_insert(0);
        *closer_entry += 1;

        if let Some((jump_offset, jump_target)) = self.detect_else(false_target) {
            if !self.is_loop_control_target(jump_target) {
                self.skip_jumps.insert(jump_offset);
                let else_entry = self.else_targets.entry(false_target).or_insert(0);
                *else_entry += 1;
                let closer = self.pending_closers.entry(jump_target).or_insert(0);
                *closer += 1;
            }
        }
    }

    pub(in super::super) fn emit_if_block(&mut self, instruction: &Instruction) {
        self.emit_unary_if_block(instruction, false, "jump-ifnot");
    }

    pub(in super::super) fn emit_jmpif_block(&mut self, instruction: &Instruction) {
        self.emit_unary_if_block(instruction, true, "jump-if");
    }

    fn emit_unary_if_block(
        &mut self,
        instruction: &Instruction,
        negate_condition: bool,
        fallback_label: &str,
    ) {
        let width = Self::branch_width(instruction.opcode);
        let delta = match instruction.operand {
            Some(Operand::Jump(value)) => value as isize,
            Some(Operand::Jump32(value)) => value as isize,
            _ => {
                self.emit_relative(instruction, width, fallback_label);
                return;
            }
        };
        let target = instruction.offset as isize + width + delta;
        if target <= instruction.offset as isize {
            self.emit_relative(instruction, width, fallback_label);
            return;
        }

        let mut condition = match self.stack.pop() {
            Some(value) => value,
            None => {
                self.push_comment(instruction);
                self.stack_underflow(instruction, 1);
                return;
            }
        };
        if negate_condition {
            condition = format!("!{condition}");
        }

        self.push_comment(instruction);
        let false_target = target as usize;
        let loop_jump = self.detect_loop_back(false_target, instruction.offset);
        if let Some(loop_jump) = loop_jump.as_ref() {
            self.statements.push(format!("while {condition} {{"));
            self.skip_jumps.insert(loop_jump.jump_offset);
            self.loop_stack.push(LoopContext {
                break_offset: false_target,
                continue_offset: loop_jump.target,
            });
        } else {
            self.statements.push(format!("if {condition} {{"));
        }
        let closer_entry = self.pending_closers.entry(false_target).or_insert(0);
        *closer_entry += 1;

        if loop_jump.is_none() {
            if let Some((jump_offset, jump_target)) = self.detect_else(false_target) {
                if !self.is_loop_control_target(jump_target) {
                    self.skip_jumps.insert(jump_offset);
                    let else_entry = self.else_targets.entry(false_target).or_insert(0);
                    *else_entry += 1;
                    let closer = self.pending_closers.entry(jump_target).or_insert(0);
                    *closer += 1;
                }
            }
        }
    }

    fn detect_else(&self, false_offset: usize) -> Option<(usize, usize)> {
        let target_index = *self.index_by_offset.get(&false_offset)?;
        if target_index == 0 {
            return None;
        }
        let jump = self.program.get(target_index.checked_sub(1)?)?;
        let width = Self::branch_width(jump.opcode);
        let target = self.forward_jump_target(jump, width)?;
        if target > false_offset {
            Some((jump.offset, target))
        } else {
            None
        }
    }
}
