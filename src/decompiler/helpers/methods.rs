use std::collections::BTreeSet;

use crate::instruction::{Instruction, OpCode, Operand};
use crate::manifest::{ContractManifest, ManifestMethod};

/// Return the ABI method that matches the script entry offset, falling back to
/// the first ABI method when offsets are missing.
pub(in super::super) fn find_manifest_entry_method(
    manifest: &ContractManifest,
    entry_offset: usize,
) -> Option<(&ManifestMethod, bool)> {
    manifest
        .abi
        .methods
        .iter()
        .find(|method| offset_as_usize(method.offset) == Some(entry_offset))
        .map(|method| (method, true))
}

/// Return `true` when at least one manifest method starts at `offset`.
pub(in super::super) fn has_manifest_method_at_offset(
    manifest: &ContractManifest,
    offset: usize,
) -> bool {
    manifest
        .abi
        .methods
        .iter()
        .any(|method| offset_as_usize(method.offset) == Some(offset))
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

fn collect_post_ret_method_offsets(
    instructions: &[Instruction],
    baseline_starts: &BTreeSet<usize>,
) -> Vec<usize> {
    let known_offsets: BTreeSet<usize> = instructions.iter().map(|ins| ins.offset).collect();
    let control_flow_edges = collect_control_flow_edges(instructions, &known_offsets);
    let mut starts = instructions
        .windows(2)
        .filter_map(|pair| {
            let current = &pair[0];
            let next = &pair[1];
            let method_start = baseline_starts
                .range(..=current.offset)
                .next_back()
                .copied()
                .unwrap_or(current.offset);
            let method_end = baseline_starts
                .range((method_start + 1)..)
                .next()
                .copied()
                .unwrap_or(usize::MAX);
            let has_incoming_from_same_baseline_method =
                control_flow_edges.iter().any(|(source, target)| {
                    *target == next.offset && *source >= method_start && *source < method_end
                });
            let has_incoming_from_other_baseline_method =
                control_flow_edges.iter().any(|(source, target)| {
                    *target == next.offset && (*source < method_start || *source >= method_end)
                });
            let has_same_baseline_incoming_later_in_range =
                control_flow_edges.iter().any(|(source, target)| {
                    *source >= method_start
                        && *source < method_end
                        && *target > next.offset
                        && *target < method_end
                });
            let detached_tail_after_terminator = has_incoming_from_other_baseline_method
                || (!has_incoming_from_same_baseline_method
                    && !has_same_baseline_incoming_later_in_range);
            let is_terminator = matches!(
                current.opcode,
                OpCode::Ret | OpCode::Throw | OpCode::Abort | OpCode::Abortmsg
            );
            (is_terminator && detached_tail_after_terminator).then_some(next.offset)
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
