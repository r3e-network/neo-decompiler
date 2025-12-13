//! Instruction dispatch table for the high-level emitter.
//!
//! This module translates raw VM opcodes into the higher-level helper methods
//! that build up pseudo-source statements.

use crate::instruction::Instruction;

use super::HighLevelEmitter;

mod collections;
mod control_flow;
mod literals;
mod math;
mod slots;
mod stack_ops;

impl HighLevelEmitter {
    pub(crate) fn emit_instruction(&mut self, instruction: &Instruction) {
        if self.try_emit_literals(instruction)
            || self.try_emit_math(instruction)
            || self.try_emit_stack_ops(instruction)
            || self.try_emit_slot_ops(instruction)
            || self.try_emit_collection_ops(instruction)
            || self.try_emit_control_flow(instruction)
        {
            return;
        }

        self.note(
            instruction,
            &format!("{} (not yet translated)", instruction.opcode),
        );
    }
}
