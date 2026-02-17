//! Call graph construction for Neo N3 scripts.

// Bytecode offset arithmetic requires isize↔usize casts for signed jump deltas.
// NEF scripts are bounded (~1 MB), so these conversions are structurally safe.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]

use std::collections::BTreeMap;

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

fn calla_target_from_pusha(instructions: &[Instruction], index: usize) -> Option<usize> {
    let mut cursor = index.checked_sub(1)?;
    loop {
        let prev = instructions.get(cursor)?;
        if prev.opcode == OpCode::Nop {
            cursor = cursor.checked_sub(1)?;
            continue;
        }
        if prev.opcode == OpCode::PushA {
            return pusha_absolute_target(prev);
        }
        if let Some(slot) = local_load_index(prev) {
            return resolve_slot_pointer_target(instructions, cursor, SlotDomain::Local(slot));
        }
        if let Some(slot) = static_load_index(prev) {
            return resolve_slot_pointer_target(instructions, cursor, SlotDomain::Static(slot));
        }
        return None;
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
    for index in (0..before_index).rev() {
        let instruction = instructions.get(index)?;
        if slot_store_domain(instruction) != Some(domain) {
            continue;
        }

        let source = index.checked_sub(1).and_then(|prev| instructions.get(prev));
        return source.and_then(pusha_absolute_target);
    }
    None
}

// ---------------------------------------------------------------------------
// Second-pass inter-procedural CALLA resolution for LDARG patterns
// ---------------------------------------------------------------------------

/// Check if a CALLA instruction is preceded by LDARG and return the arg index.
fn calla_ldarg_index(instructions: &[Instruction], calla_index: usize) -> Option<u8> {
    let mut cursor = calla_index.checked_sub(1)?;
    loop {
        let prev = instructions.get(cursor)?;
        if prev.opcode == OpCode::Nop {
            cursor = cursor.checked_sub(1)?;
            continue;
        }
        return arg_load_index(prev);
    }
}

/// Extract the argument count from an INITSLOT instruction at the given method offset.
fn initslot_arg_count_at(instructions: &[Instruction], method_offset: usize) -> Option<usize> {
    instructions
        .iter()
        .find(|i| i.offset == method_offset && i.opcode == OpCode::Initslot)
        .and_then(|i| match &i.operand {
            Some(Operand::Bytes(bytes)) if bytes.len() >= 2 => Some(bytes[1] as usize),
            _ => None,
        })
}

/// Trace backwards from a CALL instruction to find the PUSHA target that was
/// pushed as the `arg_index`-th argument (0-indexed).
///
/// Neo VM pops arguments top-first: the top of stack becomes arg0, the next
/// item becomes arg1, etc.  So `arg0` is the last item pushed (0 items to
/// skip) and `arg N` requires skipping N single-push instructions.
fn trace_call_arg_to_pusha(
    instructions: &[Instruction],
    call_index: usize,
    arg_index: u8,
    callee_arg_count: usize,
) -> Option<usize> {
    if (arg_index as usize) >= callee_arg_count {
        return None;
    }
    let skip_count = arg_index as usize;

    let mut cursor = call_index.checked_sub(1)?;
    let mut remaining = skip_count;

    loop {
        let instr = instructions.get(cursor)?;
        if instr.opcode == OpCode::Nop {
            cursor = cursor.checked_sub(1)?;
            continue;
        }

        if remaining == 0 {
            // This instruction should push the target argument.
            if instr.opcode == OpCode::PushA {
                return pusha_absolute_target(instr);
            }
            if let Some(slot) = local_load_index(instr) {
                return resolve_slot_pointer_target(instructions, cursor, SlotDomain::Local(slot));
            }
            if let Some(slot) = static_load_index(instr) {
                return resolve_slot_pointer_target(
                    instructions,
                    cursor,
                    SlotDomain::Static(slot),
                );
            }
            return None;
        }

        // Assume each non-NOP instruction in the arg-push sequence produces
        // exactly one stack item (LDLOC, LDARG, PUSHA, PUSH*, etc.).
        remaining -= 1;
        cursor = cursor.checked_sub(1)?;
    }
}

/// Second pass over call edges: resolve CALLA targets that load their function
/// pointer from an argument slot (LDARG N) by tracing back through callers.
fn resolve_ldarg_calla_targets(
    instructions: &[Instruction],
    edges: &mut Vec<CallEdge>,
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

    // Build caller index: callee_method_offset → [call_instruction_offset, …]
    let mut callers_by_target: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for edge in edges.iter() {
        if let CallTarget::Internal { method } = &edge.target {
            if edge.opcode == "CALL" || edge.opcode == "CALL_L" {
                callers_by_target
                    .entry(method.offset)
                    .or_default()
                    .push(edge.call_offset);
            }
        }
    }

    for (edge_idx, arg_idx, method_offset) in &sites {
        let Some(call_sites) = callers_by_target.get(method_offset) else {
            continue;
        };
        let Some(callee_arg_count) = initslot_arg_count_at(instructions, *method_offset) else {
            continue;
        };

        let mut resolved = None;
        for &call_offset in call_sites {
            let Some(&call_idx) = offset_to_index.get(&call_offset) else {
                continue;
            };
            if let Some(target) =
                trace_call_arg_to_pusha(instructions, call_idx, *arg_idx, callee_arg_count)
            {
                resolved = Some(target);
                break;
            }
        }

        if let Some(target) = resolved {
            let callee = table.resolve_internal_target(target);
            methods.insert(callee.offset, callee.clone());
            edges[*edge_idx].target = CallTarget::Internal { method: callee };
        }
    }
}
