use crate::instruction::OpCode;

use super::super::super::HighLevelEmitter;

impl HighLevelEmitter {
    pub(super) fn find_endtry_target(&self, start: usize, end: usize) -> Option<(usize, usize)> {
        if start >= end {
            return None;
        }
        for (&offset, &index) in self.index_by_offset.range(start..end) {
            let instruction = self.program.get(index)?;
            if !matches!(instruction.opcode, OpCode::Endtry | OpCode::EndtryL) {
                continue;
            }
            if let Some(target) = self.forward_jump_target(instruction) {
                if target > instruction.offset {
                    return Some((offset, target));
                }
            }
        }
        None
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
