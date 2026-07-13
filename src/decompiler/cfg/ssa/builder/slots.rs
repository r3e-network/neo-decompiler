//! Slot naming and static/local reaching-definition helpers.

use std::collections::BTreeMap;

use crate::instruction::{Instruction, OpCode, Operand};

use super::SsaVariable;

pub(super) type SlotState = BTreeMap<String, SsaVariable>;

pub(super) fn is_static_slot_name(name: &str) -> bool {
    name.strip_prefix("static").is_some_and(|index| {
        !index.is_empty() && index.chars().all(|character| character.is_ascii_digit())
    })
}

pub(super) fn static_load_index(instruction: &Instruction) -> Option<usize> {
    match instruction.opcode {
        OpCode::Ldsfld0 => Some(0),
        OpCode::Ldsfld1 => Some(1),
        OpCode::Ldsfld2 => Some(2),
        OpCode::Ldsfld3 => Some(3),
        OpCode::Ldsfld4 => Some(4),
        OpCode::Ldsfld5 => Some(5),
        OpCode::Ldsfld6 => Some(6),
        OpCode::Ldsfld => match instruction.operand {
            Some(Operand::U8(index)) => Some(usize::from(index)),
            _ => None,
        },
        _ => None,
    }
}

pub(super) fn static_store_index(instruction: &Instruction) -> Option<usize> {
    match instruction.opcode {
        OpCode::Stsfld0 => Some(0),
        OpCode::Stsfld1 => Some(1),
        OpCode::Stsfld2 => Some(2),
        OpCode::Stsfld3 => Some(3),
        OpCode::Stsfld4 => Some(4),
        OpCode::Stsfld5 => Some(5),
        OpCode::Stsfld6 => Some(6),
        OpCode::Stsfld => match instruction.operand {
            Some(Operand::U8(index)) => Some(usize::from(index)),
            _ => None,
        },
        _ => None,
    }
}

pub(super) fn absent_slot_value(name: &str) -> SsaVariable {
    if is_static_slot_name(name) {
        SsaVariable::initial(name.to_string())
    } else {
        SsaVariable::new("?".to_string(), 0)
    }
}

/// Derive the stable slot name for a load or store opcode.
pub(super) fn slot_name_for(op: OpCode, operand: &Option<Operand>) -> Option<String> {
    use OpCode::*;
    let (kind, idx): (&str, usize) = match op {
        Ldloc0 => ("loc", 0),
        Ldloc1 => ("loc", 1),
        Ldloc2 => ("loc", 2),
        Ldloc3 => ("loc", 3),
        Ldloc4 => ("loc", 4),
        Ldloc5 => ("loc", 5),
        Ldloc6 => ("loc", 6),
        Ldarg0 => ("arg", 0),
        Ldarg1 => ("arg", 1),
        Ldarg2 => ("arg", 2),
        Ldarg3 => ("arg", 3),
        Ldarg4 => ("arg", 4),
        Ldarg5 => ("arg", 5),
        Ldarg6 => ("arg", 6),
        Ldsfld0 => ("static", 0),
        Ldsfld1 => ("static", 1),
        Ldsfld2 => ("static", 2),
        Ldsfld3 => ("static", 3),
        Ldsfld4 => ("static", 4),
        Ldsfld5 => ("static", 5),
        Ldsfld6 => ("static", 6),
        Stloc0 => ("loc", 0),
        Stloc1 => ("loc", 1),
        Stloc2 => ("loc", 2),
        Stloc3 => ("loc", 3),
        Stloc4 => ("loc", 4),
        Stloc5 => ("loc", 5),
        Stloc6 => ("loc", 6),
        Starg0 => ("arg", 0),
        Starg1 => ("arg", 1),
        Starg2 => ("arg", 2),
        Starg3 => ("arg", 3),
        Starg4 => ("arg", 4),
        Starg5 => ("arg", 5),
        Starg6 => ("arg", 6),
        Stsfld0 => ("static", 0),
        Stsfld1 => ("static", 1),
        Stsfld2 => ("static", 2),
        Stsfld3 => ("static", 3),
        Stsfld4 => ("static", 4),
        Stsfld5 => ("static", 5),
        Stsfld6 => ("static", 6),
        Ldloc | Stloc => ("loc", indexed_slot(operand)?),
        Ldarg | Starg => ("arg", indexed_slot(operand)?),
        Ldsfld | Stsfld => ("static", indexed_slot(operand)?),
        _ => return None,
    };
    Some(format!("{kind}{idx}"))
}

fn indexed_slot(operand: &Option<Operand>) -> Option<usize> {
    match operand {
        Some(Operand::U8(index)) => Some(usize::from(*index)),
        _ => None,
    }
}

pub(super) fn requires_reaching_slot_definition(op: OpCode) -> bool {
    use OpCode::*;
    matches!(
        op,
        Ldloc0
            | Ldloc1
            | Ldloc2
            | Ldloc3
            | Ldloc4
            | Ldloc5
            | Ldloc6
            | Ldloc
            | Ldarg0
            | Ldarg1
            | Ldarg2
            | Ldarg3
            | Ldarg4
            | Ldarg5
            | Ldarg6
            | Ldarg
    )
}
