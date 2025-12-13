use crate::instruction::Instruction;

use super::super::super::HighLevelEmitter;

impl HighLevelEmitter {
    pub(in super::super::super) fn emit_reverse_fixed(
        &mut self,
        instruction: &Instruction,
        count: usize,
    ) {
        self.push_comment(instruction);
        if self.stack.len() < count {
            self.stack_underflow(instruction, count);
            return;
        }
        let start = self.stack.len() - count;
        self.stack[start..].reverse();
        self.statements
            .push(format!("// reverse top {count} stack values"));
    }

    pub(in super::super::super) fn emit_reverse_n(&mut self, instruction: &Instruction) {
        self.push_comment(instruction);
        let Some(count_name) = self.stack.pop() else {
            self.stack_underflow(instruction, 1);
            return;
        };

        let count_literal = self.take_usize_literal(&count_name);

        if let Some(count) = count_literal {
            if self.stack.len() >= count {
                let start = self.stack.len() - count;
                self.stack[start..].reverse();
                self.statements
                    .push(format!("// reverse top {count} stack values"));
                return;
            }
        }

        self.statements
            .push(format!("// reverse top {count_name} stack values"));
    }
}
