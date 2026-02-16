use crate::instruction::{Instruction, OpCode, Operand};

use super::super::{format_pushdata, HighLevelEmitter};

impl HighLevelEmitter {
    /// Resolve a PUSHA instruction's operand into a human-readable function
    /// pointer display string (e.g. `&sub_0x000C`).  Falls back to the raw
    /// operand value when the target cannot be computed.
    fn resolve_pusha_display(&self, instruction: &Instruction) -> String {
        let delta = match instruction.operand {
            Some(Operand::U32(value)) => i32::from_le_bytes(value.to_le_bytes()) as isize,
            Some(Operand::I32(value)) => value as isize,
            _ => {
                return instruction
                    .operand
                    .as_ref()
                    .map_or_else(|| "null".to_string(), |op| op.to_string());
            }
        };
        let Some(target) = instruction.offset.checked_add_signed(delta) else {
            return format!("{delta}");
        };
        if let Some(label) = self.method_labels_by_offset.get(&target) {
            format!("&{label}")
        } else {
            format!("&fn_0x{target:04X}")
        }
    }

    pub(super) fn try_emit_literals(&mut self, instruction: &Instruction) -> bool {
        use OpCode::*;

        match instruction.opcode {
            Pushdata1 | Pushdata2 | Pushdata4 => {
                if let Some(Operand::Bytes(bytes)) = &instruction.operand {
                    self.push_literal(instruction, format_pushdata(bytes));
                } else {
                    self.warn(
                        instruction,
                        "literal push missing operand (malformed instruction)",
                    );
                }
                true
            }
            PushA => {
                // PUSHA pushes a function pointer (relative offset from the
                // instruction).  Display the resolved absolute target address
                // rather than the raw delta integer.
                let value = self.resolve_pusha_display(instruction);
                self.push_literal(instruction, value);
                true
            }
            Pushint8 | Pushint16 | Pushint32 | Pushint64 | Pushint128 | Pushint256 | PushM1
            | Push0 | Push1 | Push2 | Push3 | Push4 | Push5 | Push6 | Push7 | Push8 | Push9
            | Push10 | Push11 | Push12 | Push13 | Push14 | Push15 | Push16 | PushT | PushF
            | PushNull => {
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
