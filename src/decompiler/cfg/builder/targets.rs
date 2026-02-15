// Bytecode offset arithmetic requires isize↔usize casts for signed jump deltas.
// NEF scripts are bounded (~1 MB), so these conversions are structurally safe on
// all supported targets (32-bit and 64-bit).
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]

use crate::instruction::{Instruction, OpCode, Operand};

use super::CfgBuilder;

impl<'a> CfgBuilder<'a> {
    /// Resolve a jump target offset.  Neo VM jump offsets are relative to the
    /// **opcode position** (start of the instruction), NOT the end.
    /// Reference: neo-vm `ExecuteJumpOffset` → `IP + offset`.
    pub(super) fn jump_target(&self, _index: usize, instr: &Instruction) -> Option<usize> {
        let delta = match &instr.operand {
            Some(Operand::Jump(v)) => *v as isize,
            Some(Operand::Jump32(v)) => *v as isize,
            _ => return None,
        };

        let target = instr.offset as isize + delta;
        if target < 0 {
            return None;
        }
        let target = target as usize;
        self.offset_to_index.contains_key(&target).then_some(target)
    }

    pub(super) fn try_targets(
        &self,
        _index: usize,
        instr: &Instruction,
    ) -> Option<(Option<usize>, Option<usize>)> {
        let bytes = match &instr.operand {
            Some(Operand::Bytes(bytes)) => bytes.as_slice(),
            _ => return None,
        };

        // Neo VM: try handler offsets are relative to the **opcode position**
        // (start of the instruction), NOT the end.
        let base = instr.offset as isize;

        match instr.opcode {
            OpCode::Try => {
                if bytes.len() != 2 {
                    return None;
                }
                let catch_delta = bytes[0] as i8 as isize;
                let finally_delta = bytes[1] as i8 as isize;

                let catch_target = (catch_delta != 0)
                    .then(|| base + catch_delta)
                    .filter(|target| *target >= 0)
                    .map(|target| target as usize)
                    .filter(|target| self.offset_to_index.contains_key(target));
                let finally_target = (finally_delta != 0)
                    .then(|| base + finally_delta)
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
                    .then(|| base + catch_delta)
                    .filter(|target| *target >= 0)
                    .map(|target| target as usize)
                    .filter(|target| self.offset_to_index.contains_key(target));
                let finally_target = (finally_delta != 0)
                    .then(|| base + finally_delta)
                    .filter(|target| *target >= 0)
                    .map(|target| target as usize)
                    .filter(|target| self.offset_to_index.contains_key(target));

                Some((catch_target, finally_target))
            }
            _ => None,
        }
    }
}
