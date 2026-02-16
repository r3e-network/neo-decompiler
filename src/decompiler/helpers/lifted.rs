use std::collections::{BTreeMap, HashSet};

use crate::instruction::{Instruction, OpCode};
use crate::manifest::ContractManifest;

use super::methods::{
    find_manifest_entry_method, initslot_argument_count_at, next_inferred_method_offset,
    offset_as_usize,
};

/// Build method argument counts keyed by method start offset.
///
/// This is shared by high-level and C# lift renderers so both views infer the
/// same entry stack arity for detached helper chunks.
#[must_use]
pub(in super::super) fn build_method_arg_counts_by_offset(
    instructions: &[Instruction],
    inferred_starts: &[usize],
    manifest: Option<&ContractManifest>,
) -> BTreeMap<usize, usize> {
    let mut counts = BTreeMap::new();

    let entry_offset = instructions.first().map(|ins| ins.offset).unwrap_or(0);
    let entry_method = manifest.and_then(|m| find_manifest_entry_method(m, entry_offset));
    let use_manifest_entry = entry_method
        .as_ref()
        .map(|(_, matched)| *matched)
        .unwrap_or(false);
    let entry_arg_count = if use_manifest_entry {
        entry_method
            .as_ref()
            .map(|(method, _)| method.parameters.len())
            .unwrap_or(0)
    } else {
        initslot_argument_count_at(instructions, entry_offset).unwrap_or(0)
    };
    counts.insert(entry_offset, entry_arg_count);

    if let Some(manifest) = manifest {
        for method in &manifest.abi.methods {
            if let Some(start) = offset_as_usize(method.offset) {
                counts.insert(start, method.parameters.len());
            }
        }
    }

    let manifest_offsets: HashSet<usize> = manifest
        .map(|m| {
            m.abi
                .methods
                .iter()
                .filter_map(|method| offset_as_usize(method.offset))
                .collect()
        })
        .unwrap_or_default();

    for start in inferred_starts {
        if manifest_offsets.contains(start) {
            continue;
        }
        let arg_count = initslot_argument_count_at(instructions, *start)
            .or_else(|| {
                infer_entry_stack_arg_count_for_inferred_start(
                    instructions,
                    inferred_starts,
                    *start,
                )
            })
            .unwrap_or(0);
        counts.insert(*start, arg_count);
    }

    counts
}

fn infer_entry_stack_arg_count_for_inferred_start(
    instructions: &[Instruction],
    inferred_starts: &[usize],
    start: usize,
) -> Option<usize> {
    let end = next_inferred_method_offset(inferred_starts, start)
        .or_else(|| instructions.last().map(|ins| ins.offset + 1))
        .unwrap_or(start);
    let lo = instructions.partition_point(|ins| ins.offset < start);
    let hi = instructions.partition_point(|ins| ins.offset < end);
    estimate_required_entry_stack_depth(&instructions[lo..hi])
}

fn estimate_required_entry_stack_depth(instructions: &[Instruction]) -> Option<usize> {
    let mut required_entry_depth = 0usize;
    let mut depth_delta = 0isize;
    let mut saw_supported_opcode = false;

    for instruction in instructions {
        if instruction.opcode == OpCode::Ret {
            break;
        }
        let Some((pops, pushes)) = fixed_stack_effect(instruction.opcode) else {
            break;
        };
        saw_supported_opcode = true;
        let needed = pops as isize - depth_delta;
        if needed > 0 {
            required_entry_depth = required_entry_depth.max(needed as usize);
        }
        depth_delta += pushes as isize - pops as isize;
    }

    saw_supported_opcode.then_some(required_entry_depth)
}

fn fixed_stack_effect(opcode: OpCode) -> Option<(usize, usize)> {
    use OpCode::*;

    match opcode {
        Pushint8 | Pushint16 | Pushint32 | Pushint64 | Pushint128 | Pushint256 | PushT | PushF
        | PushA | PushNull | Pushdata1 | Pushdata2 | Pushdata4 | PushM1 | Push0 | Push1 | Push2
        | Push3 | Push4 | Push5 | Push6 | Push7 | Push8 | Push9 | Push10 | Push11 | Push12
        | Push13 | Push14 | Push15 | Push16 | Newarray0 | Newstruct0 | Newmap | Ldsfld0
        | Ldsfld1 | Ldsfld2 | Ldsfld3 | Ldsfld4 | Ldsfld5 | Ldsfld6 | Ldsfld | Ldloc0 | Ldloc1
        | Ldloc2 | Ldloc3 | Ldloc4 | Ldloc5 | Ldloc6 | Ldloc | Ldarg0 | Ldarg1 | Ldarg2
        | Ldarg3 | Ldarg4 | Ldarg5 | Ldarg6 | Ldarg | Depth => Some((0, 1)),
        Nop | Initsslot | Initslot => Some((0, 0)),
        Drop | Stsfld0 | Stsfld1 | Stsfld2 | Stsfld3 | Stsfld4 | Stsfld5 | Stsfld6 | Stsfld
        | Stloc0 | Stloc1 | Stloc2 | Stloc3 | Stloc4 | Stloc5 | Stloc6 | Stloc | Starg0
        | Starg1 | Starg2 | Starg3 | Starg4 | Starg5 | Starg6 | Starg | Reverseitems
        | Clearitems => Some((1, 0)),
        Newbuffer | Isnull | Istype | Convert | Keys | Values | Size | Sign | Abs | Negate
        | Inc | Dec | Not | Nz | Sqrt | Newarray | NewarrayT | Newstruct | Invert => Some((1, 1)),
        Dup => Some((1, 2)),
        Nip => Some((2, 1)),
        Over | Tuck => Some((2, 3)),
        Swap => Some((2, 2)),
        Rot | Reverse3 => Some((3, 3)),
        Reverse4 => Some((4, 4)),
        Cat | Left | Right | And | Or | Xor | Equal | Notequal | Add | Sub | Mul | Div | Mod
        | Pow | Shl | Shr | Booland | Boolor | Numequal | Numnotequal | Lt | Le | Gt | Ge | Min
        | Max | Haskey | Pickitem | Popitem => Some((2, 1)),
        Append | Remove => Some((2, 0)),
        Substr | Modmul | Modpow | Within => Some((3, 1)),
        Memcpy | Setitem => Some((3, 0)),
        _ => None,
    }
}
