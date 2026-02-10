use crate::instruction::Instruction;

use super::super::super::{literal_from_operand, HighLevelEmitter};

impl HighLevelEmitter {
    pub(super) fn pop_stack_value(&mut self) -> Option<String> {
        if let Some(name) = self.stack.pop() {
            self.literal_values.remove(&name);
            Some(name)
        } else {
            None
        }
    }

    pub(in super::super::super) fn emit_call(
        &mut self,
        instruction: &Instruction,
        name: &str,
        arg_count: usize,
        returns_value: bool,
    ) {
        self.push_comment(instruction);
        if self.stack.len() < arg_count {
            self.stack_underflow(instruction, arg_count);
            return;
        }

        let mut args = Vec::with_capacity(arg_count);
        for _ in 0..arg_count {
            if let Some(value) = self.pop_stack_value() {
                args.push(value);
            }
        }
        args.reverse();
        let args = args.join(", ");

        if returns_value {
            let temp = self.next_temp();
            self.statements
                .push(format!("let {temp} = {name}({args});"));
            self.stack.push(temp);
        } else {
            self.statements.push(format!("{name}({args});"));
        }
    }

    pub(in super::super::super) fn push_literal(
        &mut self,
        instruction: &Instruction,
        value: String,
    ) {
        self.push_comment(instruction);
        let temp = self.next_temp();
        self.statements.push(format!("let {temp} = {value};"));
        if let Some(literal) = literal_from_operand(instruction.operand.as_ref()) {
            self.literal_values.insert(temp.clone(), literal);
        }
        self.stack.push(temp);
    }

    pub(in super::super::super) fn binary_op(&mut self, instruction: &Instruction, symbol: &str) {
        self.push_comment(instruction);
        if self.stack.len() < 2 {
            self.stack_underflow(instruction, 2);
            return;
        }

        let (Some(right), Some(left)) = (self.pop_stack_value(), self.pop_stack_value()) else {
            return;
        };
        let temp = self.next_temp();
        self.statements
            .push(format!("let {temp} = {left} {symbol} {right};"));
        self.stack.push(temp);
    }

    pub(in super::super::super) fn unary_op<F>(&mut self, instruction: &Instruction, build: F)
    where
        F: Fn(&str) -> String,
    {
        self.push_comment(instruction);
        if let Some(value) = self.pop_stack_value() {
            let temp = self.next_temp();
            self.statements
                .push(format!("let {temp} = {};", build(&value)));
            self.stack.push(temp);
        } else {
            self.stack_underflow(instruction, 1);
        }
    }
}
