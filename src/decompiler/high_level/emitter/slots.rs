use crate::instruction::{Instruction, Operand};

use super::{HighLevelEmitter, LiteralValue, SlotKind};

impl HighLevelEmitter {
    pub(super) fn emit_init_static_slots(&mut self, instruction: &Instruction) {
        self.push_comment(instruction);
        match instruction.operand {
            Some(Operand::U8(count)) => {
                self.statements
                    .push(format!("// declare {count} static slots"));
                self.static_pointer_values.clear();
            }
            _ => self.statements.push("// missing INITSSLOT operand".into()),
        }
    }

    pub(super) fn emit_init_slots(&mut self, instruction: &Instruction) {
        self.push_comment(instruction);
        match &instruction.operand {
            Some(Operand::Bytes(bytes)) if bytes.len() >= 2 => {
                let locals = bytes[0];
                let args = bytes[1];
                self.statements
                    .push(format!("// declare {locals} locals, {args} arguments"));
                self.local_pointer_values.clear();
            }
            _ => self.statements.push("// missing INITSLOT operand".into()),
        }
    }

    pub(super) fn emit_load_slot(
        &mut self,
        instruction: &Instruction,
        kind: SlotKind,
        index: usize,
    ) {
        self.push_comment(instruction);
        let name = self.slot_label(kind, index);
        match kind {
            SlotKind::Local => {
                if let Some(pointer) = self.local_pointer_values.get(&index).copied() {
                    self.literal_values
                        .insert(name.clone(), LiteralValue::Pointer(pointer));
                } else {
                    self.literal_values.remove(&name);
                }
            }
            SlotKind::Static => {
                if let Some(pointer) = self.static_pointer_values.get(&index).copied() {
                    self.literal_values
                        .insert(name.clone(), LiteralValue::Pointer(pointer));
                } else {
                    self.literal_values.remove(&name);
                }
            }
            SlotKind::Argument => {}
        }
        self.stack.push(name);
    }

    pub(super) fn emit_load_slot_from_operand(
        &mut self,
        instruction: &Instruction,
        kind: SlotKind,
    ) {
        let Some(index) = Self::slot_index_from_operand(instruction) else {
            self.warn(
                instruction,
                &format!("{} missing operand", instruction.opcode),
            );
            return;
        };
        self.emit_load_slot(instruction, kind, index);
    }

    pub(super) fn emit_store_slot(
        &mut self,
        instruction: &Instruction,
        kind: SlotKind,
        index: usize,
    ) {
        self.push_comment(instruction);
        if let Some((value, literal)) = self.pop_stack_value_with_literal() {
            let name = self.slot_label(kind, index);
            let use_let = match kind {
                SlotKind::Local => self.initialized_locals.insert(index),
                SlotKind::Static => self.initialized_statics.insert(index),
                SlotKind::Argument => false,
            };
            if use_let {
                self.statements.push(format!("let {name} = {value};"));
            } else {
                self.statements.push(format!("{name} = {value};"));
            }
            if let Some(elements) = self.packed_values_by_name.get(&value).cloned() {
                self.packed_values_by_name.insert(name.clone(), elements);
            } else {
                self.packed_values_by_name.remove(&name);
            }
            match kind {
                SlotKind::Local => {
                    if let Some(LiteralValue::Pointer(pointer)) = literal {
                        self.local_pointer_values.insert(index, pointer);
                    } else {
                        self.local_pointer_values.remove(&index);
                    }
                }
                SlotKind::Static => {
                    if let Some(LiteralValue::Pointer(pointer)) = literal {
                        self.static_pointer_values.insert(index, pointer);
                    } else {
                        self.static_pointer_values.remove(&index);
                    }
                }
                SlotKind::Argument => {}
            }
        } else {
            self.stack_underflow(instruction, 1);
        }
    }

    pub(super) fn emit_store_slot_from_operand(
        &mut self,
        instruction: &Instruction,
        kind: SlotKind,
    ) {
        let Some(index) = Self::slot_index_from_operand(instruction) else {
            self.warn(
                instruction,
                &format!("{} missing operand", instruction.opcode),
            );
            return;
        };
        self.emit_store_slot(instruction, kind, index);
    }

    fn slot_label(&self, kind: SlotKind, index: usize) -> String {
        match kind {
            SlotKind::Local => format!("loc{index}"),
            SlotKind::Argument => self
                .argument_labels
                .get(&index)
                .cloned()
                .unwrap_or_else(|| format!("arg{index}")),
            SlotKind::Static => format!("static{index}"),
        }
    }

    fn slot_index_from_operand(instruction: &Instruction) -> Option<usize> {
        match instruction.operand {
            Some(Operand::U8(value)) => Some(value as usize),
            _ => None,
        }
    }
}
