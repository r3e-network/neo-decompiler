use crate::instruction::{Instruction, OpCode};

use super::super::{HighLevelEmitter, SlotKind};

impl HighLevelEmitter {
    pub(super) fn try_emit_slot_ops(&mut self, instruction: &Instruction) -> bool {
        use OpCode::*;

        match instruction.opcode {
            Initsslot => self.emit_init_static_slots(instruction),
            Initslot => self.emit_init_slots(instruction),
            Ldloc0 => self.emit_load_slot(instruction, SlotKind::Local, 0),
            Ldloc1 => self.emit_load_slot(instruction, SlotKind::Local, 1),
            Ldloc2 => self.emit_load_slot(instruction, SlotKind::Local, 2),
            Ldloc3 => self.emit_load_slot(instruction, SlotKind::Local, 3),
            Ldloc4 => self.emit_load_slot(instruction, SlotKind::Local, 4),
            Ldloc5 => self.emit_load_slot(instruction, SlotKind::Local, 5),
            Ldloc6 => self.emit_load_slot(instruction, SlotKind::Local, 6),
            Ldloc => self.emit_load_slot_from_operand(instruction, SlotKind::Local),
            Stloc0 => self.emit_store_slot(instruction, SlotKind::Local, 0),
            Stloc1 => self.emit_store_slot(instruction, SlotKind::Local, 1),
            Stloc2 => self.emit_store_slot(instruction, SlotKind::Local, 2),
            Stloc3 => self.emit_store_slot(instruction, SlotKind::Local, 3),
            Stloc4 => self.emit_store_slot(instruction, SlotKind::Local, 4),
            Stloc5 => self.emit_store_slot(instruction, SlotKind::Local, 5),
            Stloc6 => self.emit_store_slot(instruction, SlotKind::Local, 6),
            Stloc => self.emit_store_slot_from_operand(instruction, SlotKind::Local),
            Ldarg0 => self.emit_load_slot(instruction, SlotKind::Argument, 0),
            Ldarg1 => self.emit_load_slot(instruction, SlotKind::Argument, 1),
            Ldarg2 => self.emit_load_slot(instruction, SlotKind::Argument, 2),
            Ldarg3 => self.emit_load_slot(instruction, SlotKind::Argument, 3),
            Ldarg4 => self.emit_load_slot(instruction, SlotKind::Argument, 4),
            Ldarg5 => self.emit_load_slot(instruction, SlotKind::Argument, 5),
            Ldarg6 => self.emit_load_slot(instruction, SlotKind::Argument, 6),
            Ldarg => self.emit_load_slot_from_operand(instruction, SlotKind::Argument),
            Starg0 => self.emit_store_slot(instruction, SlotKind::Argument, 0),
            Starg1 => self.emit_store_slot(instruction, SlotKind::Argument, 1),
            Starg2 => self.emit_store_slot(instruction, SlotKind::Argument, 2),
            Starg3 => self.emit_store_slot(instruction, SlotKind::Argument, 3),
            Starg4 => self.emit_store_slot(instruction, SlotKind::Argument, 4),
            Starg5 => self.emit_store_slot(instruction, SlotKind::Argument, 5),
            Starg6 => self.emit_store_slot(instruction, SlotKind::Argument, 6),
            Starg => self.emit_store_slot_from_operand(instruction, SlotKind::Argument),
            Ldsfld0 => self.emit_load_slot(instruction, SlotKind::Static, 0),
            Ldsfld1 => self.emit_load_slot(instruction, SlotKind::Static, 1),
            Ldsfld2 => self.emit_load_slot(instruction, SlotKind::Static, 2),
            Ldsfld3 => self.emit_load_slot(instruction, SlotKind::Static, 3),
            Ldsfld4 => self.emit_load_slot(instruction, SlotKind::Static, 4),
            Ldsfld5 => self.emit_load_slot(instruction, SlotKind::Static, 5),
            Ldsfld6 => self.emit_load_slot(instruction, SlotKind::Static, 6),
            Ldsfld => self.emit_load_slot_from_operand(instruction, SlotKind::Static),
            Stsfld0 => self.emit_store_slot(instruction, SlotKind::Static, 0),
            Stsfld1 => self.emit_store_slot(instruction, SlotKind::Static, 1),
            Stsfld2 => self.emit_store_slot(instruction, SlotKind::Static, 2),
            Stsfld3 => self.emit_store_slot(instruction, SlotKind::Static, 3),
            Stsfld4 => self.emit_store_slot(instruction, SlotKind::Static, 4),
            Stsfld5 => self.emit_store_slot(instruction, SlotKind::Static, 5),
            Stsfld6 => self.emit_store_slot(instruction, SlotKind::Static, 6),
            Stsfld => self.emit_store_slot_from_operand(instruction, SlotKind::Static),
            _ => return false,
        }

        true
    }
}
