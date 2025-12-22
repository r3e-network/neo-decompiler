use crate::instruction::Instruction;

use super::super::super::HighLevelEmitter;

impl HighLevelEmitter {
    pub(in super::super::super) fn emit_try_block(&mut self, instruction: &Instruction) {
        self.push_comment(instruction);

        let Some((body_start, catch_target, finally_target)) =
            self.try_handler_targets(instruction)
        else {
            self.warn(instruction, "try with unsupported operand (skipping)");
            return;
        };

        self.statements.push("try {".into());

        let mut handlers = Vec::new();
        if let Some(catch) = catch_target {
            handlers.push(catch);
        }
        if let Some(finally) = finally_target {
            handlers.push(finally);
        }
        handlers.sort_unstable();

        let try_end = handlers.first().copied();
        if let Some(end) = try_end {
            let closer_entry = self.pending_closers.entry(end).or_insert(0);
            *closer_entry += 1;
        }

        let mut resume_target = None;
        if let Some(end) = try_end {
            if let Some((endtry_offset, target)) = self.find_endtry_target(body_start, end) {
                self.skip_jumps.insert(endtry_offset);
                resume_target = Some(target);
            }
        }

        if let Some(catch) = catch_target {
            let catch_entry = self.catch_targets.entry(catch).or_insert(0);
            *catch_entry += 1;

            let catch_end = finally_target.or(resume_target);
            if let Some(end) = catch_end {
                let closer_entry = self.pending_closers.entry(end).or_insert(0);
                *closer_entry += 1;
            }

            if let Some(end) = catch_end {
                if let Some((endtry_offset, target)) = self.find_endtry_target(catch, end) {
                    self.skip_jumps.insert(endtry_offset);
                    resume_target.get_or_insert(target);
                }
            }
        }

        if let Some(finally) = finally_target {
            let finally_entry = self.finally_targets.entry(finally).or_insert(0);
            *finally_entry += 1;

            let mut endfinally_end = None;
            if let Some((endfinally_offset, end)) = self.find_endfinally_end(finally) {
                self.skip_jumps.insert(endfinally_offset);
                endfinally_end = Some(end);
            }

            let finally_end = resume_target.or(endfinally_end);
            if let Some(end) = finally_end {
                let closer_entry = self.pending_closers.entry(end).or_insert(0);
                *closer_entry += 1;
            }
        }
    }
}
