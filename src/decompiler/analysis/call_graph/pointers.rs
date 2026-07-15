use super::*;
mod arguments;
mod static_values;

pub(crate) use arguments::resolve_ldarg_calla_targets;

pub(crate) fn calla_target_from_pusha(instructions: &[Instruction], index: usize) -> Option<usize> {
    let mut cursor = index.checked_sub(1)?;
    loop {
        let prev = instructions.get(cursor)?;
        if prev.opcode == OpCode::Nop {
            cursor = cursor.checked_sub(1)?;
            continue;
        }
        return trace_pointer_target_from_value_source(instructions, cursor);
    }
}

fn pusha_absolute_target(instruction: &Instruction) -> Option<usize> {
    // PUSHA's operand is decoded as a signed I32 relative offset (the
    // generated opcode table uses `OperandEncoding::I32`); no u32→i32
    // reinterpretation is needed.
    let delta = match instruction.operand {
        Some(Operand::I32(value)) => value as isize,
        _ => return None,
    };
    instruction.offset.checked_add_signed(delta)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SlotDomain {
    Local(u8),
    Static(u8),
}

fn local_load_index(instruction: &Instruction) -> Option<u8> {
    match instruction.opcode {
        OpCode::Ldloc0 => Some(0),
        OpCode::Ldloc1 => Some(1),
        OpCode::Ldloc2 => Some(2),
        OpCode::Ldloc3 => Some(3),
        OpCode::Ldloc4 => Some(4),
        OpCode::Ldloc5 => Some(5),
        OpCode::Ldloc6 => Some(6),
        OpCode::Ldloc => match instruction.operand {
            Some(Operand::U8(index)) => Some(index),
            _ => None,
        },
        _ => None,
    }
}

fn static_load_index(instruction: &Instruction) -> Option<u8> {
    match instruction.opcode {
        OpCode::Ldsfld0 => Some(0),
        OpCode::Ldsfld1 => Some(1),
        OpCode::Ldsfld2 => Some(2),
        OpCode::Ldsfld3 => Some(3),
        OpCode::Ldsfld4 => Some(4),
        OpCode::Ldsfld5 => Some(5),
        OpCode::Ldsfld6 => Some(6),
        OpCode::Ldsfld => match instruction.operand {
            Some(Operand::U8(index)) => Some(index),
            _ => None,
        },
        _ => None,
    }
}

fn arg_load_index(instruction: &Instruction) -> Option<u8> {
    match instruction.opcode {
        OpCode::Ldarg0 => Some(0),
        OpCode::Ldarg1 => Some(1),
        OpCode::Ldarg2 => Some(2),
        OpCode::Ldarg3 => Some(3),
        OpCode::Ldarg4 => Some(4),
        OpCode::Ldarg5 => Some(5),
        OpCode::Ldarg6 => Some(6),
        OpCode::Ldarg => match instruction.operand {
            Some(Operand::U8(index)) => Some(index),
            _ => None,
        },
        _ => None,
    }
}

fn slot_store_domain(instruction: &Instruction) -> Option<SlotDomain> {
    match instruction.opcode {
        OpCode::Stloc0 => Some(SlotDomain::Local(0)),
        OpCode::Stloc1 => Some(SlotDomain::Local(1)),
        OpCode::Stloc2 => Some(SlotDomain::Local(2)),
        OpCode::Stloc3 => Some(SlotDomain::Local(3)),
        OpCode::Stloc4 => Some(SlotDomain::Local(4)),
        OpCode::Stloc5 => Some(SlotDomain::Local(5)),
        OpCode::Stloc6 => Some(SlotDomain::Local(6)),
        OpCode::Stloc => match instruction.operand {
            Some(Operand::U8(index)) => Some(SlotDomain::Local(index)),
            _ => None,
        },
        OpCode::Stsfld0 => Some(SlotDomain::Static(0)),
        OpCode::Stsfld1 => Some(SlotDomain::Static(1)),
        OpCode::Stsfld2 => Some(SlotDomain::Static(2)),
        OpCode::Stsfld3 => Some(SlotDomain::Static(3)),
        OpCode::Stsfld4 => Some(SlotDomain::Static(4)),
        OpCode::Stsfld5 => Some(SlotDomain::Static(5)),
        OpCode::Stsfld6 => Some(SlotDomain::Static(6)),
        OpCode::Stsfld => match instruction.operand {
            Some(Operand::U8(index)) => Some(SlotDomain::Static(index)),
            _ => None,
        },
        _ => None,
    }
}

fn resolve_slot_pointer_target(
    instructions: &[Instruction],
    before_index: usize,
    domain: SlotDomain,
) -> Option<usize> {
    if let Some(store_index) = find_slot_store_before(instructions, before_index, domain) {
        let source_index = previous_non_nop_index(instructions, store_index.checked_sub(1)?)?;
        return trace_pointer_target_from_value_source(instructions, source_index);
    }

    // Compiler-generated static initializers commonly appear after all public
    // methods. A backward-only search therefore misses a function pointer that
    // is initialized once in `_initialize`. Recover only an unambiguous,
    // direct constant PUSHA assignment; dynamic or multiply-written statics
    // remain unresolved rather than being guessed.
    match domain {
        SlotDomain::Static(slot) => {
            static_values::resolve_constant_static_pointer_target(instructions, slot)
        }
        SlotDomain::Local(_) => None,
    }
}

fn trace_pointer_target_from_value_source(
    instructions: &[Instruction],
    mut source_index: usize,
) -> Option<usize> {
    loop {
        let instruction = instructions.get(source_index)?;
        if instruction.opcode == OpCode::Dup {
            source_index = previous_non_nop_index(instructions, source_index.checked_sub(1)?)?;
            continue;
        }
        if instruction.opcode == OpCode::PushA {
            return pusha_absolute_target(instruction);
        }
        if instruction.opcode == OpCode::Pickitem {
            return resolve_pickitem_pointer_target(instructions, source_index);
        }

        let domain = if let Some(slot) = local_load_index(instruction) {
            SlotDomain::Local(slot)
        } else {
            SlotDomain::Static(static_load_index(instruction)?)
        };

        return resolve_slot_pointer_target(instructions, source_index, domain);
    }
}

// ---------------------------------------------------------------------------
// Second-pass inter-procedural CALLA resolution for LDARG patterns
// ---------------------------------------------------------------------------

/// Check if a CALLA instruction ultimately loads its pointer from an argument
/// slot and return that argument index.
pub(crate) fn calla_ldarg_index(instructions: &[Instruction], calla_index: usize) -> Option<u8> {
    let producer_index = previous_non_nop_index(instructions, calla_index.checked_sub(1)?)?;
    trace_argument_index_from_value_source(instructions, producer_index)
}

fn trace_argument_index_from_value_source(
    instructions: &[Instruction],
    mut source_index: usize,
) -> Option<u8> {
    loop {
        let instruction = instructions.get(source_index)?;
        if instruction.opcode == OpCode::Dup {
            source_index = previous_non_nop_index(instructions, source_index.checked_sub(1)?)?;
            continue;
        }
        if let Some(arg_index) = arg_load_index(instruction) {
            return Some(arg_index);
        }

        let domain = if let Some(slot) = local_load_index(instruction) {
            SlotDomain::Local(slot)
        } else {
            SlotDomain::Static(static_load_index(instruction)?)
        };

        let store_index = find_slot_store_before(instructions, source_index, domain)?;
        source_index = previous_non_nop_index(instructions, store_index.checked_sub(1)?)?;
    }
}

fn resolve_pickitem_pointer_target(
    instructions: &[Instruction],
    pickitem_index: usize,
) -> Option<usize> {
    let index_source = previous_non_nop_index(instructions, pickitem_index.checked_sub(1)?)?;
    let array_source_index = previous_non_nop_index(instructions, index_source.checked_sub(1)?)?;
    let domain = trace_container_domain_from_value_source(instructions, array_source_index)?;

    let scan_start = match domain {
        SlotDomain::Local(_) => find_resolution_start_index(instructions, pickitem_index),
        SlotDomain::Static(_) => 0,
    };

    let mut resolved_target = None;
    for (index, instruction) in instructions
        .iter()
        .enumerate()
        .take(pickitem_index)
        .skip(scan_start)
    {
        if instruction.opcode != OpCode::Append {
            continue;
        }
        let item_index = trace_stack_value_producer_before(instructions, index, 0)?;
        let array_index = trace_stack_value_producer_before(instructions, index, 1)?;
        let Some(array_domain) =
            trace_container_domain_from_value_source(instructions, array_index)
        else {
            continue;
        };
        if array_domain != domain {
            continue;
        }
        let target = trace_pointer_target_from_value_source(instructions, item_index)?;
        if let Some(existing) = resolved_target {
            if existing != target {
                return None;
            }
        } else {
            resolved_target = Some(target);
        }
    }
    resolved_target
}

fn trace_container_domain_from_value_source(
    instructions: &[Instruction],
    mut source_index: usize,
) -> Option<SlotDomain> {
    loop {
        let instruction = instructions.get(source_index)?;
        if instruction.opcode == OpCode::Dup {
            source_index = previous_non_nop_index(instructions, source_index.checked_sub(1)?)?;
            continue;
        }
        if let Some(slot) = local_load_index(instruction) {
            let domain = SlotDomain::Local(slot);
            let Some(store_index) = find_slot_store_before(instructions, source_index, domain)
            else {
                return Some(domain);
            };
            let source = previous_non_nop_index(instructions, store_index.checked_sub(1)?)?;
            let source_instruction = instructions.get(source)?;
            if source_instruction.opcode == OpCode::Dup {
                source_index = previous_non_nop_index(instructions, source.checked_sub(1)?)?;
                continue;
            }
            if local_load_index(source_instruction).is_some()
                || static_load_index(source_instruction).is_some()
            {
                source_index = source;
                continue;
            }
            return Some(domain);
        }
        if let Some(slot) = static_load_index(instruction) {
            let domain = SlotDomain::Static(slot);
            let Some(store_index) = find_slot_store_before(instructions, source_index, domain)
            else {
                return Some(domain);
            };
            let source = previous_non_nop_index(instructions, store_index.checked_sub(1)?)?;
            let source_instruction = instructions.get(source)?;
            if source_instruction.opcode == OpCode::Dup {
                source_index = previous_non_nop_index(instructions, source.checked_sub(1)?)?;
                continue;
            }
            if local_load_index(source_instruction).is_some()
                || static_load_index(source_instruction).is_some()
            {
                source_index = source;
                continue;
            }
            return Some(domain);
        }
        return None;
    }
}

fn trace_stack_value_producer_before(
    instructions: &[Instruction],
    before_index: usize,
    mut depth: usize,
) -> Option<usize> {
    for index in (0..before_index).rev() {
        let instruction = instructions.get(index)?;
        let (pops, pushes) = stack_effect(instruction)?;
        if depth < pushes {
            return Some(index);
        }
        depth = depth.checked_add(pops)?.checked_sub(pushes)?;
    }
    None
}

fn stack_effect(instruction: &Instruction) -> Option<(usize, usize)> {
    use OpCode::*;
    let opcode = instruction.opcode;
    match opcode {
        Nop => Some((0, 0)),
        PushA | PushNull | PushT | PushF | PushM1 | Push0 | Push1 | Push2 | Push3 | Push4
        | Push5 | Push6 | Push7 | Push8 | Push9 | Push10 | Push11 | Push12 | Push13 | Push14
        | Push15 | Push16 | Pushint8 | Pushint16 | Pushint32 | Pushint64 | Pushint128
        | Pushint256 | Pushdata1 | Pushdata2 | Pushdata4 | Newarray0 | Newmap | Newstruct0
        | Ldloc0 | Ldloc1 | Ldloc2 | Ldloc3 | Ldloc4 | Ldloc5 | Ldloc6 | Ldloc | Ldarg0
        | Ldarg1 | Ldarg2 | Ldarg3 | Ldarg4 | Ldarg5 | Ldarg6 | Ldarg | Ldsfld0 | Ldsfld1
        | Ldsfld2 | Ldsfld3 | Ldsfld4 | Ldsfld5 | Ldsfld6 | Ldsfld => Some((0, 1)),
        Stloc0 | Stloc1 | Stloc2 | Stloc3 | Stloc4 | Stloc5 | Stloc6 | Stloc | Starg0 | Starg1
        | Starg2 | Starg3 | Starg4 | Starg5 | Starg6 | Starg | Stsfld0 | Stsfld1 | Stsfld2
        | Stsfld3 | Stsfld4 | Stsfld5 | Stsfld6 | Stsfld => Some((1, 0)),
        Append => Some((2, 0)),
        Pickitem => Some((2, 1)),
        Dup => Some((1, 2)),
        _ => None,
    }
}

fn find_resolution_start_index(instructions: &[Instruction], before_index: usize) -> usize {
    for index in (0..before_index).rev() {
        if let Some(instruction) = instructions.get(index) {
            if is_pointer_resolution_boundary(instruction.opcode) {
                return index + 1;
            }
        }
    }
    0
}

fn find_slot_store_before(
    instructions: &[Instruction],
    before_index: usize,
    domain: SlotDomain,
) -> Option<usize> {
    for index in (0..before_index).rev() {
        let instruction = instructions.get(index)?;
        if matches!(domain, SlotDomain::Local(_))
            && is_pointer_resolution_boundary(instruction.opcode)
        {
            return None;
        }
        if slot_store_domain(instruction) == Some(domain) {
            return Some(index);
        }
    }
    None
}

fn is_pointer_resolution_boundary(opcode: OpCode) -> bool {
    matches!(
        opcode,
        OpCode::Ret
            | OpCode::Throw
            | OpCode::Abort
            | OpCode::Abortmsg
            | OpCode::Initslot
            | OpCode::Initsslot
    )
}

fn previous_non_nop_index(instructions: &[Instruction], mut index: usize) -> Option<usize> {
    loop {
        let instruction = instructions.get(index)?;
        if instruction.opcode != OpCode::Nop {
            return Some(index);
        }
        index = index.checked_sub(1)?;
    }
}

/// Extract the argument count from an INITSLOT instruction at the given method offset.
pub(crate) fn initslot_arg_count_at(
    instructions: &[Instruction],
    method_offset: usize,
) -> Option<usize> {
    instructions
        .iter()
        .find(|i| i.offset == method_offset && i.opcode == OpCode::Initslot)
        .and_then(|i| match &i.operand {
            Some(Operand::Bytes(bytes)) if bytes.len() >= 2 => Some(bytes[1] as usize),
            _ => None,
        })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CallArgSource {
    Target(usize),
    PassThrough(u8),
}

/// Trace backwards from a CALL instruction to find the source of the
/// `arg_index`-th argument (0-indexed).
///
/// Neo VM pops arguments top-first: the top of stack becomes arg0, the next
/// item becomes arg1, etc. So `arg0` is the last item pushed (0 items to skip)
/// and `arg N` requires skipping N single-push instructions.
pub(crate) fn trace_call_arg_source(
    instructions: &[Instruction],
    call_index: usize,
    arg_index: u8,
    callee_arg_count: usize,
) -> Option<CallArgSource> {
    if (arg_index as usize) >= callee_arg_count {
        return None;
    }
    let call_instruction = instructions.get(call_index)?;
    let skip_count = if call_instruction.opcode == OpCode::CallA {
        arg_index as usize + 1
    } else {
        arg_index as usize
    };

    let mut cursor = call_index.checked_sub(1)?;
    let mut remaining = skip_count;

    loop {
        let instr = instructions.get(cursor)?;
        if instr.opcode == OpCode::Nop {
            cursor = cursor.checked_sub(1)?;
            continue;
        }

        if remaining == 0 {
            if instr.opcode == OpCode::PushA {
                return pusha_absolute_target(instr).map(CallArgSource::Target);
            }
            if let Some(slot) = local_load_index(instr) {
                return resolve_slot_pointer_target(instructions, cursor, SlotDomain::Local(slot))
                    .map(CallArgSource::Target)
                    .or_else(|| {
                        trace_argument_index_from_value_source(instructions, cursor)
                            .map(CallArgSource::PassThrough)
                    });
            }
            if let Some(slot) = static_load_index(instr) {
                return resolve_slot_pointer_target(instructions, cursor, SlotDomain::Static(slot))
                    .map(CallArgSource::Target)
                    .or_else(|| {
                        trace_argument_index_from_value_source(instructions, cursor)
                            .map(CallArgSource::PassThrough)
                    });
            }
            return arg_load_index(instr).map(CallArgSource::PassThrough);
        }

        remaining -= 1;
        cursor = cursor.checked_sub(1)?;
    }
}
