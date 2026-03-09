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
    let use_manifest_entry = entry_method.is_some();
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
    let mut simulated_stack: Vec<Option<usize>> = Vec::new();
    let mut saw_supported_opcode = false;

    for instruction in instructions {
        if instruction.opcode == OpCode::Ret {
            break;
        }
        let Some(effect) = stack_effect_for_arg_inference(instruction, &simulated_stack) else {
            break;
        };
        saw_supported_opcode = true;

        while simulated_stack.len() < effect.pops {
            simulated_stack.insert(0, None);
            required_entry_depth += 1;
        }
        for _ in 0..effect.pops {
            simulated_stack.pop();
        }
        for pushed in effect.pushes {
            simulated_stack.push(pushed);
        }
    }

    saw_supported_opcode.then_some(required_entry_depth)
}

struct StackEffect {
    pops: usize,
    pushes: Vec<Option<usize>>,
}

fn stack_effect_for_arg_inference(
    instruction: &Instruction,
    simulated_stack: &[Option<usize>],
) -> Option<StackEffect> {
    use OpCode::*;

    let literal_push = |value: usize| StackEffect {
        pops: 0,
        pushes: vec![Some(value)],
    };
    let unknown_push = || StackEffect {
        pops: 0,
        pushes: vec![None],
    };
    let unary_unknown = || StackEffect {
        pops: 1,
        pushes: vec![None],
    };
    let binary_unknown = || StackEffect {
        pops: 2,
        pushes: vec![None],
    };

    match instruction.opcode {
        Push0 => Some(literal_push(0)),
        Push1 => Some(literal_push(1)),
        Push2 => Some(literal_push(2)),
        Push3 => Some(literal_push(3)),
        Push4 => Some(literal_push(4)),
        Push5 => Some(literal_push(5)),
        Push6 => Some(literal_push(6)),
        Push7 => Some(literal_push(7)),
        Push8 => Some(literal_push(8)),
        Push9 => Some(literal_push(9)),
        Push10 => Some(literal_push(10)),
        Push11 => Some(literal_push(11)),
        Push12 => Some(literal_push(12)),
        Push13 => Some(literal_push(13)),
        Push14 => Some(literal_push(14)),
        Push15 => Some(literal_push(15)),
        Push16 => Some(literal_push(16)),
        Pushint8 | Pushint16 | Pushint32 | Pushint64 => match instruction.operand {
            Some(crate::instruction::Operand::I8(v)) if v >= 0 => Some(literal_push(v as usize)),
            Some(crate::instruction::Operand::I16(v)) if v >= 0 => Some(literal_push(v as usize)),
            Some(crate::instruction::Operand::I32(v)) if v >= 0 => Some(literal_push(v as usize)),
            Some(crate::instruction::Operand::I64(v)) if v >= 0 => Some(literal_push(v as usize)),
            _ => Some(unknown_push()),
        },
        Pushint128 | Pushint256 | PushT | PushF | PushA | PushNull | Pushdata1 | Pushdata2
        | Pushdata4 | PushM1 | Newarray0 | Newstruct0 | Newmap | Ldsfld0 | Ldsfld1 | Ldsfld2
        | Ldsfld3 | Ldsfld4 | Ldsfld5 | Ldsfld6 | Ldsfld | Ldloc0 | Ldloc1 | Ldloc2 | Ldloc3
        | Ldloc4 | Ldloc5 | Ldloc6 | Ldloc | Ldarg0 | Ldarg1 | Ldarg2 | Ldarg3 | Ldarg4
        | Ldarg5 | Ldarg6 | Ldarg | Depth => Some(unknown_push()),
        Nop | Initsslot | Initslot => Some(StackEffect {
            pops: 0,
            pushes: vec![],
        }),
        Drop | Stsfld0 | Stsfld1 | Stsfld2 | Stsfld3 | Stsfld4 | Stsfld5 | Stsfld6 | Stsfld
        | Stloc0 | Stloc1 | Stloc2 | Stloc3 | Stloc4 | Stloc5 | Stloc6 | Stloc | Starg0
        | Starg1 | Starg2 | Starg3 | Starg4 | Starg5 | Starg6 | Starg | Reverseitems
        | Clearitems => Some(StackEffect {
            pops: 1,
            pushes: vec![],
        }),
        Newbuffer | Isnull | Istype | Convert | Keys | Values | Size | Sign | Abs | Negate
        | Inc | Dec | Not | Nz | Sqrt | Newarray | NewarrayT | Newstruct | Invert => {
            Some(unary_unknown())
        }
        Dup => {
            let top = simulated_stack.last().copied().flatten();
            Some(StackEffect {
                pops: 1,
                pushes: vec![top, top],
            })
        }
        Nip => Some(StackEffect {
            pops: 2,
            pushes: vec![None],
        }),
        Over | Tuck => Some(StackEffect {
            pops: 2,
            pushes: vec![None, None, None],
        }),
        Swap => Some(StackEffect {
            pops: 2,
            pushes: vec![None, None],
        }),
        Rot | Reverse3 => Some(StackEffect {
            pops: 3,
            pushes: vec![None, None, None],
        }),
        Reverse4 => Some(StackEffect {
            pops: 4,
            pushes: vec![None, None, None, None],
        }),
        Cat | Left | Right | And | Or | Xor | Equal | Notequal | Add | Sub | Mul | Div | Mod
        | Pow | Shl | Shr | Booland | Boolor | Numequal | Numnotequal | Lt | Le | Gt | Ge | Min
        | Max | Haskey | Pickitem | Popitem => Some(binary_unknown()),
        Append | Remove => Some(StackEffect {
            pops: 2,
            pushes: vec![],
        }),
        Substr | Modmul | Modpow | Within => Some(StackEffect {
            pops: 3,
            pushes: vec![None],
        }),
        Memcpy | Setitem => Some(StackEffect {
            pops: 3,
            pushes: vec![],
        }),
        Pack | Packmap | Packstruct => {
            let count = simulated_stack.last().copied().flatten()?;
            Some(StackEffect {
                pops: count + 1,
                pushes: vec![None],
            })
        }
        Syscall => match instruction.operand {
            Some(crate::instruction::Operand::Syscall(hash)) => {
                let info = crate::syscalls::lookup(hash);
                let pops = info.map_or(0usize, |syscall| syscall.param_count as usize);
                let pushes = if info.is_some_and(|syscall| syscall.returns_value) {
                    vec![None]
                } else {
                    vec![]
                };
                Some(StackEffect { pops, pushes })
            }
            _ => None,
        },
        _ => None,
    }
}
