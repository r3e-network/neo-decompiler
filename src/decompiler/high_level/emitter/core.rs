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
        emitter.pre_register_backward_jump_labels();
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

    pub(crate) fn set_noreturn_method_offsets(&mut self, offsets: &std::collections::BTreeSet<usize>) {
        self.noreturn_method_offsets = offsets.clone();
    }

    pub(crate) fn set_inline_single_use_temps(&mut self, enabled: bool) {
        self.inline_single_use_temps = enabled;
    }

    pub(crate) fn set_returns_void(&mut self, value: bool) {
        self.returns_void = value;
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
                // Merge point after an if/else (or plain if).  Reconcile the
                // stack states from both branches.
                if let Some(saved) = self.branch_saved_stacks.remove(&offset) {
                    let pre_depth = self
                        .pre_branch_stack_depth
                        .remove(&offset)
                        .unwrap_or(0);

                    if self.stack.is_empty() && !saved.is_empty() {
                        self.stack = saved;
                    } else if !self.stack.is_empty()
                        && !saved.is_empty()
                        && self.stack.len() == saved.len()
                        && self.stack.len() > pre_depth
                    {
                        let close_idx = self
                            .statements
                            .iter()
                            .rposition(|s| s.trim() == "}")
                            .unwrap_or(self.statements.len());
                        let mut inserts = Vec::new();
                        for i in pre_depth..self.stack.len() {
                            if self.stack[i] != saved[i] {
                                inserts.push(format!(
                                    "let {} = {};",
                                    saved[i], self.stack[i]
                                ));
                            }
                        }
                        for (j, stmt) in inserts.into_iter().enumerate() {
                            self.statements.insert(close_idx + j, stmt);
                        }
                        self.stack = saved;
                    }
                }
            }
        }

        // Restore try block's exit stack at the resume point after a
        // try-catch.  This must live outside the pending_closers gate
        // because the catch closer may be registered at the finally
        // offset rather than the ENDTRY target offset.
        if let Some(saved) = self.try_exit_stacks.remove(&offset) {
            self.stack = saved;
        }

        self.close_loops_at(offset);

        // Catch/finally MUST be emitted before else so that exception handlers
        // appear as siblings of the try block rather than nesting inside an
        // else branch when both targets share the same offset.
        if let Some(count) = self.catch_targets.remove(&offset) {
            // Save the try block's exit stack before the catch handler
            // clears it.  This lets us restore the stack at the resume
            // point after the try-catch so that values carried through
            // ENDTRY (e.g. return values) are not lost.
            if let Some(resume) = self.try_catch_resume.remove(&offset) {
                if !self.stack.is_empty() {
                    self.try_exit_stacks
                        .entry(resume)
                        .or_insert_with(|| self.stack.clone());
                }
            }
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

        if let Some(count) = self.else_targets.remove(&offset) {
            for _ in 0..count {
                self.statements.push("else {".into());
            }
            // Keep the saved pre-branch snapshot until the else block closes.
            // If the else branch terminates (throw/abort/return), merge-time
            // restoration still needs this snapshot.
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
        Self::rewrite_else_if_chains(&mut self.statements);
        Self::collapse_overflow_checks(&mut self.statements);
        Self::rewrite_goto_do_while(&mut self.statements);
        Self::rewrite_if_goto_to_while(&mut self.statements);
        Self::eliminate_fallthrough_gotos(&mut self.statements);
        Self::rewrite_for_loops(&mut self.statements);
        // Note: inline_single_use_temps is available but disabled by default
        // as it can be too aggressive for some use cases. Enable selectively.
        Self::inline_condition_temps(&mut self.statements);
        Self::inline_for_increment_temps(&mut self.statements);
        if self.inline_single_use_temps {
            Self::inline_single_use_temps(&mut self.statements);
        }
        Self::rewrite_compound_assignments(&mut self.statements);
        Self::rewrite_indexing_syntax(&mut self.statements);
        Self::collapse_if_true(&mut self.statements);
        Self::rewrite_switch_statements(&mut self.statements);
        Self::rewrite_switch_break_gotos(&mut self.statements);
        self.statements.retain(|line| !line.trim().is_empty());
        super::HighLevelOutput {
            statements: self.statements,
            warnings: self.warnings,
        }
    }
}
