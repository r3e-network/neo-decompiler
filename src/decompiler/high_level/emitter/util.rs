use crate::instruction::Instruction;

use super::{HighLevelEmitter, LiteralValue};

impl HighLevelEmitter {
    pub(super) fn note(&mut self, instruction: &Instruction, message: &str) {
        if !self.emit_trace_comments {
            return;
        }
        self.statements
            .push(format!("// {:04X}: {}", instruction.offset, message));
    }

    pub(super) fn warn(&mut self, instruction: &Instruction, message: &str) {
        // Warnings are real holes in the lifted source — an untranslated
        // opcode, a malformed operand, an unsupported call shape — and a
        // reader needs to see them inline regardless of whether trace
        // comments are otherwise enabled. Use a `// warning:` prefix so
        // the inline annotation is clearly semantic (a known limitation
        // or hazard), distinct from the per-instruction trace stream
        // (`// XXXX: OPCODE`) which is gated on `emit_trace_comments`.
        // The JS port already uses this same prefix; aligning here
        // gives byte-identical warning rendering across ports.
        self.statements.push(format!("// warning: {message}"));
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
        if !self.emit_trace_comments {
            return;
        }
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
