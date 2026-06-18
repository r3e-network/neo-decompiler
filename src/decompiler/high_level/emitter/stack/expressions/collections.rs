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
            // Cap inline rendering to bound output for malformed inputs:
            // pathological NEFs can ask PACK to consume thousands of items
            // when the actual stack has only a handful, ballooning the
            // emitted statement and per-iteration `missing_pack_item()`
            // synthetic-temp lines. Hand-written contracts rarely PACK
            // more than a few dozen items, so 64 is a generous ceiling
            // for normal cases. For PACKMAP the cap applies to ENTRIES
            // (key/value pairs).
            const PACK_MAX_INLINE: usize = 64;
            // PACKMAP pops a key/value PAIR per entry — `Pop: 2n+1 item(s)`
            // per OpCode.cs — with the key popped before its value;
            // PACK/PACKSTRUCT pop one item per entry (`Pop: n+1`).
            let is_map = kind == "map";
            let unit = if is_map { 2 } else { 1 };
            // Bound the inline width by the actual simulated stack depth as well,
            // so a genuine literal-count underflow (PACK n with fewer than n
            // values on the stack) renders the elided remainder as a single
            // `/* N more */` marker instead of synthesizing `missing_pack_item()`
            // temps — matching the JS port for byte-for-byte parity.
            let avail = self.stack.len() / unit;
            let cap = PACK_MAX_INLINE.min(need).min(avail);
            let mut rendered = Vec::with_capacity(cap);
            let mut elements = Vec::with_capacity(cap * unit);
            for _ in 0..cap {
                let mut unit_values = Vec::with_capacity(unit);
                for _ in 0..unit {
                    if let Some((value, literal)) = self.pop_stack_value_with_literal() {
                        if let Some(literal) = literal {
                            self.literal_values.insert(value.clone(), literal);
                        }
                        unit_values.push(value);
                    } else {
                        let missing_temp = self.next_temp();
                        self.statements.push(format!(
                            "let {missing_temp} = missing_pack_item(); // synthetic missing element for literal pack"
                        ));
                        unit_values.push(missing_temp);
                    }
                }
                if is_map {
                    rendered.push(format!("{}: {}", unit_values[0], unit_values[1]));
                } else {
                    rendered.push(unit_values[0].clone());
                }
                elements.extend(unit_values);
            }
            if need > cap {
                // The VM still consumes the elided entries: drain whatever
                // the simulated stack actually holds (bounded by its depth)
                // so subsequent instructions bind the right operands, but
                // render them as a single elision marker.
                let mut excess = (need - cap).saturating_mul(unit);
                while excess > 0 && self.pop_stack_value().is_some() {
                    excess -= 1;
                }
                let remainder = need - cap;
                let noun = match (is_map, remainder == 1) {
                    (true, true) => "entry",
                    (true, false) => "entries",
                    (false, true) => "element",
                    (false, false) => "elements",
                };
                rendered.push(format!("/* {remainder} more {noun} */"));
                elements.push(format!("/* {remainder} more {noun} */"));
            }
            // Neo VM PACK: first popped item becomes array[0], second becomes
            // array[1], etc.  Since we pop in stack order (top-first), the
            // elements vector is already in correct array-index order — do NOT
            // reverse. PACKMAP entries are likewise added in pop order.
            let temp = self.next_temp();
            let body = rendered.join(", ");
            let (ctor, noun) = match kind {
                "map" => (format!("Map({body})"), "entry(s)"),
                "struct" => (format!("Struct({body})"), "element(s)"),
                _ => (format!("[{body}]"), "element(s)"),
            };
            self.statements
                .push(format!("let {temp} = {ctor}; // pack {need} {noun}"));
            // UNPACK of a map pushes key/value pairs plus the ENTRY count
            // (`Push: 2n+1`, OpCode.cs) — the flat element replay below
            // models arrays/structs only, so skip tracking for maps and
            // let UNPACK take its honest unknown-source path.
            if !is_map {
                self.packed_values_by_name
                    .insert(temp.clone(), elements.clone());
            }
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
        let has_dup_before =
            unpack_index > 0 && self.program[unpack_index - 1].opcode == OpCode::Dup;
        let count = if has_dup_before {
            pops.saturating_sub(1)
        } else {
            pops
        };

        if count == 0 {
            DEFAULT_COUNT
        } else {
            count
        }
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
