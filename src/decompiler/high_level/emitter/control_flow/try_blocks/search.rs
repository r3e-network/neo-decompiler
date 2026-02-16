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
        let mut first_forward = None;
        for (&offset, &index) in self.index_by_offset.range(start..end) {
            let instruction = self.program.get(index)?;
            if !matches!(instruction.opcode, OpCode::Endtry | OpCode::EndtryL) {
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
                        return Some((offset, target));
                    }
                    if first_forward.is_none() {
                        first_forward = Some((offset, target));
                    }
                }
            }
        }
        first_forward
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
