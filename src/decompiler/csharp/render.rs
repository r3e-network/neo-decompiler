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
use std::collections::BTreeMap;

use super::super::helpers::{
    build_method_arg_counts_by_offset, extract_contract_name, inferred_method_starts,
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
    let method_arg_counts_by_offset =
        build_method_arg_counts_by_offset(instructions, &inferred_starts, manifest);
    let call_targets_by_offset = build_call_targets_by_offset(call_graph);
    let calla_targets_by_offset = build_calla_targets_by_offset(call_graph);
    let body_context = body::LiftedBodyContext {
        method_arg_counts_by_offset: &method_arg_counts_by_offset,
        call_targets_by_offset: &call_targets_by_offset,
        calla_targets_by_offset: &calla_targets_by_offset,
        callt_labels: &callt_labels,
        callt_param_counts: &callt_param_counts,
        callt_returns_value: &callt_returns_value,
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
    } else {
        methods::write_fallback_entry(&mut output, &methods_context, &mut warnings);
    }

    header::write_contract_close(&mut output);
    CSharpRender {
        source: output,
        warnings,
    }
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
