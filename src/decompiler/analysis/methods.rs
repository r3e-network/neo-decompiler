use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::instruction::{Instruction, OpCode, Operand};
use crate::manifest::ContractManifest;

use super::super::helpers::{
    collect_call_targets, collect_initslot_offsets, find_manifest_entry_method, offset_as_usize,
    sanitize_identifier,
};
use super::call_graph::{
    calla_ldarg_index, calla_target_from_pusha, initslot_arg_count_at, trace_call_arg_source,
    CallArgSource,
};

/// Reference to a (possibly inferred) method within a script.
///
/// When a manifest is present, `name` typically matches the ABI method name.
/// For internal helper routines without ABI metadata, `name` will be a
/// synthetic `sub_0x....` label.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct MethodRef {
    /// Method entry offset in bytecode.
    pub offset: usize,
    /// Human-readable method name.
    pub name: String,
}

impl MethodRef {
    pub(super) fn synthetic(offset: usize) -> Self {
        Self {
            offset,
            name: format!("sub_0x{offset:04X}"),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct MethodSpan {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) method: MethodRef,
}

/// Helper for mapping bytecode offsets to method ranges.
#[derive(Debug, Clone)]
pub struct MethodTable {
    spans: Vec<MethodSpan>,
    manifest_index_by_start: BTreeMap<usize, usize>,
}

impl MethodTable {
    /// Build a method table using stable method starts plus manifest metadata.
    ///
    /// Stable starts include the script entry, manifest ABI offsets, compiler
    /// `INITSLOT` prologues, and direct internal call targets. This keeps
    /// analysis aligned with inferred helpers without over-splitting detached
    /// tails that are only useful for presentation-time rendering.
    #[must_use]
    pub fn new(instructions: &[Instruction], manifest: Option<&ContractManifest>) -> Self {
        let script_start = instructions.first().map(|ins| ins.offset).unwrap_or(0);
        let script_end = instructions
            .last()
            .map(|ins| ins.offset.saturating_add(1))
            .unwrap_or(script_start);

        let mut manifest_index_by_start = BTreeMap::new();
        let entry_manifest = manifest.and_then(|manifest| {
            let entry_method = find_manifest_entry_method(manifest, script_start)?;
            let index = manifest
                .abi
                .methods
                .iter()
                .position(|candidate| std::ptr::eq(candidate, entry_method.0))?;
            Some((entry_method.0, index))
        });

        let mut starts = BTreeMap::new();
        starts.insert(script_start, ());
        for start in collect_initslot_offsets(instructions) {
            starts.insert(start, ());
        }
        for start in collect_call_targets(instructions) {
            starts.insert(start, ());
        }
        let mut callers_by_target: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (index, instruction) in instructions.iter().enumerate() {
            if instruction.opcode == OpCode::CallA {
                if let Some(start) = calla_target_from_pusha(instructions, index) {
                    starts.insert(start, ());
                    callers_by_target.entry(start).or_default().push(index);
                }
                continue;
            }
            if matches!(instruction.opcode, OpCode::Call | OpCode::Call_L) {
                if let Some(target) = Self::direct_call_target(instruction) {
                    callers_by_target.entry(target).or_default().push(index);
                }
            }
        }

        loop {
            let method_starts: Vec<usize> = starts.keys().copied().collect();
            let mut progress = false;

            for (index, instruction) in instructions.iter().enumerate() {
                if instruction.opcode != OpCode::CallA {
                    continue;
                }
                let Some(arg_index) = calla_ldarg_index(instructions, index) else {
                    continue;
                };
                let Some(method_offset) = method_starts
                    .iter()
                    .copied()
                    .filter(|start| *start <= instruction.offset)
                    .max()
                else {
                    continue;
                };
                let mut visited = BTreeSet::new();
                if let Some(start) = Self::resolve_argument_target_for_method(
                    instructions,
                    &callers_by_target,
                    &method_starts,
                    method_offset,
                    arg_index,
                    &mut visited,
                ) {
                    let mut changed = starts.insert(start, ()).is_none();
                    let callers = callers_by_target.entry(start).or_default();
                    if !callers.contains(&index) {
                        callers.push(index);
                        changed = true;
                    }
                    if changed {
                        progress = true;
                    }
                }
            }

            if !progress {
                break;
            }
        }

        if let Some(manifest) = manifest {
            for (idx, method) in manifest.abi.methods.iter().enumerate() {
                if let Some(start) = offset_as_usize(method.offset) {
                    manifest_index_by_start.insert(start, idx);
                    starts.insert(start, ());
                }
            }
            if let Some((_, index)) = entry_manifest {
                manifest_index_by_start.entry(script_start).or_insert(index);
            }
        }

        let ordered_starts: Vec<usize> = starts.into_keys().collect();
        let mut spans = Vec::new();
        for (position, start) in ordered_starts.iter().copied().enumerate() {
            let end = ordered_starts
                .get(position + 1)
                .copied()
                .unwrap_or(script_end);
            let method = if let Some(manifest) = manifest {
                if let Some(index) = manifest_index_by_start.get(&start).copied() {
                    let manifest_method = &manifest.abi.methods[index];
                    MethodRef {
                        offset: start,
                        name: sanitize_identifier(&manifest_method.name),
                    }
                } else if start == script_start {
                    MethodRef {
                        offset: start,
                        name: entry_manifest
                            .as_ref()
                            .map(|(method, _)| sanitize_identifier(&method.name))
                            .unwrap_or_else(|| "script_entry".to_string()),
                    }
                } else {
                    MethodRef::synthetic(start)
                }
            } else if start == script_start {
                MethodRef {
                    offset: start,
                    name: "script_entry".to_string(),
                }
            } else {
                MethodRef::synthetic(start)
            };

            spans.push(MethodSpan { start, end, method });
        }

        spans.sort_by_key(|span| span.start);

        Self {
            spans,
            manifest_index_by_start,
        }
    }

    fn resolve_argument_target_for_method(
        instructions: &[Instruction],
        callers_by_target: &BTreeMap<usize, Vec<usize>>,
        method_starts: &[usize],
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

        for &call_index in call_sites {
            let call_offset = instructions.get(call_index)?.offset;
            match trace_call_arg_source(instructions, call_index, arg_index, callee_arg_count) {
                Some(CallArgSource::Target(target)) => return Some(target),
                Some(CallArgSource::PassThrough(next_arg)) => {
                    let caller_method_offset = method_starts
                        .iter()
                        .copied()
                        .filter(|start| *start <= call_offset)
                        .max()
                        .unwrap_or(call_offset);
                    if let Some(target) = Self::resolve_argument_target_for_method(
                        instructions,
                        callers_by_target,
                        method_starts,
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

    /// Return all known method spans ordered by start offset.
    pub(super) fn spans(&self) -> &[MethodSpan] {
        &self.spans
    }

    /// Resolve the method that contains the given bytecode offset.
    #[must_use]
    pub fn method_for_offset(&self, offset: usize) -> MethodRef {
        match self.spans.binary_search_by_key(&offset, |span| span.start) {
            Ok(index) => self.spans[index].method.clone(),
            Err(0) => self
                .spans
                .first()
                .map(|span| span.method.clone())
                .unwrap_or_else(|| MethodRef::synthetic(offset)),
            Err(index) => {
                let span = &self.spans[index - 1];
                span.method.clone()
            }
        }
    }

    /// Resolve an internal call target to a method reference.
    #[must_use]
    pub fn resolve_internal_target(&self, target_offset: usize) -> MethodRef {
        self.spans
            .iter()
            .find(|span| span.start == target_offset)
            .map(|span| span.method.clone())
            .unwrap_or_else(|| MethodRef::synthetic(target_offset))
    }

    fn direct_call_target(instruction: &Instruction) -> Option<usize> {
        let delta = match instruction.operand {
            Some(Operand::Jump(value)) => value as isize,
            Some(Operand::Jump32(value)) => value as isize,
            _ => return None,
        };
        instruction.offset.checked_add_signed(delta)
    }

    /// Return the manifest ABI method index for a method starting at `offset`, if any.
    #[must_use]
    pub fn manifest_index_for_start(&self, offset: usize) -> Option<usize> {
        self.manifest_index_by_start.get(&offset).copied()
    }
}
