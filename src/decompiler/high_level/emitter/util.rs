use crate::instruction::Instruction;

use super::{HighLevelEmitter, LiteralValue};

impl HighLevelEmitter {
    pub(super) fn note(&mut self, instruction: &Instruction, message: &str) {
        self.statements
            .push(format!("// {:04X}: {}", instruction.offset, message));
    }

    pub(super) fn warn(&mut self, instruction: &Instruction, message: &str) {
        self.note(instruction, message);
        self.warnings.push(format!(
            "high-level: 0x{:04X}: {}",
            instruction.offset, message
        ));
    }

    pub(super) fn stack_underflow(&mut self, instruction: &Instruction, needed: usize) {
        let message = format!(
            "insufficient values on stack for {} (needs {needed})",
            instruction.opcode
        );
        self.note(instruction, &message);
        self.warnings.push(format!(
            "high-level: 0x{:04X}: {}",
            instruction.offset, message
        ));
    }

    pub(super) fn push_comment(&mut self, instruction: &Instruction) {
        self.statements.push(format!(
            "// {:04X}: {}",
            instruction.offset, instruction.opcode
        ));
    }

    pub(super) fn next_temp(&mut self) -> String {
        let name = format!("t{}", self.next_temp);
        self.next_temp += 1;
        name
    }

    pub(super) fn take_usize_literal(&mut self, name: &str) -> Option<usize> {
        match self.literal_values.remove(name) {
            Some(LiteralValue::Integer(value)) => usize::try_from(value).ok(),
            Some(LiteralValue::Pointer(value)) => Some(value),
            _ => None,
        }
    }

    pub(super) fn transfer_label_name(target: usize) -> String {
        format!("label_0x{target:04X}")
    }
}
