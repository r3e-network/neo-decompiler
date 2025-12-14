use crate::instruction::Instruction;

use super::{HighLevelEmitter, LiteralValue};

impl HighLevelEmitter {
    pub(super) fn note(&mut self, instruction: &Instruction, message: &str) {
        self.statements
            .push(format!("// {:04X}: {}", instruction.offset, message));
    }

    pub(super) fn stack_underflow(&mut self, instruction: &Instruction, needed: usize) {
        self.statements.push(format!(
            "// {:04X}: insufficient values on stack for {} (needs {needed})",
            instruction.offset, instruction.opcode
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
            _ => None,
        }
    }
}
