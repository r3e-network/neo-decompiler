use crate::instruction::Instruction;

use super::super::super::{convert_target_name, HighLevelEmitter};

impl HighLevelEmitter {
    pub(in super::super::super) fn emit_pack(&mut self, instruction: &Instruction, kind: &str) {
        self.push_comment(instruction);
        let Some(count_name) = self.stack.pop() else {
            self.stack_underflow(instruction, 1);
            return;
        };

        let count_literal = self.take_usize_literal(&count_name);

        if let Some(need) = count_literal {
            if self.stack.len() < need {
                self.stack_underflow(instruction, need);
                return;
            }
            let mut elements = Vec::with_capacity(need);
            for _ in 0..need {
                if let Some(val) = self.pop_stack_value() {
                    elements.push(val);
                }
            }
            elements.reverse();
            let temp = self.next_temp();
            let body = elements.join(", ");
            let ctor = match kind {
                "map" => format!("Map({})", body),
                "struct" => format!("Struct({})", body),
                _ => format!("[{body}]"),
            };
            self.statements
                .push(format!("let {temp} = {ctor}; // pack {need} element(s)"));
            self.stack.push(temp);
        } else {
            let temp = self.next_temp();
            self.statements.push(format!(
                "let {temp} = pack_dynamic({count_name}); // pack with dynamic count"
            ));
            self.stack.push(temp);
        }
    }

    pub(in super::super::super) fn emit_unpack(&mut self, instruction: &Instruction) {
        self.push_comment(instruction);
        if let Some(value) = self.pop_stack_value() {
            // Unpack pushes each element; represent as a single temp to preserve stack shape.
            let temp = self.next_temp();
            self.statements
                .push(format!("let {temp} = unpack({value});"));
            self.stack.push(temp);
        } else {
            self.stack_underflow(instruction, 1);
        }
    }

    pub(in super::super::super) fn emit_convert(&mut self, instruction: &Instruction) {
        self.push_comment(instruction);
        if let Some(value) = self.pop_stack_value() {
            if let Some(target) = instruction.operand.as_ref().and_then(convert_target_name) {
                let temp = self.next_temp();
                self.statements
                    .push(format!("let {temp} = convert_to_{target}({value});"));
                self.stack.push(temp);
            } else {
                let temp = self.next_temp();
                self.statements
                    .push(format!("let {temp} = convert({value});"));
                self.stack.push(temp);
            }
        } else {
            self.stack_underflow(instruction, 1);
        }
    }
}
