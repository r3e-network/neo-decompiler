use crate::instruction::{Instruction, OpCode};

use super::super::HighLevelEmitter;

impl HighLevelEmitter {
    pub(super) fn try_emit_literals(&mut self, instruction: &Instruction) -> bool {
        use OpCode::*;

        match instruction.opcode {
            Pushint8 | Pushint16 | Pushint32 | Pushint64 | Pushint128 | Pushint256 | Pushdata1
            | Pushdata2 | Pushdata4 | PushM1 | Push0 | Push1 | Push2 | Push3 | Push4 | Push5
            | Push6 | Push7 | Push8 | Push9 | Push10 | Push11 | Push12 | Push13 | Push14
            | Push15 | Push16 | PushT | PushF | PushA | PushNull => {
                if let Some(operand) = &instruction.operand {
                    self.push_literal(instruction, operand.to_string());
                } else {
                    self.warn(
                        instruction,
                        "literal push missing operand (malformed instruction)",
                    );
                }
                true
            }
            _ => false,
        }
    }
}
