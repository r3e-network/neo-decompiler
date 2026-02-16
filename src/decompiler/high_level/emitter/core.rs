use crate::instruction::{Instruction, OpCode};
use std::collections::BTreeMap;

use super::HighLevelEmitter;

impl HighLevelEmitter {
    pub(crate) fn with_program(instructions: &[Instruction]) -> Self {
        let mut emitter = Self {
            program: instructions.to_vec(),
            ..Self::default()
        };
        for (index, instruction) in instructions.iter().enumerate() {
            emitter.index_by_offset.insert(instruction.offset, index);
        }
        emitter.analyze_do_while_loops();
        emitter
    }

    pub(crate) fn set_argument_labels(&mut self, labels: &[String]) {
        for (index, label) in labels.iter().enumerate() {
            self.argument_labels.insert(index, label.clone());
        }
        let starts_with_initslot = self
            .program
            .first()
            .is_some_and(|instruction| instruction.opcode == OpCode::Initslot);
        if !starts_with_initslot {
            self.stack.extend(labels.iter().cloned());
        }
    }

    pub(crate) fn set_callt_labels(&mut self, labels: Vec<String>) {
        self.callt_labels = labels;
    }

    pub(crate) fn set_callt_param_counts(&mut self, counts: Vec<usize>) {
        self.callt_param_counts = counts;
    }

    pub(crate) fn set_callt_returns_value(&mut self, returns: Vec<bool>) {
        self.callt_returns_value = returns;
    }

    pub(crate) fn set_method_labels_by_offset(&mut self, labels: &BTreeMap<usize, String>) {
        self.method_labels_by_offset = labels.clone();
    }

    pub(crate) fn set_method_arg_counts_by_offset(&mut self, counts: &BTreeMap<usize, usize>) {
        self.method_arg_counts_by_offset = counts.clone();
    }

    pub(crate) fn set_call_targets_by_offset(&mut self, targets: &BTreeMap<usize, usize>) {
        self.call_targets_by_offset = targets.clone();
    }

    pub(crate) fn set_calla_targets_by_offset(&mut self, targets: &BTreeMap<usize, usize>) {
        self.calla_targets_by_offset = targets.clone();
    }

    pub(crate) fn set_inline_single_use_temps(&mut self, enabled: bool) {
        self.inline_single_use_temps = enabled;
    }

    pub(crate) fn advance_to(&mut self, offset: usize) {
        let entering_else = self.else_targets.contains_key(&offset);
        if let Some(count) = self.pending_closers.remove(&offset) {
            for _ in 0..count {
                self.statements.push("}".into());
            }
            if entering_else {
                // Before restoring else-entry state, capture the then-branch
                // terminal stack for the upcoming merge closer (if any). This
                // allows merge-time recovery when the else branch terminates.
                if let Some((&merge_offset, _)) = self.pending_closers.range((offset + 1)..).next()
                {
                    self.branch_saved_stacks
                        .entry(merge_offset)
                        .or_insert_with(|| self.stack.clone());
                }
                // Entering an else block: its entry stack must match the
                // pre-branch stack snapshot, not the stack mutated by the
                // then-branch instructions emitted just above.
                if let Some(saved) = self.branch_saved_stacks.get(&offset).cloned() {
                    self.stack = saved;
                }
            } else {
                // Restore the stack state saved before the branch body.
                // This handles cases where the branch body terminated
                // (throw/return/abort) and cleared the stack — the code
                // after the branch still needs the pre-branch stack.
                if let Some(saved) = self.branch_saved_stacks.remove(&offset) {
                    if self.stack.is_empty() && !saved.is_empty() {
                        self.stack = saved;
                    }
                }
            }
        }

        self.close_loops_at(offset);

        if let Some(count) = self.else_targets.remove(&offset) {
            for _ in 0..count {
                self.statements.push("else {".into());
            }
            // Keep the saved pre-branch snapshot until the else block closes.
            // If the else branch terminates (throw/abort/return), merge-time
            // restoration still needs this snapshot.
        }

        if let Some(count) = self.catch_targets.remove(&offset) {
            for _ in 0..count {
                self.statements.push("catch {".into());
            }
            // Neo VM enters catch handlers with the exception object on top of
            // an unwound evaluation stack.
            self.stack.clear();
            self.stack.push("exception".into());
        }

        if let Some(count) = self.finally_targets.remove(&offset) {
            for _ in 0..count {
                self.statements.push("finally {".into());
            }
        }

        if let Some(entries) = self.do_while_headers.remove(&offset) {
            for entry in entries {
                self.statements.push("do {".into());
                self.active_do_while_tails.insert(entry.tail_offset);
                self.loop_stack.push(super::LoopContext {
                    break_offset: entry.break_offset,
                    continue_offset: entry.tail_offset,
                });
            }
        }

        if let Some(headers) = self.pending_if_headers.remove(&offset) {
            for header in headers {
                self.statements.push(header);
            }
        }

        if self.transfer_labels.remove(&offset) {
            self.statements
                .push(format!("{}:", Self::transfer_label_name(offset)));
        }
    }

    pub(crate) fn finish(mut self) -> super::HighLevelOutput {
        // BTreeMap iterates in key order — no sort needed.
        for (_, count) in self.pending_closers {
            for _ in 0..count {
                self.statements.push("}".into());
            }
        }
        Self::rewrite_for_loops(&mut self.statements);
        Self::rewrite_else_if_chains(&mut self.statements);
        // Note: inline_single_use_temps is available but disabled by default
        // as it can be too aggressive for some use cases. Enable selectively.
        Self::inline_condition_temps(&mut self.statements);
        Self::inline_for_increment_temps(&mut self.statements);
        if self.inline_single_use_temps {
            Self::inline_single_use_temps(&mut self.statements);
        }
        Self::rewrite_compound_assignments(&mut self.statements);
        Self::rewrite_indexing_syntax(&mut self.statements);
        Self::rewrite_switch_statements(&mut self.statements);
        self.statements.retain(|line| !line.trim().is_empty());
        super::HighLevelOutput {
            statements: self.statements,
            warnings: self.warnings,
        }
    }
}
