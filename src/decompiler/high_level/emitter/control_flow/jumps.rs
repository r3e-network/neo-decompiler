use crate::instruction::{Instruction, Operand};

use super::super::{HighLevelEmitter, LiteralValue};

const MAX_INTERNAL_CALL_ENTRY_DELTA: usize = 16;

impl HighLevelEmitter {
    fn normalize_internal_call_target(&self, target: usize) -> usize {
        if self.method_labels_by_offset.contains_key(&target)
            || self.method_arg_counts_by_offset.contains_key(&target)
        {
            return target;
        }

        if let Some((candidate, _)) = self
            .method_arg_counts_by_offset
            .range(..=target)
            .next_back()
        {
            if target.saturating_sub(*candidate) <= MAX_INTERNAL_CALL_ENTRY_DELTA
                && self
                    .method_labels_by_offset
                    .get(candidate)
                    .map(|label| label.as_str() != "script_entry")
                    .unwrap_or(false)
            {
                return *candidate;
            }
        }

        if let Some((candidate, label)) = self.method_labels_by_offset.range(..=target).next_back()
        {
            if target.saturating_sub(*candidate) <= MAX_INTERNAL_CALL_ENTRY_DELTA
                && label.as_str() != "script_entry"
            {
                return *candidate;
            }
        }

        target
    }

    fn resolve_internal_call_name(&self, target: usize) -> Option<&str> {
        self.method_labels_by_offset
            .get(&target)
            .map(std::string::String::as_str)
    }

    fn emit_internal_call(&mut self, instruction: &Instruction, target: usize) {
        let mut target = self.normalize_internal_call_target(target);
        if let Some(required_args) = self.method_arg_counts_by_offset.get(&target).copied() {
            if self.stack.len() < required_args {
                if let Some((candidate, _)) =
                    self.method_arg_counts_by_offset.range(..target).rev().find(
                        |(candidate, candidate_args)| {
                            target.saturating_sub(**candidate) <= MAX_INTERNAL_CALL_ENTRY_DELTA
                                && **candidate_args <= self.stack.len()
                                && self
                                    .method_labels_by_offset
                                    .get(*candidate)
                                    .map(|label| label.as_str() != "script_entry")
                                    .unwrap_or(false)
                        },
                    )
                {
                    target = *candidate;
                }
            }
        }
        let callee = self
            .resolve_internal_call_name(target)
            .map(str::to_string)
            .unwrap_or_else(|| format!("call_0x{target:04X}"));
        if let Some(arg_count) = self.method_arg_counts_by_offset.get(&target).copied() {
            self.push_comment(instruction);
            if self.stack.len() < arg_count {
                self.stack_underflow(instruction, arg_count);
                return;
            }
            let mut args = Vec::with_capacity(arg_count);
            for _ in 0..arg_count {
                if let Some(value) = self.pop_stack_value() {
                    // Internal calls use right-to-left push order (C convention),
                    // so popping yields arguments in correct left-to-right display order.
                    args.push(value);
                }
            }
            let args = args.join(", ");
            let temp = self.next_temp();
            self.statements
                .push(format!("let {temp} = {callee}({args});"));
            self.stack.push(temp);
            return;
        }

        self.push_comment(instruction);
        let temp = self.next_temp();
        self.statements.push(format!("let {temp} = {callee}();"));
        self.stack.push(temp);
    }

    pub(in super::super) fn emit_relative_call(&mut self, instruction: &Instruction) {
        if let Some(target) = self.jump_target(instruction) {
            let resolved_target = self
                .call_targets_by_offset
                .get(&instruction.offset)
                .copied()
                .unwrap_or(target);
            self.emit_internal_call(instruction, resolved_target);
        } else if let Some(target) = self
            .call_targets_by_offset
            .get(&instruction.offset)
            .copied()
        {
            self.emit_internal_call(instruction, target);
        } else {
            self.warn(instruction, "call with unsupported operand (skipping)");
        }
    }

    pub(in super::super) fn emit_relative(&mut self, instruction: &Instruction, label: &str) {
        if self.skip_jumps.remove(&instruction.offset) {
            return;
        }
        if let Some(target) = self.jump_target(instruction) {
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
        match instruction.operand {
            Some(Operand::U16(value)) => {
                // CALLT: token-based indirect call with a U16 operand.
                // Resolve token metadata so argument consumption and return
                // behavior match the declared method signature.
                let index = value as usize;
                let resolved = self.callt_labels.get(index).cloned();
                if let Some(name) = resolved {
                    let arg_count = self.callt_param_counts.get(index).copied().unwrap_or(0);
                    let returns_value =
                        self.callt_returns_value.get(index).copied().unwrap_or(true);
                    self.push_comment(instruction);
                    if self.stack.len() < arg_count {
                        self.stack_underflow(instruction, arg_count);
                        return;
                    }
                    let mut args = Vec::with_capacity(arg_count);
                    for _ in 0..arg_count {
                        if let Some(value) = self.pop_stack_value() {
                            args.push(value);
                        }
                    }
                    let args = args.join(", ");
                    if returns_value {
                        let temp = self.next_temp();
                        self.statements
                            .push(format!("let {temp} = {name}({args});"));
                        self.stack.push(temp);
                    } else {
                        self.statements.push(format!("{name}({args});"));
                    }
                } else {
                    self.push_comment(instruction);
                    let temp = self.next_temp();
                    self.statements
                        .push(format!("let {temp} = {label}(0x{value:04X});"));
                    self.stack.push(temp);
                }
            }
            None => {
                // CALLA: stack-based indirect call — pops a Pointer from the
                // evaluation stack, so consume one stack entry as the target.
                let (target, literal) = self
                    .pop_stack_value_with_literal()
                    .unwrap_or_else(|| ("??".to_string(), None));
                if let Some(LiteralValue::Pointer(offset)) = literal {
                    self.emit_internal_call(instruction, offset);
                } else if let Some(offset) = self
                    .calla_targets_by_offset
                    .get(&instruction.offset)
                    .copied()
                {
                    self.emit_internal_call(instruction, offset);
                } else {
                    self.push_comment(instruction);
                    let temp = self.next_temp();
                    self.statements
                        .push(format!("let {temp} = {label}({target});"));
                    self.stack.push(temp);
                }
            }
            _ => self.warn(instruction, &format!("{label} (unexpected operand)")),
        }
    }

    pub(in super::super) fn emit_jump(&mut self, instruction: &Instruction) {
        if self.skip_jumps.remove(&instruction.offset) {
            // jump consumed by structured if/else handling
            return;
        }
        match self.jump_target(instruction) {
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

    pub(in super::super) fn emit_endtry(&mut self, instruction: &Instruction) {
        if self.skip_jumps.remove(&instruction.offset) {
            return;
        }

        self.push_comment(instruction);
        match self.jump_target(instruction) {
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
