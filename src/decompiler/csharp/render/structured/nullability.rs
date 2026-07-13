use std::collections::BTreeSet;

use crate::instruction::{Instruction, OpCode, Operand};

pub(super) fn null_checked_argument_indices(
    instructions: &[Instruction],
    start: usize,
    end: usize,
) -> BTreeSet<usize> {
    let method = instructions
        .iter()
        .filter(|instruction| instruction.offset >= start && instruction.offset < end)
        .collect::<Vec<_>>();
    let mut checked = BTreeSet::new();
    for (index, instruction) in method.iter().enumerate() {
        if instruction.opcode != OpCode::Isnull {
            continue;
        }
        if let Some(source) = null_checked_argument_source(&method, index) {
            checked.insert(source);
        }
    }
    checked
}

/// Resolve only direct slot aliases feeding ISNULL. This intentionally stops
/// at computed values and control-flow joins; widening a signature is safer
/// than claiming that an arbitrary expression has a nullable ABI type.
fn null_checked_argument_source(method: &[&Instruction], isnull_index: usize) -> Option<usize> {
    let value_index = isnull_index.checked_sub(1).and_then(|index| {
        (method[index].opcode == OpCode::Dup)
            .then(|| index.checked_sub(1))
            .flatten()
            .or(Some(index))
    })?;
    trace_argument_value(method, value_index, 0)
}

fn trace_argument_value(
    method: &[&Instruction],
    value_index: usize,
    depth: usize,
) -> Option<usize> {
    if depth >= 8 {
        return None;
    }
    if let Some(argument) = argument_load_index(method[value_index]) {
        return Some(argument);
    }
    let local = local_load_index(method[value_index])?;
    let store_index = (0..value_index)
        .rev()
        .find(|index| local_store_index(method[*index]) == Some(local))?;
    let stored_value = store_index.checked_sub(1).and_then(|index| {
        (method[index].opcode == OpCode::Dup)
            .then(|| index.checked_sub(1))
            .flatten()
            .or(Some(index))
    })?;
    trace_argument_value(method, stored_value, depth + 1)
}

fn local_load_index(instruction: &Instruction) -> Option<usize> {
    match instruction.opcode {
        OpCode::Ldloc0 => Some(0),
        OpCode::Ldloc1 => Some(1),
        OpCode::Ldloc2 => Some(2),
        OpCode::Ldloc3 => Some(3),
        OpCode::Ldloc4 => Some(4),
        OpCode::Ldloc5 => Some(5),
        OpCode::Ldloc6 => Some(6),
        OpCode::Ldloc => match instruction.operand {
            Some(Operand::U8(index)) => Some(usize::from(index)),
            _ => None,
        },
        _ => None,
    }
}

fn local_store_index(instruction: &Instruction) -> Option<usize> {
    match instruction.opcode {
        OpCode::Stloc0 => Some(0),
        OpCode::Stloc1 => Some(1),
        OpCode::Stloc2 => Some(2),
        OpCode::Stloc3 => Some(3),
        OpCode::Stloc4 => Some(4),
        OpCode::Stloc5 => Some(5),
        OpCode::Stloc6 => Some(6),
        OpCode::Stloc => match instruction.operand {
            Some(Operand::U8(index)) => Some(usize::from(index)),
            _ => None,
        },
        _ => None,
    }
}

fn argument_load_index(instruction: &Instruction) -> Option<usize> {
    match instruction.opcode {
        OpCode::Ldarg0 => Some(0),
        OpCode::Ldarg1 => Some(1),
        OpCode::Ldarg2 => Some(2),
        OpCode::Ldarg3 => Some(3),
        OpCode::Ldarg4 => Some(4),
        OpCode::Ldarg5 => Some(5),
        OpCode::Ldarg6 => Some(6),
        OpCode::Ldarg => match instruction.operand {
            Some(Operand::U8(index)) => Some(usize::from(index)),
            _ => None,
        },
        _ => None,
    }
}
