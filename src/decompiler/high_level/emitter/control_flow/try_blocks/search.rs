use crate::instruction::OpCode;

use super::super::super::HighLevelEmitter;

impl HighLevelEmitter {
    pub(super) fn find_endtry_target(&self, start: usize, end: usize) -> Option<(usize, usize)> {
        if start >= end {
            return None;
        }
        // In nested try blocks the search range may contain ENDTRYs that
        // belong to an inner try — their targets land *within* [start, end).
        // The ENDTRY we want belongs to the current (outer) try and its
        // target escapes the range (>= end).  We prefer that; only fall back
        // to the first forward ENDTRY when the search bound is the program
        // end (catch_end unknown), where no target can reach past it.
        //
        // To avoid grabbing ENDTRYs from sibling or nested TRY blocks, we
        // track TRY nesting depth: only consider ENDTRYs at depth 0.
        let mut first_forward = None;
        let mut last_escaping = None;
        let mut try_depth: usize = 0;
        for (&offset, &index) in self.index_by_offset.range(start..end) {
            let instruction = self.program.get(index)?;
            if matches!(instruction.opcode, OpCode::Try | OpCode::TryL) {
                try_depth += 1;
                continue;
            }
            if !matches!(instruction.opcode, OpCode::Endtry | OpCode::EndtryL) {
                // ENDFINALLY at depth > 0 closes an inner finally; the
                // matching inner TRY's catch/finally handlers may contain
                // their own ENDTRY that we must skip.  Decrement depth for
                // each ENDFINALLY since it terminates one TRY scope.
                if instruction.opcode == OpCode::Endfinally && try_depth > 0 {
                    try_depth -= 1;
                }
                continue;
            }
            // This is an ENDTRY/ENDTRY_L.
            if try_depth > 0 {
                // Belongs to an inner TRY — decrement depth and skip.
                try_depth -= 1;
                continue;
            }
            // Skip ENDTRYs already claimed by another try block (e.g. an
            // outer try's ENDTRY_L sitting between inner catch and finally).
            if self.skip_jumps.contains(&offset) {
                continue;
            }
            if let Some(target) = self.forward_jump_target(instruction) {
                if target > instruction.offset {
                    if target >= end {
                        // Prefer the LAST escaping ENDTRY — that is the
                        // normal try-body exit sitting right before the
                        // catch/finally handler.  Earlier escaping ENDTRYs
                        // are early exits (break/return inside loops) and
                        // should be emitted, not silently consumed.
                        last_escaping = Some((offset, target));
                    }
                    if first_forward.is_none() {
                        first_forward = Some((offset, target));
                    }
                }
            }
        }
        last_escaping.or(first_forward)
    }

    pub(super) fn find_endfinally_end(&self, start: usize) -> Option<(usize, usize)> {
        for (&offset, &index) in self.index_by_offset.range(start..) {
            let instruction = self.program.get(index)?;
            if instruction.opcode != OpCode::Endfinally {
                continue;
            }
            let end = self
                .program
                .get(index + 1)
                .map(|next| next.offset)
                .unwrap_or(offset + 1);
            return Some((offset, end));
        }
        None
    }
}
