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

            let mut catch_end = finally_target.or(resume_target);

            // Search the catch body for its ENDTRY.  When catch_end is already
            // known (from a finally target or the try-body's ENDTRY), use it as
            // the search bound.  Otherwise the try body always terminates
            // (throw/abort) so there was no ENDTRY — search forward from the
            // catch target to find the catch body's own ENDTRY.
            let search_bound = catch_end.unwrap_or_else(|| {
                self.program.last().map(|i| i.offset + 1).unwrap_or(catch)
            });
            if let Some((endtry_offset, target)) = self.find_endtry_target(catch, search_bound)
            {
                self.skip_jumps.insert(endtry_offset);
                resume_target.get_or_insert(target);
                catch_end.get_or_insert(target);
            }

            // Fallback: when the catch body has no ENDTRY (e.g. it contains
            // an inner try-finally that ends with ENDFINALLY and the catch
            // itself terminates via throw/abort), use the ENDFINALLY position
            // to infer where the catch body ends.
            if catch_end.is_none() {
                if let Some((_, endfinally_end)) = self.find_endfinally_end(catch) {
                    catch_end = Some(endfinally_end);
                }
            }

            if let Some(end) = catch_end {
                let closer_entry = self.pending_closers.entry(end).or_insert(0);
                *closer_entry += 1;
            }
        }

        if let Some(finally) = finally_target {
            // Suppress duplicate `finally {` when this finally target falls
            // inside an already-registered finally body.  The Neo compiler
            // sometimes points an outer TRY's finally into the middle of an
            // inner TRY's finally body (code sharing / overlapping regions).
            let already_covered = self
                .finally_body_ranges
                .iter()
                .any(|&(start, end)| finally > start && finally < end);

            if already_covered {
                // The code at `finally` is already inside another finally
                // block — do not emit a redundant `finally { }` wrapper.
                // We still need to remove the closer that was registered for
                // the try body at this offset (the `}` before `finally {`),
                // since there is no `finally {` to pair with.
                //
                // Actually, the try body closer at `try_end` (= min handler)
                // is correct — it closes the try body `}`.  The catch closer
                // at `finally` offset closes the catch body `}`.  Both are
                // needed.  We just skip emitting the finally header + closer.
            } else {
                let finally_entry = self.finally_targets.entry(finally).or_insert(0);
                *finally_entry += 1;

                let mut endfinally_end = None;
                if let Some((endfinally_offset, end)) = self.find_endfinally_end(finally) {
                    self.skip_jumps.insert(endfinally_offset);
                    endfinally_end = Some(end);

                    // Record this finally body range so nested TRY blocks
                    // whose finally targets overlap can be detected.
                    self.finally_body_ranges.push((finally, end));
                }

                // The finally closer belongs at the instruction after ENDFINALLY,
                // not at the try/catch resume point.  In nested try blocks these
                // can differ (resume_target may precede the finally block).
                //
                // When neither ENDFINALLY nor a resume target exists, the finally
                // block terminates unconditionally (RET/THROW/ABORT).  Register
                // the closer past the last instruction so `finish()` flushes it.
                let finally_end = endfinally_end
                    .or(resume_target)
                    .or_else(|| self.program.last().map(|i| i.offset + 1));
                if let Some(end) = finally_end {
                    // Record the finally body range even when ENDFINALLY is
                    // absent (the compiler omits it when all paths inside the
                    // finally block terminate unconditionally via abort/throw).
                    // This lets detect_implicit_else avoid wrapping sequential
                    // code inside finally blocks in spurious else branches.
                    if endfinally_end.is_none() {
                        self.finally_body_ranges.push((finally, end));
                    }
                    let closer_entry = self.pending_closers.entry(end).or_insert(0);
                    *closer_entry += 1;
                }
            }
        }
    }
}
