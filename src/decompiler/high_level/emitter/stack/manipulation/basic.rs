use crate::instruction::Instruction;

use super::super::super::HighLevelEmitter;
use super::is_simple_literal_or_identifier;

impl HighLevelEmitter {
    pub(in super::super::super) fn drop_top(&mut self, instruction: &Instruction) {
        self.push_comment(instruction);
        if let Some(value) = self.stack.pop() {
            self.literal_values.remove(&value);
            self.statements.push(format!("// drop {value}"));
        } else {
            self.stack_underflow(instruction, 1);
        }
    }

    pub(in super::super::super) fn dup_top(&mut self, instruction: &Instruction) {
        self.push_comment(instruction);
        if let Some(value) = self.stack.last().cloned() {
            // Skip the temp materialization when the duplicated value
            // is a simple literal or identifier — pushing another copy
            // of the same expression string is semantically equivalent
            // and yields tighter output (`return 5 + 5;` instead of
            // `let t0 = 5; return t0 + t0;`). Mirrors the JS port's
            // `materialiseStackTopForDup`. Complex expressions still
            // get materialized so the side-effecting evaluation runs
            // exactly once.
            if is_simple_literal_or_identifier(&value) {
                self.stack.push(value);
                return;
            }
            let temp = self.next_temp();
            self.statements
                .push(format!("let {temp} = {value}; // duplicate top of stack"));
            self.stack.push(temp.clone());
            if let Some(elements) = self.packed_values_by_name.get(&value).cloned() {
                self.packed_values_by_name.insert(temp.clone(), elements);
            }
            if let Some(literal) = self.literal_values.get(&value).cloned() {
                self.literal_values.insert(temp.clone(), literal);
            }
        } else {
            self.stack_underflow(instruction, 1);
        }
    }

    pub(in super::super::super) fn over_second(&mut self, instruction: &Instruction) {
        self.push_comment(instruction);
        if self.stack.len() < 2 {
            self.stack_underflow(instruction, 2);
            return;
        }
        let value = self.stack[self.stack.len() - 2].clone();
        // Same skip-on-literal optimization as `dup_top`: copying a
        // bare literal/identifier doesn't require a temp.
        if is_simple_literal_or_identifier(&value) {
            self.stack.push(value);
            return;
        }
        let temp = self.next_temp();
        self.statements
            .push(format!("let {temp} = {value}; // copy second stack value"));
        self.stack.push(temp.clone());
        if let Some(elements) = self.packed_values_by_name.get(&value).cloned() {
            self.packed_values_by_name.insert(temp.clone(), elements);
        }
        if let Some(literal) = self.literal_values.get(&value).cloned() {
            self.literal_values.insert(temp.clone(), literal);
        }
    }

    pub(in super::super::super) fn swap_top(&mut self, instruction: &Instruction) {
        self.push_comment(instruction);
        while self.stack.len() < 2 {
            let missing = self.next_temp();
            self.statements.push(format!(
                "let {missing} = missing_stack_item(); // synthetic missing stack value"
            ));
            // Missing values belong below currently known top-of-stack values.
            self.stack.insert(0, missing);
        }
        let len = self.stack.len();
        self.stack.swap(len - 1, len - 2);
        self.statements
            .push("// swapped top two stack values".into());
    }

    pub(in super::super::super) fn nip_second(&mut self, instruction: &Instruction) {
        self.push_comment(instruction);
        if self.stack.len() < 2 {
            self.stack_underflow(instruction, 2);
            return;
        }
        let removed = self.stack.remove(self.stack.len() - 2);
        self.literal_values.remove(&removed);
        self.statements
            .push(format!("// remove second stack value {removed}"));
    }
}
