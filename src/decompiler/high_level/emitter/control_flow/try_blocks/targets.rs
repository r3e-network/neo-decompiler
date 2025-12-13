use crate::instruction::{Instruction, Operand};

use super::super::super::HighLevelEmitter;

impl HighLevelEmitter {
    pub(super) fn try_handler_targets(
        &self,
        instruction: &Instruction,
    ) -> Option<(usize, Option<usize>, Option<usize>)> {
        let Operand::Bytes(bytes) = instruction.operand.as_ref()? else {
            return None;
        };

        let (catch_delta, finally_delta) = match bytes.as_slice() {
            [catch, finally] => (*catch as i8 as isize, *finally as i8 as isize),
            slice if slice.len() == 8 => {
                let catch_delta = i32::from_le_bytes(slice[0..4].try_into().ok()?) as isize;
                let finally_delta = i32::from_le_bytes(slice[4..8].try_into().ok()?) as isize;
                (catch_delta, finally_delta)
            }
            _ => return None,
        };

        let width = 1 + bytes.len();
        let body_start = instruction.offset + width;
        let catch_target = if catch_delta != 0 {
            let target = instruction.offset as isize + width as isize + catch_delta;
            (target > instruction.offset as isize).then_some(target as usize)
        } else {
            None
        };
        let finally_target = if finally_delta != 0 {
            let target = instruction.offset as isize + width as isize + finally_delta;
            (target > instruction.offset as isize).then_some(target as usize)
        } else {
            None
        };

        Some((body_start, catch_target, finally_target))
    }
}
