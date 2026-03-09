//! Call graph construction for Neo N3 scripts.

// Bytecode offset arithmetic requires isize↔usize casts for signed jump deltas.
// NEF scripts are bounded (~1 MB), so these conversions are structurally safe.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::instruction::{Instruction, OpCode, Operand};
use crate::manifest::ContractManifest;
use crate::nef::NefFile;
use crate::{syscalls, util};

use super::{MethodRef, MethodTable};

/// A resolved call target extracted from the instruction stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum CallTarget {
    /// Direct call into the same script (CALL/CALL_L).
    Internal {
        /// Callee method resolved from the target offset.
        method: MethodRef,
    },
    /// Call to an entry in the NEF method-token table (CALLT).
    MethodToken {
        /// Index into the NEF `method_tokens` table.
        index: u16,
        /// Script hash (little-endian) for the called contract.
        hash_le: String,
        /// Script hash (big-endian) for the called contract.
        hash_be: String,
        /// Target method name.
        method: String,
        /// Declared parameter count.
        parameters_count: u16,
        /// Whether the target method has a return value.
        has_return_value: bool,
        /// Call flags bitfield.
        call_flags: u8,
    },
    /// System call (SYSCALL).
    Syscall {
        /// Syscall identifier (little-endian u32).
        hash: u32,
        /// Resolved syscall name when known.
        name: Option<String>,
        /// Whether the syscall is known to push a value.
        returns_value: bool,
    },
    /// Indirect call (e.g., CALLA) where the destination cannot be resolved statically.
    Indirect {
        /// Opcode mnemonic (`CALLA` or similar).
        opcode: String,
        /// Optional operand value (when present).
        operand: Option<u16>,
    },
    /// A CALL/CALL_L target that could not be resolved to a valid offset.
    UnresolvedInternal {
        /// Computed target offset (may be negative when malformed).
        target: isize,
    },
}

/// One call edge in the call graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CallEdge {
    /// Caller method containing the call instruction.
    pub caller: MethodRef,
    /// Bytecode offset of the call instruction.
    pub call_offset: usize,
    /// Opcode mnemonic of the call instruction (e.g., `CALL_L`, `SYSCALL`).
    pub opcode: String,
    /// Resolved target.
    pub target: CallTarget,
}

/// Call graph for a decompiled script.
#[derive(Debug, Clone, Default, Serialize)]
pub struct CallGraph {
    /// Known methods (manifest-defined plus synthetic internal targets).
    pub methods: Vec<MethodRef>,
    /// Call edges extracted from the instruction stream.
    pub edges: Vec<CallEdge>,
}

/// Build a call graph for the provided instruction stream.
#[must_use]
pub fn build_call_graph(
    nef: &NefFile,
    instructions: &[Instruction],
    manifest: Option<&ContractManifest>,
) -> CallGraph {
    let table = MethodTable::new(instructions, manifest);
    let mut methods: BTreeMap<usize, MethodRef> = table
        .spans()
        .iter()
        .map(|span| (span.method.offset, span.method.clone()))
        .collect();

    let mut edges = Vec::new();
    for (index, instr) in instructions.iter().enumerate() {
        match instr.opcode {
            OpCode::Syscall => {
                let Some(Operand::Syscall(hash)) = instr.operand else {
                    continue;
                };
                let info = syscalls::lookup(hash);
                edges.push(CallEdge {
                    caller: table.method_for_offset(instr.offset),
                    call_offset: instr.offset,
                    opcode: instr.opcode.to_string(),
                    target: CallTarget::Syscall {
                        hash,
                        name: info.map(|i| i.name.to_string()),
                        returns_value: info.map(|i| i.returns_value).unwrap_or(true),
                    },
                });
            }
            OpCode::Call | OpCode::Call_L => {
                let caller = table.method_for_offset(instr.offset);
                match relative_target_isize(instr) {
                    Some(target) if target >= 0 => {
                        let target = target as usize;
                        let callee = table.resolve_internal_target(target);
                        methods.insert(callee.offset, callee.clone());
                        edges.push(CallEdge {
                            caller,
                            call_offset: instr.offset,
                            opcode: instr.opcode.to_string(),
                            target: CallTarget::Internal { method: callee },
                        });
                    }
                    Some(target) => edges.push(CallEdge {
                        caller,
                        call_offset: instr.offset,
                        opcode: instr.opcode.to_string(),
                        target: CallTarget::UnresolvedInternal { target },
                    }),
                    None => edges.push(CallEdge {
                        caller,
                        call_offset: instr.offset,
                        opcode: instr.opcode.to_string(),
                        target: CallTarget::UnresolvedInternal { target: -1 },
                    }),
                }
            }
            OpCode::CallT => {
                let Some(Operand::U16(index)) = instr.operand else {
                    continue;
                };
                let token = nef.method_tokens.get(index as usize);
                if let Some(token) = token {
                    edges.push(CallEdge {
                        caller: table.method_for_offset(instr.offset),
                        call_offset: instr.offset,
                        opcode: instr.opcode.to_string(),
                        target: CallTarget::MethodToken {
                            index,
                            hash_le: util::format_hash(&token.hash),
                            hash_be: util::format_hash_be(&token.hash),
                            method: token.method.clone(),
                            parameters_count: token.parameters_count,
                            has_return_value: token.has_return_value,
                            call_flags: token.call_flags,
                        },
                    });
                } else {
                    edges.push(CallEdge {
                        caller: table.method_for_offset(instr.offset),
                        call_offset: instr.offset,
                        opcode: instr.opcode.to_string(),
                        target: CallTarget::Indirect {
                            opcode: instr.opcode.to_string(),
                            operand: Some(index),
                        },
                    });
                }
            }
            OpCode::CallA => {
                // CALLA takes no operand — it pops a Pointer from the stack.
                // Resolve direct PUSHA + CALLA sequences to internal call edges.
                let caller = table.method_for_offset(instr.offset);
                if let Some(target) = calla_target_from_pusha(instructions, index) {
                    let callee = table.resolve_internal_target(target);
                    methods.insert(callee.offset, callee.clone());
                    edges.push(CallEdge {
                        caller,
                        call_offset: instr.offset,
                        opcode: instr.opcode.to_string(),
                        target: CallTarget::Internal { method: callee },
                    });
                } else {
                    edges.push(CallEdge {
                        caller,
                        call_offset: instr.offset,
                        opcode: instr.opcode.to_string(),
                        target: CallTarget::Indirect {
                            opcode: instr.opcode.to_string(),
                            operand: None,
                        },
                    });
                }
            }
            _ => {}
        }
    }

    // Second pass: resolve CALLA targets that load function pointers from
    // argument slots (LDARG) by tracing back through callers.
    resolve_ldarg_calla_targets(instructions, &mut edges, &table, &mut methods);

    CallGraph {
        methods: methods.into_values().collect(),
        edges,
    }
}

fn relative_target_isize(instr: &Instruction) -> Option<isize> {
    let delta = match &instr.operand {
        Some(Operand::Jump(v)) => *v as isize,
        Some(Operand::Jump32(v)) => *v as isize,
        _ => return None,
    };
    Some(instr.offset as isize + delta)
}

pub(super) fn calla_target_from_pusha(instructions: &[Instruction], index: usize) -> Option<usize> {
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
    let delta = match instruction.operand {
        Some(Operand::U32(value)) => i32::from_le_bytes(value.to_le_bytes()) as isize,
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
    let store_index = find_slot_store_before(instructions, before_index, domain)?;
    let source_index = previous_non_nop_index(instructions, store_index.checked_sub(1)?)?;
    trace_pointer_target_from_value_source(instructions, source_index)
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
        } else if let Some(slot) = static_load_index(instruction) {
            SlotDomain::Static(slot)
        } else {
            return None;
        };

        let store_index = find_slot_store_before(instructions, source_index, domain)?;
        source_index = previous_non_nop_index(instructions, store_index.checked_sub(1)?)?;
    }
}

// ---------------------------------------------------------------------------
// Second-pass inter-procedural CALLA resolution for LDARG patterns
// ---------------------------------------------------------------------------

/// Check if a CALLA instruction ultimately loads its pointer from an argument
/// slot and return that argument index.
pub(super) fn calla_ldarg_index(instructions: &[Instruction], calla_index: usize) -> Option<u8> {
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
        } else if let Some(slot) = static_load_index(instruction) {
            SlotDomain::Static(slot)
        } else {
            return None;
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
pub(super) fn initslot_arg_count_at(
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
pub(super) enum CallArgSource {
    Target(usize),
    PassThrough(u8),
}

/// Trace backwards from a CALL instruction to find the source of the
/// `arg_index`-th argument (0-indexed).
///
/// Neo VM pops arguments top-first: the top of stack becomes arg0, the next
/// item becomes arg1, etc. So `arg0` is the last item pushed (0 items to skip)
/// and `arg N` requires skipping N single-push instructions.
pub(super) fn trace_call_arg_source(
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

/// Second pass over call edges: resolve CALLA targets that load their function
/// pointer from an argument slot (LDARG N) by tracing back through callers.
fn resolve_ldarg_calla_targets(
    instructions: &[Instruction],
    edges: &mut [CallEdge],
    table: &MethodTable,
    methods: &mut BTreeMap<usize, MethodRef>,
) {
    // Build offset → instruction-index map.
    let offset_to_index: BTreeMap<usize, usize> = instructions
        .iter()
        .enumerate()
        .map(|(i, instr)| (instr.offset, i))
        .collect();

    // Collect unresolved CALLA sites preceded by LDARG.
    // NOTE: edge.caller.offset may be inaccurate for internal helpers discovered
    // during the first pass (the MethodTable was built before those methods were
    // found).  Use the `methods` map — which now contains all first-pass
    // discoveries — to find the true containing method for each CALLA.
    let mut sites: Vec<(usize, u8, usize)> = Vec::new(); // (edge_index, arg_index, method_offset)
    for (edge_idx, edge) in edges.iter().enumerate() {
        if edge.opcode != "CALLA" || !matches!(edge.target, CallTarget::Indirect { .. }) {
            continue;
        }
        let Some(&calla_idx) = offset_to_index.get(&edge.call_offset) else {
            continue;
        };
        if let Some(arg_idx) = calla_ldarg_index(instructions, calla_idx) {
            // Find the actual method containing this CALLA by looking up the
            // largest method offset <= the CALLA offset in the methods map.
            let actual_method_offset = methods
                .range(..=edge.call_offset)
                .next_back()
                .map(|(&offset, _)| offset)
                .unwrap_or(edge.caller.offset);
            sites.push((edge_idx, arg_idx, actual_method_offset));
        }
    }

    if sites.is_empty() {
        return;
    }

    loop {
        let mut callers_by_target: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for edge in edges.iter() {
            if let CallTarget::Internal { method } = &edge.target {
                if edge.opcode == "CALL" || edge.opcode == "CALL_L" || edge.opcode == "CALLA" {
                    callers_by_target
                        .entry(method.offset)
                        .or_default()
                        .push(edge.call_offset);
                }
            }
        }

        let mut progress = false;
        for (edge_idx, arg_idx, method_offset) in &sites {
            if !matches!(edges[*edge_idx].target, CallTarget::Indirect { .. }) {
                continue;
            }

            let mut visited = BTreeSet::new();
            let resolved = resolve_argument_target_recursive(
                instructions,
                &offset_to_index,
                &callers_by_target,
                methods,
                *method_offset,
                *arg_idx,
                &mut visited,
            );

            if let Some(target) = resolved {
                let callee = table.resolve_internal_target(target);
                methods.insert(callee.offset, callee.clone());
                edges[*edge_idx].target = CallTarget::Internal { method: callee };
                progress = true;
            }
        }

        if !progress {
            break;
        }
    }
}

fn resolve_argument_target_recursive(
    instructions: &[Instruction],
    offset_to_index: &BTreeMap<usize, usize>,
    callers_by_target: &BTreeMap<usize, Vec<usize>>,
    methods: &BTreeMap<usize, MethodRef>,
    method_offset: usize,
    arg_index: u8,
    visited: &mut BTreeSet<(usize, u8)>,
) -> Option<usize> {
    if !visited.insert((method_offset, arg_index)) {
        return None;
    }

    let call_sites = callers_by_target.get(&method_offset)?;
    let callee_arg_count =
        initslot_arg_count_at(instructions, method_offset).unwrap_or(arg_index as usize + 1);

    for &call_offset in call_sites {
        let &call_idx = offset_to_index.get(&call_offset)?;
        match trace_call_arg_source(instructions, call_idx, arg_index, callee_arg_count) {
            Some(CallArgSource::Target(target)) => return Some(target),
            Some(CallArgSource::PassThrough(next_arg)) => {
                let caller_method_offset = methods
                    .range(..=call_offset)
                    .next_back()
                    .map(|(&offset, _)| offset)
                    .unwrap_or(call_offset);
                if let Some(target) = resolve_argument_target_recursive(
                    instructions,
                    offset_to_index,
                    callers_by_target,
                    methods,
                    caller_method_offset,
                    next_arg,
                    visited,
                ) {
                    return Some(target);
                }
            }
            None => {}
        }
    }

    None
}
