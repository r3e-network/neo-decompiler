//! C# skeleton renderer.
//!
//! The renderer produces a Neo SmartContract Framework-compatible skeleton
//! (methods, events, and manifest metadata) and optionally includes lifted
//! pseudo-bodies when method offsets are available.

use super::super::analysis::call_graph::{CallGraph, CallTarget};
use crate::instruction::Instruction;
use crate::manifest::ContractManifest;
use crate::native_contracts;
use crate::nef::NefFile;
use std::collections::{BTreeMap, HashSet};

use super::super::helpers::{
    build_method_arg_counts_by_offset, extract_contract_name, find_manifest_entry_method,
    inferred_method_starts, make_unique_identifier, offset_as_usize,
};
use super::helpers::sanitize_csharp_identifier;

mod body;
mod events;
mod header;
mod methods;

pub(crate) struct CSharpRender {
    pub(crate) source: String,
    pub(crate) warnings: Vec<String>,
}

/// Render a C# skeleton with lifted bodies when possible.
pub(crate) fn render_csharp(
    nef: &NefFile,
    instructions: &[Instruction],
    manifest: Option<&ContractManifest>,
    call_graph: &CallGraph,
    inline_single_use_temps: bool,
    emit_trace_comments: bool,
) -> CSharpRender {
    let mut output = String::new();
    let mut warnings = Vec::new();
    header::write_preamble(&mut output);

    let contract_name = extract_contract_name(manifest, sanitize_csharp_identifier);

    // Pre-resolve CALLT method-token labels.
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
    let inferred_starts = inferred_method_starts(instructions, manifest);
    let method_labels_by_offset =
        build_method_labels_by_offset(instructions, &inferred_starts, manifest);
    let method_arg_counts_by_offset =
        build_method_arg_counts_by_offset(instructions, &inferred_starts, manifest);
    let call_targets_by_offset = build_call_targets_by_offset(call_graph);
    let calla_targets_by_offset = build_calla_targets_by_offset(call_graph);
    let body_context = body::LiftedBodyContext {
        method_labels_by_offset: &method_labels_by_offset,
        method_arg_counts_by_offset: &method_arg_counts_by_offset,
        call_targets_by_offset: &call_targets_by_offset,
        calla_targets_by_offset: &calla_targets_by_offset,
        callt_labels: &callt_labels,
        callt_param_counts: &callt_param_counts,
        callt_returns_value: &callt_returns_value,
        inline_single_use_temps,
        emit_trace_comments,
    };
    let methods_context = methods::MethodsContext {
        instructions,
        inferred_method_starts: &inferred_starts,
        body_context,
    };

    header::write_contract_open(&mut output, &contract_name, nef, manifest);

    if let Some(manifest) = manifest {
        events::write_events(&mut output, manifest);
        methods::write_manifest_methods(&mut output, manifest, &methods_context, &mut warnings);
        methods::write_inferred_methods(
            &mut output,
            &methods_context,
            Some(manifest),
            &mut warnings,
        );
    } else {
        methods::write_fallback_entry(&mut output, &methods_context, &mut warnings);
        methods::write_inferred_methods(&mut output, &methods_context, None, &mut warnings);
    }

    header::write_contract_close(&mut output);
    CSharpRender {
        source: output,
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
    let entry_name = entry_method
        .as_ref()
        .map(|(method, _)| sanitize_csharp_identifier(&method.name))
        .unwrap_or_else(|| "ScriptEntry".to_string());
    labels.insert(entry_offset, make_unique_identifier(entry_name, &mut used));

    let entry_manifest_marker = entry_method
        .as_ref()
        .map(|(method, _)| (method.name.clone(), method.offset));

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
                make_unique_identifier(sanitize_csharp_identifier(&method.name), &mut used)
            });
        }
    }

    let entry_manifest_offset = entry_manifest_marker
        .as_ref()
        .and_then(|(_, offset)| offset.and_then(|value| usize::try_from(value).ok()));
    let manifest_offsets: HashSet<usize> = manifest
        .map(|manifest| {
            manifest
                .abi
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
