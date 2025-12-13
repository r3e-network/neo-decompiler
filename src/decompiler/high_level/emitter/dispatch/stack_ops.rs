use crate::instruction::{Instruction, OpCode};

use super::super::HighLevelEmitter;

impl HighLevelEmitter {
    pub(super) fn try_emit_stack_ops(&mut self, instruction: &Instruction) -> bool {
        use OpCode::*;

        match instruction.opcode {
            Depth => self.push_literal(instruction, self.stack.len().to_string()),
            Drop => self.drop_top(instruction),
            Nip => self.nip_second(instruction),
            Xdrop => self.emit_xdrop(instruction),
            Clear => {
                self.push_comment(instruction);
                self.stack.clear();
                self.statements.push("// clear stack".into());
            }
            Dup => self.dup_top(instruction),
            Over => self.over_second(instruction),
            Pick => self.emit_pick(instruction),
            Tuck => self.emit_tuck(instruction),
            Swap => self.swap_top(instruction),
            Rot => self.emit_rot(instruction),
            Roll => self.emit_roll(instruction),
            Reverse3 => self.emit_reverse_fixed(instruction, 3),
            Reverse4 => self.emit_reverse_fixed(instruction, 4),
            Reversen => self.emit_reverse_n(instruction),
            _ => return false,
        }

        true
    }
}
