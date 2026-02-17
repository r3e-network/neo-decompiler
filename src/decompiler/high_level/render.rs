use super::super::analysis::call_graph::{CallGraph, CallTarget};
use super::super::helpers::{
    build_method_arg_counts_by_offset, find_manifest_entry_method, inferred_method_starts,
    make_unique_identifier, offset_as_usize, sanitize_identifier,
};
use crate::instruction::Instruction;
use crate::manifest::ContractManifest;
use crate::native_contracts;
use crate::nef::NefFile;

use std::collections::{BTreeMap, BTreeSet, HashSet};

mod body;
mod entry;
mod header;
mod manifest_summary;
mod method_tokens;
mod methods;

pub(crate) struct HighLevelRender {
    pub(crate) text: String,
    pub(crate) warnings: Vec<String>,
}

/// Render the high-level pseudo-contract view.
pub(crate) fn render_high_level(
    nef: &NefFile,
    instructions: &[Instruction],
    manifest: Option<&ContractManifest>,
    call_graph: &CallGraph,
    inline_single_use_temps: bool,
) -> HighLevelRender {
    use std::fmt::Write;

    let mut output = String::new();
    let mut warnings = Vec::new();
    let mut used_method_names = HashSet::new();

    // Pre-resolve CALLT method-token labels so the emitter can emit friendly names.
    let callt_labels: Vec<String> = nef
        .method_tokens
        .iter()
        .map(|token| {
            if let Some(hint) = native_contracts::describe_method_token(&token.hash, &token.method)
            {
                hint.formatted_label(&token.method)
            } else {
                token.method.clone()
            }
        })
        .collect();
    let callt_param_counts: Vec<usize> = nef
        .method_tokens
        .iter()
        .map(|token| token.parameters_count as usize)
        .collect();
    let callt_returns_value: Vec<bool> = nef
        .method_tokens
        .iter()
        .map(|token| token.has_return_value)
        .collect();

    header::write_contract_header(&mut output, nef, manifest);

    let inferred_starts = inferred_method_starts(instructions, manifest);
    let method_labels_by_offset =
        build_method_labels_by_offset(instructions, &inferred_starts, manifest);
    let method_arg_counts_by_offset =
        build_method_arg_counts_by_offset(instructions, &inferred_starts, manifest);
    let call_targets_by_offset = if manifest.is_some() {
        build_call_targets_by_offset(call_graph)
    } else {
        BTreeMap::new()
    };
    let calla_targets_by_offset = build_calla_targets_by_offset(call_graph);
    let noreturn_method_offsets =
        build_noreturn_method_offsets(instructions, &inferred_starts);
    let body_context = body::MethodBodyContext {
        method_labels_by_offset: &method_labels_by_offset,
        method_arg_counts_by_offset: &method_arg_counts_by_offset,
        call_targets_by_offset: &call_targets_by_offset,
        calla_targets_by_offset: &calla_targets_by_offset,
        noreturn_method_offsets: &noreturn_method_offsets,
        inline_single_use_temps,
        callt_labels: &callt_labels,
        callt_param_counts: &callt_param_counts,
        callt_returns_value: &callt_returns_value,
    };
    let methods_context = methods::MethodsContext {
        instructions,
        inferred_method_starts: &inferred_starts,
        body_context: &body_context,
    };
    let entry_method = entry::write_entry_method(
        &mut output,
        instructions,
        &inferred_starts,
        manifest,
        &body_context,
        &mut warnings,
        &mut used_method_names,
    );
    if let Some(manifest) = manifest {
        methods::write_manifest_methods(
            &mut output,
            &methods_context,
            manifest,
            entry_method.as_ref(),
            &mut warnings,
            &mut used_method_names,
        );
    }
    methods::write_inferred_methods(
        &mut output,
        &methods_context,
        manifest,
        entry_method.as_ref(),
        &mut warnings,
        &mut used_method_names,
    );
    writeln!(output, "}}").unwrap();

    HighLevelRender {
        text: output,
        warnings,
    }
}

fn build_method_labels_by_offset(
    instructions: &[Instruction],
    inferred_starts: &[usize],
    manifest: Option<&ContractManifest>,
) -> BTreeMap<usize, String> {
    let mut labels = BTreeMap::new();
    let mut used = HashSet::new();

    let entry_offset = instructions.first().map(|ins| ins.offset).unwrap_or(0);
    let entry_method = manifest.and_then(|m| find_manifest_entry_method(m, entry_offset));
    let use_manifest_entry = entry_method
        .as_ref()
        .map(|(_, matched)| *matched)
        .unwrap_or(false);
    let entry_name = if use_manifest_entry {
        entry_method
            .as_ref()
            .map(|(method, _)| sanitize_identifier(&method.name))
            .unwrap_or_else(|| "script_entry".to_string())
    } else {
        "script_entry".to_string()
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
            labels.entry(start).or_insert_with(|| {
                make_unique_identifier(sanitize_identifier(&method.name), &mut used)
            });
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

/// Compute the set of method start offsets whose bodies always terminate
/// without returning (every exit path ends with ABORT, ABORTMSG, or THROW).
///
/// This is a conservative heuristic: a method is considered noreturn only if
/// its last instruction is a terminating opcode and it contains no RET.
fn build_noreturn_method_offsets(
    instructions: &[Instruction],
    inferred_starts: &[usize],
) -> BTreeSet<usize> {
    use crate::instruction::OpCode;

    let mut noreturn = BTreeSet::new();
    let all_starts: Vec<usize> = inferred_starts.to_vec();

    for (idx, &start) in all_starts.iter().enumerate() {
        let end = all_starts
            .get(idx + 1)
            .copied()
            .or_else(|| instructions.last().map(|i| i.offset + 1))
            .unwrap_or(start);

        let lo = instructions.partition_point(|ins| ins.offset < start);
        let hi = instructions.partition_point(|ins| ins.offset < end);
        let slice = &instructions[lo..hi];
        if slice.is_empty() {
            continue;
        }

        let has_ret = slice.iter().any(|ins| ins.opcode == OpCode::Ret);
        let last_is_terminator = slice.last().is_some_and(|ins| {
            matches!(
                ins.opcode,
                OpCode::Abort | OpCode::Abortmsg | OpCode::Throw
            )
        });

        if !has_ret && last_is_terminator {
            noreturn.insert(start);
        }
    }

    noreturn
}

fn build_calla_targets_by_offset(call_graph: &CallGraph) -> BTreeMap<usize, usize> {
    let mut targets = BTreeMap::new();
    for edge in &call_graph.edges {
        if edge.opcode != "CALLA" {
            continue;
        }
        if let CallTarget::Internal { method } = &edge.target {
            targets.insert(edge.call_offset, method.offset);
        }
    }
    targets
}

fn build_call_targets_by_offset(call_graph: &CallGraph) -> BTreeMap<usize, usize> {
    let mut targets = BTreeMap::new();
    for edge in &call_graph.edges {
        if edge.opcode != "CALL" && edge.opcode != "CALL_L" {
            continue;
        }
        if let CallTarget::Internal { method } = &edge.target {
            targets.insert(edge.call_offset, method.offset);
        }
    }
    targets
}
