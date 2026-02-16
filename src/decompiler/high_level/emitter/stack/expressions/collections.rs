use crate::instruction::Instruction;

use super::super::super::{
    convert_target_name, format_type_operand, HighLevelEmitter, LiteralValue,
};

impl HighLevelEmitter {
    pub(in super::super::super) fn emit_pack(&mut self, instruction: &Instruction, kind: &str) {
        self.push_comment(instruction);
        let Some(count_name) = self.stack.pop() else {
            self.stack_underflow(instruction, 1);
            return;
        };

        let count_literal = self.take_usize_literal(&count_name);

        if let Some(need) = count_literal {
            let mut elements = Vec::with_capacity(need);
            for _ in 0..need {
                if let Some(val) = self.pop_stack_value() {
                    elements.push(val);
                } else {
                    let missing_temp = self.next_temp();
                    self.statements.push(format!(
                        "let {missing_temp} = missing_pack_item(); // synthetic missing element for literal pack"
                    ));
                    elements.push(missing_temp);
                }
            }
            // Neo VM PACK: first popped item becomes array[0], second becomes
            // array[1], etc.  Since we pop in stack order (top-first), the
            // elements vector is already in correct array-index order — do NOT
            // reverse.
            let temp = self.next_temp();
            let body = elements.join(", ");
            let ctor = match kind {
                "map" => format!("Map({})", body),
                "struct" => format!("Struct({})", body),
                _ => format!("[{body}]"),
            };
            self.statements
                .push(format!("let {temp} = {ctor}; // pack {need} element(s)"));
            self.packed_values_by_name
                .insert(temp.clone(), elements.clone());
            self.stack.push(temp);
        } else {
            let temp = self.next_temp();
            self.statements.push(format!(
                "let {temp} = pack_dynamic({count_name}); // pack with dynamic count"
            ));
            self.packed_values_by_name.remove(&temp);
            self.stack.push(temp);
        }
    }

    pub(in super::super::super) fn emit_unpack(&mut self, instruction: &Instruction) {
        self.push_comment(instruction);
        if let Some(value) = self.pop_stack_value() {
            if let Some(elements) = self.packed_values_by_name.get(&value).cloned() {
                // Neo VM UNPACK pushes array[n-1] first, array[0] last (on top).
                // Our elements vector is in array-index order [0..n-1], so push
                // in reverse so that elements[0] ends up on top of the stack.
                for element in elements.iter().rev() {
                    self.stack.push(element.clone());
                }
                let count_temp = self.next_temp();
                let count = elements.len() as i64;
                self.statements.push(format!(
                    "let {count_temp} = len({value}); // unpack also pushes element count"
                ));
                self.literal_values
                    .insert(count_temp.clone(), LiteralValue::Integer(count));
                self.stack.push(count_temp);
                return;
            }

            // Neo VM UNPACK: pops a compound type, pushes each element, then pushes the count.
            // For unknown shapes, synthesize a small placeholder stack shape plus count.
            // Four placeholders cover common tuple/map-entry patterns without requiring
            // brittle lookahead over downstream stack consumers.
            const UNKNOWN_UNPACK_PLACEHOLDER_COUNT: usize = 4;
            let elements_temp = self.next_temp();
            self.statements.push(format!(
                "let {elements_temp} = unpack({value}); // unknown unpack source"
            ));
            for index in 0..UNKNOWN_UNPACK_PLACEHOLDER_COUNT {
                let element_temp = self.next_temp();
                self.statements.push(format!(
                    "let {element_temp} = unpack_item({elements_temp}, {index}); // synthetic unpack element"
                ));
                self.stack.push(element_temp);
            }

            let count_temp = self.next_temp();
            self.statements.push(format!(
                "let {count_temp} = len({value}); // unpack also pushes element count"
            ));
            self.stack.push(count_temp);
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

    pub(in super::super::super) fn emit_is_type(&mut self, instruction: &Instruction) {
        self.push_comment(instruction);
        if let Some(value) = self.pop_stack_value() {
            let temp = self.next_temp();
            if let Some(target) = instruction.operand.as_ref().and_then(convert_target_name) {
                self.statements
                    .push(format!("let {temp} = is_type_{target}({value});"));
            } else if let Some(operand) = instruction.operand.as_ref() {
                let literal = format_type_operand(operand);
                self.statements
                    .push(format!("let {temp} = is_type({value}, {literal});"));
            } else {
                self.statements
                    .push(format!("let {temp} = is_type({value});"));
            }
            self.stack.push(temp);
        } else {
            self.stack_underflow(instruction, 1);
        }
    }
}
