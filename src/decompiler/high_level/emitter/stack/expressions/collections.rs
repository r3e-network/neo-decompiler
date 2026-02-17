use crate::instruction::{Instruction, OpCode};

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
            // Infer the actual element count by scanning forward: after UNPACK the
            // typical pattern is DROP (count) followed by N single-pop instructions
            // (STLOC/STARG/STSFLD/DROP) that consume the elements.  If DUP preceded
            // UNPACK, one of those pops consumes the original (non-DUP'd) array.
            let element_count = self.infer_unpack_element_count(instruction);
            let elements_temp = self.next_temp();
            self.statements.push(format!(
                "let {elements_temp} = unpack({value}); // unknown unpack source"
            ));
            for index in 0..element_count {
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

    /// Infer the actual element count for an UNPACK with unknown source by
    /// scanning forward in the instruction stream.  The typical post-UNPACK
    /// pattern is: DROP (count), then N × single-pop instructions (STLOC,
    /// STARG, STSFLD, DROP) that consume the elements.  If DUP preceded
    /// UNPACK, one of those trailing pops consumes the original array.
    fn infer_unpack_element_count(&self, instruction: &Instruction) -> usize {
        const DEFAULT_COUNT: usize = 4;

        let Some(&unpack_index) = self.index_by_offset.get(&instruction.offset) else {
            return DEFAULT_COUNT;
        };

        // Scan forward: first single-pop should be DROP (count).
        let mut cursor = unpack_index + 1;
        if cursor >= self.program.len() {
            return DEFAULT_COUNT;
        }
        if self.program[cursor].opcode != OpCode::Drop {
            return DEFAULT_COUNT;
        }
        cursor += 1; // skip the count DROP

        // Count consecutive single-pop instructions after the count DROP.
        let mut pops = 0usize;
        while cursor < self.program.len() && Self::is_single_pop(self.program[cursor].opcode) {
            pops += 1;
            cursor += 1;
        }

        if pops == 0 {
            return DEFAULT_COUNT;
        }

        // If DUP preceded UNPACK, one pop consumes the original array copy.
        let has_dup_before = unpack_index > 0
            && self.program[unpack_index - 1].opcode == OpCode::Dup;
        let count = if has_dup_before {
            pops.saturating_sub(1)
        } else {
            pops
        };

        if count == 0 { DEFAULT_COUNT } else { count }
    }

    fn is_single_pop(opcode: OpCode) -> bool {
        matches!(
            opcode,
            OpCode::Drop
                | OpCode::Stloc0
                | OpCode::Stloc1
                | OpCode::Stloc2
                | OpCode::Stloc3
                | OpCode::Stloc4
                | OpCode::Stloc5
                | OpCode::Stloc6
                | OpCode::Stloc
                | OpCode::Starg0
                | OpCode::Starg1
                | OpCode::Starg2
                | OpCode::Starg3
                | OpCode::Starg4
                | OpCode::Starg5
                | OpCode::Starg6
                | OpCode::Starg
                | OpCode::Stsfld0
                | OpCode::Stsfld1
                | OpCode::Stsfld2
                | OpCode::Stsfld3
                | OpCode::Stsfld4
                | OpCode::Stsfld5
                | OpCode::Stsfld6
                | OpCode::Stsfld
        )
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
