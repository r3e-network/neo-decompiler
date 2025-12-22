use crate::instruction::Instruction;

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
    }

    pub(crate) fn set_inline_single_use_temps(&mut self, enabled: bool) {
        self.inline_single_use_temps = enabled;
    }

    pub(crate) fn advance_to(&mut self, offset: usize) {
        if let Some(count) = self.pending_closers.remove(&offset) {
            for _ in 0..count {
                self.statements.push("}".into());
            }
        }

        self.close_loops_at(offset);

        if let Some(count) = self.else_targets.remove(&offset) {
            for _ in 0..count {
                self.statements.push("else {".into());
            }
        }

        if let Some(count) = self.catch_targets.remove(&offset) {
            for _ in 0..count {
                self.statements.push("catch {".into());
            }
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
    }

    pub(crate) fn finish(mut self) -> super::HighLevelOutput {
        if !self.pending_closers.is_empty() {
            let mut remaining: Vec<_> = self.pending_closers.into_iter().collect();
            remaining.sort_by_key(|(offset, _)| *offset);
            for (_, count) in remaining {
                for _ in 0..count {
                    self.statements.push("}".into());
                }
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
