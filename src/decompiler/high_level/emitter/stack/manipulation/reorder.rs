use crate::instruction::Instruction;

use super::super::super::HighLevelEmitter;

impl HighLevelEmitter {
    pub(in super::super::super) fn emit_rot(&mut self, instruction: &Instruction) {
        self.push_comment(instruction);
        while self.stack.len() < 3 {
            let missing = self.next_temp();
            self.statements.push(format!(
                "let {missing} = missing_stack_item(); // synthetic missing stack value"
            ));
            // Missing values belong below currently known top-of-stack values.
            self.stack.insert(0, missing);
        }
        let (Some(top), Some(mid), Some(bottom)) =
            (self.stack.pop(), self.stack.pop(), self.stack.pop())
        else {
            return;
        };
        // ROT: bring the third item to the top -> [a, b, c] becomes [b, c, a]
        self.stack.push(mid);
        self.stack.push(top);
        self.stack.push(bottom);
        self.statements
            .push("// rotate top three stack values".into());
    }

    pub(in super::super::super) fn emit_tuck(&mut self, instruction: &Instruction) {
        self.push_comment(instruction);
        if self.stack.len() < 2 {
            self.stack_underflow(instruction, 2);
            return;
        }

        let (Some(top), Some(second)) = (self.stack.pop(), self.stack.pop()) else {
            return;
        };

        let dup = self.next_temp();
        self.statements
            .push(format!("let {dup} = {top}; // tuck top of stack"));
        if let Some(elements) = self.packed_values_by_name.get(&top).cloned() {
            self.packed_values_by_name.insert(dup.clone(), elements);
        }
        if let Some(literal) = self.literal_values.get(&top).cloned() {
            self.literal_values.insert(dup.clone(), literal);
        }

        self.stack.push(top);
        self.stack.push(second);
        self.stack.push(dup);
    }
}
