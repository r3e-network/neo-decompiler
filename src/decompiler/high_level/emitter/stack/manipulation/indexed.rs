use crate::instruction::Instruction;

use super::super::super::HighLevelEmitter;

impl HighLevelEmitter {
    pub(in super::super::super) fn emit_pick(&mut self, instruction: &Instruction) {
        self.push_comment(instruction);
        let Some(index_name) = self.stack.pop() else {
            self.stack_underflow(instruction, 1);
            return;
        };

        let index_literal = self.take_usize_literal(&index_name);

        if let Some(index) = index_literal {
            if index < self.stack.len() {
                let source = self.stack[self.stack.len() - 1 - index].clone();
                let temp = self.next_temp();
                self.statements
                    .push(format!("let {temp} = {source}; // pick stack[{index}]"));
                self.stack.push(temp.clone());
                if let Some(literal) = self.literal_values.get(&source).cloned() {
                    self.literal_values.insert(temp, literal);
                }
                return;
            }
        }

        let temp = self.next_temp();
        self.statements
            .push(format!("let {temp} = pick({index_name});"));
        self.stack.push(temp);
    }

    pub(in super::super::super) fn emit_roll(&mut self, instruction: &Instruction) {
        self.push_comment(instruction);
        if self.stack.len() < 2 {
            self.stack_underflow(instruction, 2);
            return;
        }
        let Some(index_name) = self.stack.pop() else {
            self.stack_underflow(instruction, 1);
            return;
        };

        let index_literal = self.take_usize_literal(&index_name);

        if let Some(index) = index_literal {
            if index < self.stack.len() {
                let pos = self.stack.len() - 1 - index;
                let value = self.stack.remove(pos);
                self.stack.push(value);
                self.statements
                    .push(format!("// roll stack[{index}] to top"));
                return;
            }
        }

        self.statements
            .push(format!("// roll stack[{index_name}] to top"));
    }

    pub(in super::super::super) fn emit_xdrop(&mut self, instruction: &Instruction) {
        self.push_comment(instruction);
        if self.stack.len() < 2 {
            self.stack_underflow(instruction, 2);
            return;
        }

        let index_name = self.stack.pop().expect("len checked");
        let index_literal = self.take_usize_literal(&index_name);

        if let Some(index) = index_literal {
            if index < self.stack.len() {
                let pos = self.stack.len() - 1 - index;
                let removed = self.stack.remove(pos);
                self.literal_values.remove(&removed);
                self.statements
                    .push(format!("// xdrop stack[{index}] (removed {removed})"));
                return;
            }
        }

        let removed = self.stack.pop().expect("len checked");
        self.literal_values.remove(&removed);
        self.statements
            .push(format!("// xdrop stack[{index_name}] (removed {removed})"));
    }
}
