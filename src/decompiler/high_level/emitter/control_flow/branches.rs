// Bytecode offset arithmetic requires isize↔usize casts for signed jump deltas.
// NEF scripts are bounded (~1 MB), so these conversions are structurally safe.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]

use crate::instruction::OpCode;
use crate::instruction::{Instruction, Operand};

use super::super::{HighLevelEmitter, LoopContext};

impl HighLevelEmitter {
    fn is_conditional_branch(opcode: OpCode) -> bool {
        matches!(
            opcode,
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
        )
    }

    fn has_internal_crossing_branch(&self, branch_offset: usize, false_target: usize) -> bool {
        let start_index = self
            .index_by_offset
            .range((branch_offset + 1)..false_target)
            .next()
            .map(|(_, index)| *index);
        let Some(start_index) = start_index else {
            return false;
        };
        let end_index = self
            .index_by_offset
            .range(false_target..)
            .next()
            .map(|(_, index)| *index)
            .unwrap_or(self.program.len());

        self.program[start_index..end_index].iter().any(|inner| {
            Self::is_conditional_branch(inner.opcode)
                && self
                    .forward_jump_target(inner)
                    .map(|target| target > false_target)
                    .unwrap_or(false)
        })
    }

    fn has_crossing_closer(&self, target: usize) -> bool {
        self.pending_closers
            .keys()
            .next()
            .map(|next_close| target > *next_close)
            .unwrap_or(false)
    }

    fn emit_conditional_goto(&mut self, instruction: &Instruction, condition: &str, target: usize) {
        self.push_comment(instruction);
        if self.index_by_offset.contains_key(&target) {
            self.transfer_labels.insert(target);
        }
        self.statements.push(format!(
            "if {condition} {{ goto {}; }}",
            Self::transfer_label_name(target)
        ));
    }

    pub(in super::super) fn emit_comparison_if_block(
        &mut self,
        instruction: &Instruction,
        symbol: &str,
    ) {
        let delta = match instruction.operand {
            Some(Operand::Jump(value)) => value as isize,
            Some(Operand::Jump32(value)) => value as isize,
            _ => {
                self.emit_relative(instruction, &format!("jump-if-{symbol}"));
                return;
            }
        };
        // Neo VM: target = opcode_offset + delta (offset is relative to instruction start).
        let target = instruction.offset as isize + delta;
        if target <= instruction.offset as isize {
            self.emit_relative(instruction, &format!("jump-if-{symbol}"));
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
        // Save stack state so it can be restored when the if-body closes.
        // This handles cases where the if-body terminates (throw/return/abort)
        // and clears the stack — the code after the if-block still needs the
        // pre-branch stack state.
        self.branch_saved_stacks
            .entry(false_target)
            .or_insert_with(|| self.stack.clone());
        let closer_entry = self.pending_closers.entry(false_target).or_insert(0);
        *closer_entry += 1;

        if let Some((jump_offset, jump_target)) = self.detect_else(false_target) {
            if !self.is_loop_control_target(jump_target)
                && !self.else_targets.contains_key(&false_target)
            {
                self.skip_jumps.insert(jump_offset);
                let else_entry = self.else_targets.entry(false_target).or_insert(0);
                *else_entry += 1;
                let closer = self.pending_closers.entry(jump_target).or_insert(0);
                *closer += 1;
                // Record pre-branch stack depth at the merge offset so that
                // merge-time logic can detect branch-produced stack values.
                self.pre_branch_stack_depth
                    .entry(jump_target)
                    .or_insert(self.stack.len());
            }
        } else if let Some(else_end) = self.detect_implicit_else(false_target) {
            if !self.else_targets.contains_key(&false_target) {
                let else_entry = self.else_targets.entry(false_target).or_insert(0);
                *else_entry += 1;
                let closer = self.pending_closers.entry(else_end).or_insert(0);
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
        let delta = match instruction.operand {
            Some(Operand::Jump(value)) => value as isize,
            Some(Operand::Jump32(value)) => value as isize,
            _ => {
                self.emit_relative(instruction, fallback_label);
                return;
            }
        };
        // Neo VM: target = opcode_offset + delta (offset is relative to instruction start).
        let target = instruction.offset as isize + delta;
        if target <= instruction.offset as isize {
            self.emit_relative(instruction, fallback_label);
            return;
        }

        let raw_condition = match self.stack.pop() {
            Some(value) => value,
            None => {
                self.push_comment(instruction);
                self.stack_underflow(instruction, 1);
                return;
            }
        };
        let mut condition = raw_condition.clone();
        if negate_condition {
            condition = format!("!{condition}");
        }

        let false_target = target as usize;
        if self.has_crossing_closer(false_target)
            || self.has_internal_crossing_branch(instruction.offset, false_target)
        {
            let jump_condition = if negate_condition {
                raw_condition
            } else {
                format!("!{raw_condition}")
            };
            self.emit_conditional_goto(instruction, &jump_condition, false_target);
            return;
        }

        self.push_comment(instruction);
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
        // Save stack state so it can be restored when the if/while-body closes.
        self.branch_saved_stacks
            .entry(false_target)
            .or_insert_with(|| self.stack.clone());
        let closer_entry = self.pending_closers.entry(false_target).or_insert(0);
        *closer_entry += 1;

        if loop_jump.is_none() {
            if let Some((jump_offset, jump_target)) = self.detect_else(false_target) {
                if !self.is_loop_control_target(jump_target)
                    && !self.else_targets.contains_key(&false_target)
                {
                    self.skip_jumps.insert(jump_offset);
                    let else_entry = self.else_targets.entry(false_target).or_insert(0);
                    *else_entry += 1;
                    let closer = self.pending_closers.entry(jump_target).or_insert(0);
                    *closer += 1;
                    // Record pre-branch stack depth at the merge offset so that
                    // merge-time logic can detect branch-produced stack values.
                    self.pre_branch_stack_depth
                        .entry(jump_target)
                        .or_insert(self.stack.len());
                }
            } else if let Some(else_end) = self.detect_implicit_else(false_target) {
                if !self.else_targets.contains_key(&false_target) {
                    let else_entry = self.else_targets.entry(false_target).or_insert(0);
                    *else_entry += 1;
                    let closer = self.pending_closers.entry(else_end).or_insert(0);
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
        // Only unconditional JMP/JMP_L indicate an else branch.  Other opcodes
        // that share the Jump8/Jump32 operand encoding (CALL, ENDTRY, etc.)
        // must not be mistaken for else jumps — doing so nests catch/finally
        // blocks inside spurious else branches.
        if !matches!(jump.opcode, OpCode::Jmp | OpCode::Jmp_L) {
            return None;
        }
        let target = self.forward_jump_target(jump)?;
        if target > false_offset {
            Some((jump.offset, target))
        } else {
            None
        }
    }

    /// Detect an implicit else branch when the if-true body ends with a
    /// noreturn instruction.  The Neo C# compiler omits the JMP before the
    /// false-target when the if-true branch always terminates (ABORT, ABORTMSG,
    /// THROW, or CALL to a known noreturn method).
    ///
    /// Returns `Some(else_end_offset)` — the offset where the else closer `}`
    /// should be placed.  The caller must NOT insert a `skip_jumps` entry
    /// (there is no JMP to skip).
    fn detect_implicit_else(&self, false_offset: usize) -> Option<usize> {
        let target_index = *self.index_by_offset.get(&false_offset)?;
        if target_index == 0 {
            return None;
        }
        let prev = self.program.get(target_index.checked_sub(1)?)?;

        // Check if the if-true body ends with a direct terminator.
        let is_direct_terminator = matches!(
            prev.opcode,
            OpCode::Abort | OpCode::Abortmsg | OpCode::Throw
        );

        // Check if the if-true body ends with a CALL to a known noreturn method.
        let is_noreturn_call = matches!(prev.opcode, OpCode::Call | OpCode::Call_L)
            && self
                .call_targets_by_offset
                .get(&prev.offset)
                .map_or(false, |target| {
                    self.noreturn_method_offsets.contains(target)
                });

        if !is_direct_terminator && !is_noreturn_call {
            return None;
        }

        // Don't create an else around catch/finally handlers.
        if self.catch_targets.contains_key(&false_offset)
            || self.finally_targets.contains_key(&false_offset)
        {
            return None;
        }

        // Find the else body end: the next structural boundary (catch/finally
        // target) after false_offset, or the offset past the last instruction.
        let next_boundary = self
            .catch_targets
            .keys()
            .chain(self.finally_targets.keys())
            .filter(|&&offset| offset > false_offset)
            .min()
            .copied();

        let fallback = self.program.last().map(|ins| ins.offset + 1);
        let else_end = next_boundary.unwrap_or_else(|| fallback.unwrap_or(false_offset + 1));

        // Don't create an else when the body would be empty — i.e. every
        // instruction between false_offset (inclusive) and else_end (exclusive)
        // is purely structural control flow (ENDTRY, NOP, unconditional JMP).
        let end_index = self
            .index_by_offset
            .range(else_end..)
            .next()
            .map(|(_, idx)| *idx)
            .unwrap_or(self.program.len());
        let body_has_substance = self.program[target_index..end_index].iter().any(|ins| {
            !matches!(
                ins.opcode,
                OpCode::Endtry
                    | OpCode::EndtryL
                    | OpCode::Nop
                    | OpCode::Jmp
                    | OpCode::Jmp_L
            )
        });
        if !body_has_substance {
            return None;
        }

        Some(else_end)
    }
}
