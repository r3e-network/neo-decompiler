use crate::instruction::{Instruction, OpCode};

use super::super::HighLevelEmitter;

impl HighLevelEmitter {
    pub(super) fn try_emit_collection_ops(&mut self, instruction: &Instruction) -> bool {
        use OpCode::*;

        match instruction.opcode {
            Newbuffer => self.unary_op(instruction, |val| format!("new_buffer({val})")),
            Memcpy => self.emit_call(instruction, "memcpy", 3, false),
            Newarray0 => self.push_literal(instruction, "[]".into()),
            Newarray => self.unary_op(instruction, |val| format!("new_array({val})")),
            NewarrayT => {
                let ty = instruction
                    .operand
                    .as_ref()
                    .and_then(super::super::convert_target_name)
                    .map(|t| format!("{t:?}"))
                    .unwrap_or_else(|| "unknown".to_string());
                self.unary_op(instruction, |val| format!("new_array_t({val}, {ty})"))
            }
            Newstruct0 => self.push_literal(instruction, "Struct()".into()),
            Newstruct => self.unary_op(instruction, |val| format!("new_struct({val})")),
            Newmap => self.push_literal(instruction, "Map()".into()),
            Pack => self.emit_pack(instruction, "array"),
            Packmap => self.emit_pack(instruction, "map"),
            Packstruct => self.emit_pack(instruction, "struct"),
            Unpack => self.emit_unpack(instruction),
            Pickitem => self.binary_op(instruction, "get"),
            Setitem => self.emit_call(instruction, "set_item", 3, false),
            Append => self.emit_call(instruction, "append", 2, false),
            Reverseitems => self.emit_call(instruction, "reverse_items", 1, false),
            Remove => self.emit_call(instruction, "remove_item", 2, false),
            Clearitems => self.emit_call(instruction, "clear_items", 1, false),
            Popitem => self.emit_call(instruction, "pop_item", 2, true),
            Isnull => self.unary_op(instruction, |val| format!("is_null({val})")),
            Istype => self.emit_call(instruction, "is_type", 2, true),
            Haskey => self.binary_op(instruction, "has_key"),
            Keys => self.unary_op(instruction, |val| format!("keys({val})")),
            Values => self.unary_op(instruction, |val| format!("values({val})")),
            Size => self.unary_op(instruction, |val| format!("len({val})")),
            Convert => self.emit_convert(instruction),
            _ => return false,
        }

        true
    }
}
