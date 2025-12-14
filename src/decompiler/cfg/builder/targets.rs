use crate::instruction::{Instruction, OpCode, Operand};

use super::CfgBuilder;

impl<'a> CfgBuilder<'a> {
    pub(super) fn jump_target(&self, index: usize, instr: &Instruction) -> Option<usize> {
        let delta = match &instr.operand {
            Some(Operand::Jump(v)) => *v as isize,
            Some(Operand::Jump32(v)) => *v as isize,
            _ => return None,
        };

        let base = self
            .instruction_end_offset(index)
            .unwrap_or_else(|| instr.offset + self.instruction_len_fallback(instr));
        let target = base as isize + delta;
        if target < 0 {
            return None;
        }
        let target = target as usize;
        self.offset_to_index.contains_key(&target).then_some(target)
    }

    pub(super) fn try_targets(
        &self,
        index: usize,
        instr: &Instruction,
    ) -> Option<(Option<usize>, Option<usize>)> {
        let bytes = match &instr.operand {
            Some(Operand::Bytes(bytes)) => bytes.as_slice(),
            _ => return None,
        };

        let base = self
            .instruction_end_offset(index)
            .unwrap_or_else(|| instr.offset + self.instruction_len_fallback(instr));

        match instr.opcode {
            OpCode::Try => {
                if bytes.len() != 2 {
                    return None;
                }
                let catch_delta = bytes[0] as i8 as isize;
                let finally_delta = bytes[1] as i8 as isize;

                let catch_target = (catch_delta != 0)
                    .then(|| base as isize + catch_delta)
                    .filter(|target| *target >= 0)
                    .map(|target| target as usize)
                    .filter(|target| self.offset_to_index.contains_key(target));
                let finally_target = (finally_delta != 0)
                    .then(|| base as isize + finally_delta)
                    .filter(|target| *target >= 0)
                    .map(|target| target as usize)
                    .filter(|target| self.offset_to_index.contains_key(target));

                Some((catch_target, finally_target))
            }
            OpCode::TryL => {
                if bytes.len() != 8 {
                    return None;
                }
                let catch_delta = i32::from_le_bytes(bytes[0..4].try_into().unwrap()) as isize;
                let finally_delta = i32::from_le_bytes(bytes[4..8].try_into().unwrap()) as isize;

                let catch_target = (catch_delta != 0)
                    .then(|| base as isize + catch_delta)
                    .filter(|target| *target >= 0)
                    .map(|target| target as usize)
                    .filter(|target| self.offset_to_index.contains_key(target));
                let finally_target = (finally_delta != 0)
                    .then(|| base as isize + finally_delta)
                    .filter(|target| *target >= 0)
                    .map(|target| target as usize)
                    .filter(|target| self.offset_to_index.contains_key(target));

                Some((catch_target, finally_target))
            }
            _ => None,
        }
    }
}
