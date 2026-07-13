use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::instruction::{Instruction, OpCode, Operand};
use crate::manifest::{ContractManifest, ManifestMethod};

use super::identifiers::make_unique_identifier;

/// Return the ABI method that matches the script entry offset, falling back to
/// the first ABI method when all ABI offsets are missing.
pub(in super::super) fn find_manifest_entry_method(
    manifest: &ContractManifest,
    entry_offset: usize,
) -> Option<(&ManifestMethod, bool)> {
    if let Some(method) = manifest
        .abi
        .methods
        .iter()
        .find(|method| offset_as_usize(method.offset) == Some(entry_offset))
    {
        return Some((method, true));
    }

    if manifest
        .abi
        .methods
        .iter()
        .any(|method| offset_as_usize(method.offset).is_some())
    {
        return None;
    }

    manifest.abi.methods.first().map(|method| (method, false))
}

/// Convert a manifest offset (`Option<i32>`) to `Option<usize>`, treating
/// negative values (e.g. `-1` for abstract methods) as `None`.
pub(in super::super) fn offset_as_usize(offset: Option<i32>) -> Option<usize> {
    offset.and_then(|v| usize::try_from(v).ok())
}

/// Build a sorted list of inferred method starts.
///
/// Sources include:
/// - script entry offset;
/// - manifest ABI offsets (when present);
/// - every `INITSLOT` instruction offset (compiler-emitted method prologues);
/// - offsets immediately following terminating instructions (`RET`, `THROW`,
///   `ABORT`, `ABORTMSG`) when control-flow indicates a detached chunk
///   (incoming from outside the baseline method, or no same-baseline incoming
///   to the remaining tail range).
#[must_use]
pub(in super::super) fn inferred_method_starts(
    instructions: &[Instruction],
    manifest: Option<&ContractManifest>,
) -> Vec<usize> {
    let mut starts = BTreeSet::new();
    if let Some(entry) = instructions.first() {
        starts.insert(entry.offset);
    }

    if let Some(manifest) = manifest {
        starts.extend(
            manifest
                .abi
                .methods
                .iter()
                .filter_map(|method| offset_as_usize(method.offset)),
        );
    }

    starts.extend(collect_initslot_offsets(instructions));
    starts.extend(collect_call_targets(instructions));
    let baseline_starts = starts.clone();
    starts.extend(collect_post_ret_method_offsets(
        instructions,
        &baseline_starts,
    ));
    starts.into_iter().collect()
}

/// Return the next known method start after `start`.
#[must_use]
pub(in super::super) fn next_inferred_method_offset(
    starts: &[usize],
    start: usize,
) -> Option<usize> {
    starts.iter().copied().find(|offset| *offset > start)
}

/// Return the argument count declared by an `INITSLOT` prologue at `start`.
#[must_use]
pub(in super::super) fn initslot_argument_count_at(
    instructions: &[Instruction],
    start: usize,
) -> Option<usize> {
    instructions
        .iter()
        .find(|ins| ins.offset == start && ins.opcode == OpCode::Initslot)
        .and_then(|ins| match &ins.operand {
            Some(Operand::Bytes(bytes)) if bytes.len() >= 2 => Some(bytes[1] as usize),
            _ => None,
        })
}

/// Collect offsets where the NEF script starts a new method (`INITSLOT`).
pub(in super::super) fn collect_initslot_offsets(instructions: &[Instruction]) -> Vec<usize> {
    let mut offsets = instructions
        .iter()
        .filter(|ins| matches!(ins.opcode, OpCode::Initslot))
        .map(|ins| ins.offset)
        .collect::<Vec<_>>();
    offsets.sort_unstable();
    offsets.dedup();
    offsets
}

/// Collect targets of internal `CALL` / `CALL_L` instructions.
///
/// Each CALL target is a method entry point that may lack an `INITSLOT`
/// prologue (e.g. simple helpers that use no locals/arguments).  Adding
/// these as baseline method starts prevents their bodies from being
/// inlined into the caller's method body.
pub(in super::super) fn collect_call_targets(instructions: &[Instruction]) -> Vec<usize> {
    let known_offsets: BTreeSet<usize> = instructions.iter().map(|ins| ins.offset).collect();
    let mut targets = Vec::new();
    for instruction in instructions {
        if matches!(instruction.opcode, OpCode::Call | OpCode::Call_L) {
            if let Some(target) = relative_target(instruction, &known_offsets) {
                targets.push(target);
            }
        }
    }
    targets.sort_unstable();
    targets.dedup();
    targets
}

fn collect_post_ret_method_offsets(
    instructions: &[Instruction],
    baseline_starts: &BTreeSet<usize>,
) -> Vec<usize> {
    let known_offsets: BTreeSet<usize> = instructions.iter().map(|ins| ins.offset).collect();
    let control_flow_edges = collect_control_flow_edges(instructions, &known_offsets);

    // Resolve the baseline method `[start, end)` that contains `offset`.
    let method_range = |offset: usize| -> (usize, usize) {
        let start = baseline_starts
            .range(..=offset)
            .next_back()
            .copied()
            .unwrap_or(offset);
        let end = baseline_starts
            .range((start + 1)..)
            .next()
            .copied()
            .unwrap_or(usize::MAX);
        (start, end)
    };

    // edges_by_target: target_offset -> source offsets that branch to it.
    let mut edges_by_target: HashMap<usize, Vec<usize>> = HashMap::new();
    // max_forward_in_method: baseline method start -> the furthest forward-edge
    // target that stays inside that method. Precomputing this once turns the
    // "is there an in-method edge past `next`?" test into an O(1) lookup instead
    // of an O(method length) scan per terminator. Without it the whole pass is
    // O(n²) and a crafted in-cap NEF (many crossing jumps in one method) hangs
    // the decompiler.
    let mut max_forward_in_method: HashMap<usize, usize> = HashMap::new();
    for &(source, target) in &control_flow_edges {
        edges_by_target.entry(target).or_default().push(source);
        let (m_start, m_end) = method_range(source);
        if target < m_end {
            max_forward_in_method
                .entry(m_start)
                .and_modify(|t| *t = (*t).max(target))
                .or_insert(target);
        }
    }

    let mut starts = instructions
        .windows(2)
        .filter_map(|pair| {
            let current = &pair[0];
            // Only the instruction immediately after a terminator can begin a
            // detached tail, so skip the analysis for every other pair.
            if !matches!(
                current.opcode,
                OpCode::Ret | OpCode::Throw | OpCode::Abort | OpCode::Abortmsg
            ) {
                return None;
            }
            let next = &pair[1];
            let (method_start, method_end) = method_range(current.offset);

            let incoming = edges_by_target.get(&next.offset);
            let has_incoming_from_same_baseline_method = incoming.is_some_and(|sources| {
                sources.iter().any(|&s| s >= method_start && s < method_end)
            });
            let has_incoming_from_other_baseline_method = incoming.is_some_and(|sources| {
                sources.iter().any(|&s| s < method_start || s >= method_end)
            });
            // Equivalent to scanning every in-method edge for a target in
            // (next.offset, method_end): the per-method maximum forward target
            // (already bounded to < method_end) exceeds next.offset iff one exists.
            let has_same_baseline_incoming_later_in_range = max_forward_in_method
                .get(&method_start)
                .is_some_and(|&max_target| max_target > next.offset);

            let detached_tail_after_terminator = has_incoming_from_other_baseline_method
                || (!has_incoming_from_same_baseline_method
                    && !has_same_baseline_incoming_later_in_range);
            detached_tail_after_terminator.then_some(next.offset)
        })
        .collect::<Vec<_>>();
    starts.sort_unstable();
    starts.dedup();
    starts
}

fn collect_control_flow_edges(
    instructions: &[Instruction],
    known_offsets: &BTreeSet<usize>,
) -> Vec<(usize, usize)> {
    let mut edges = Vec::new();
    for instruction in instructions {
        match instruction.opcode {
            OpCode::Jmp
            | OpCode::Jmp_L
            | OpCode::Jmpif
            | OpCode::Jmpif_L
            | OpCode::Jmpifnot
            | OpCode::Jmpifnot_L
            | OpCode::JmpEq
            | OpCode::JmpEq_L
            | OpCode::JmpNe
            | OpCode::JmpNe_L
            | OpCode::JmpGt
            | OpCode::JmpGt_L
            | OpCode::JmpGe
            | OpCode::JmpGe_L
            | OpCode::JmpLt
            | OpCode::JmpLt_L
            | OpCode::JmpLe
            | OpCode::JmpLe_L
            | OpCode::Endtry
            | OpCode::EndtryL => {
                if let Some(target) = relative_target(instruction, known_offsets) {
                    edges.push((instruction.offset, target));
                }
            }
            OpCode::Try => {
                if let Some(Operand::Bytes(bytes)) = &instruction.operand {
                    if bytes.len() == 2 {
                        for delta in [bytes[0] as i8 as isize, bytes[1] as i8 as isize] {
                            if let Some(target) =
                                relative_target_with_delta(instruction.offset, delta, known_offsets)
                            {
                                edges.push((instruction.offset, target));
                            }
                        }
                    }
                }
            }
            OpCode::TryL => {
                if let Some(Operand::Bytes(bytes)) = &instruction.operand {
                    if bytes.len() == 8 {
                        let catch_delta =
                            i32::from_le_bytes(bytes[0..4].try_into().expect("slice length"))
                                as isize;
                        let finally_delta =
                            i32::from_le_bytes(bytes[4..8].try_into().expect("slice length"))
                                as isize;
                        for delta in [catch_delta, finally_delta] {
                            if let Some(target) =
                                relative_target_with_delta(instruction.offset, delta, known_offsets)
                            {
                                edges.push((instruction.offset, target));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    edges
}

fn relative_target(instruction: &Instruction, known_offsets: &BTreeSet<usize>) -> Option<usize> {
    let delta = match &instruction.operand {
        Some(Operand::Jump(value)) => *value as isize,
        Some(Operand::Jump32(value)) => *value as isize,
        _ => return None,
    };
    relative_target_with_delta(instruction.offset, delta, known_offsets)
}

fn relative_target_with_delta(
    base: usize,
    delta: isize,
    known_offsets: &BTreeSet<usize>,
) -> Option<usize> {
    let target = base as isize + delta;
    if target < 0 {
        return None;
    }
    let target = target as usize;
    known_offsets.contains(&target).then_some(target)
}

/// Build a `BTreeMap` of method-start-offset → unique label for the whole
/// script, shared between the high-level and C# renderers. The only
/// differences between the two callers are the identifier sanitizer and the
/// fallback name for the script entry, so both are passed in as closures.
pub(in super::super) fn build_method_labels_by_offset(
    instructions: &[Instruction],
    inferred_starts: &[usize],
    manifest: Option<&ContractManifest>,
    sanitize: impl Fn(&str) -> String,
    fallback_name: &str,
) -> BTreeMap<usize, String> {
    let mut labels = BTreeMap::new();
    let mut used = HashSet::new();

    let entry_offset = instructions.first().map(|ins| ins.offset).unwrap_or(0);
    let entry_method = manifest.and_then(|m| find_manifest_entry_method(m, entry_offset));
    let use_manifest_entry = entry_method.is_some();
    let entry_name = if use_manifest_entry {
        entry_method
            .as_ref()
            .map(|(method, _)| sanitize(&method.name))
            .unwrap_or_else(|| fallback_name.to_string())
    } else {
        fallback_name.to_string()
    };
    labels.insert(entry_offset, make_unique_identifier(entry_name, &mut used));

    let entry_manifest_marker = if use_manifest_entry {
        entry_method
            .as_ref()
            .map(|(method, _)| (method.name.clone(), method.offset))
    } else {
        None
    };

    if let Some(manifest) = manifest {
        let mut methods: Vec<_> = manifest.abi.methods.iter().collect();
        methods.sort_by_key(|m| m.offset.unwrap_or(i32::MAX));
        for method in methods {
            if entry_manifest_marker
                .as_ref()
                .map(|(name, offset)| name == &method.name && offset == &method.offset)
                .unwrap_or(false)
            {
                continue;
            }

            let Some(start) = offset_as_usize(method.offset) else {
                continue;
            };
            labels
                .entry(start)
                .or_insert_with(|| make_unique_identifier(sanitize(&method.name), &mut used));
        }
    }

    let entry_manifest_offset = entry_manifest_marker
        .as_ref()
        .and_then(|(_, offset)| offset.and_then(|value| usize::try_from(value).ok()));
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
        if Some(*start) == Some(entry_offset)
            || Some(*start) == entry_manifest_offset
            || manifest_offsets.contains(start)
        {
            continue;
        }

        labels.entry(*start).or_insert_with(|| {
            let base_name = format!("sub_0x{start:04X}");
            make_unique_identifier(base_name, &mut used)
        });
    }

    labels
}
