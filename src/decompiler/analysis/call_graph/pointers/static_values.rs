use super::*;

/// Resolve a static delegate slot whose complete write history is one
/// unambiguous constant PUSHA/DUP chain. Dynamic or conflicting writes stay
/// unresolved so callers do not receive a fabricated internal edge.
pub(super) fn resolve_constant_static_pointer_target(
    instructions: &[Instruction],
    slot: u8,
) -> Option<usize> {
    let mut target = None;
    let mut saw_store = false;
    for (index, instruction) in instructions.iter().enumerate() {
        if slot_store_domain(instruction) != Some(SlotDomain::Static(slot)) {
            continue;
        }
        saw_store = true;
        let source_index = previous_non_nop_index(instructions, index.checked_sub(1)?)?;
        let candidate = trace_constant_pointer_source(instructions, source_index)?;
        if target.is_some_and(|existing| existing != candidate) {
            return None;
        }
        target = Some(candidate);
    }
    if !saw_store {
        return None;
    }
    target
}

fn trace_constant_pointer_source(
    instructions: &[Instruction],
    mut source_index: usize,
) -> Option<usize> {
    loop {
        let instruction = instructions.get(source_index)?;
        match instruction.opcode {
            OpCode::Dup => {
                source_index = previous_non_nop_index(instructions, source_index.checked_sub(1)?)?;
            }
            OpCode::PushA => return pusha_absolute_target(instruction),
            _ => return None,
        }
    }
}
