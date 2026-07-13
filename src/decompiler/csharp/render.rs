//! C# skeleton renderer.
//!
//! The renderer produces a Neo SmartContract Framework-compatible skeleton
//! (methods, events, and manifest metadata) and optionally includes lifted
//! pseudo-bodies when method offsets are available.

use super::super::analysis::call_graph::CallGraph;
use super::super::analysis::method_contracts::MethodContracts;
use super::super::analysis::types::TypeInfo;
use super::super::helpers::build_method_labels_by_offset;
use crate::decompiler::output_format::RenderOptions;
use crate::instruction::Instruction;
use crate::manifest::ContractManifest;
use crate::native_contracts;
use crate::nef::NefFile;
use std::collections::BTreeMap;

use super::super::helpers::{
    build_call_targets_by_offset, build_calla_targets_by_offset, extract_contract_name,
    inferred_method_starts, inferred_type_to_csharp,
};
use super::helpers::sanitize_csharp_identifier;
use super::helpers::SlotTypes;

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
    method_contracts: &MethodContracts,
    types: &TypeInfo,
    opts: &RenderOptions,
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
    let method_labels_by_offset = build_method_labels_by_offset(
        instructions,
        &inferred_starts,
        manifest,
        sanitize_csharp_identifier,
        "ScriptEntry",
    );
    let method_arg_counts_by_offset = method_contracts.argument_counts_by_offset();
    let method_returns_value_by_offset = method_contracts.returns_value_by_offset();
    let call_targets_by_offset = build_call_targets_by_offset(call_graph);
    let calla_targets_by_offset = build_calla_targets_by_offset(call_graph);

    // Pre-resolve inferred C# slot types per method so that body-local
    // declarations can be rendered with concrete types (`BigInteger loc0`)
    // instead of `var` when `typed_declarations` is enabled. Built from the
    // already-computed `TypeInfo`; cheap (one entry per method).
    let slot_types_by_offset = build_slot_types_by_offset(types);
    let body_context = body::LiftedBodyContext {
        method_labels_by_offset: &method_labels_by_offset,
        method_arg_counts_by_offset: &method_arg_counts_by_offset,
        method_returns_value_by_offset: &method_returns_value_by_offset,
        call_targets_by_offset: &call_targets_by_offset,
        calla_targets_by_offset: &calla_targets_by_offset,
        callt_labels: &callt_labels,
        callt_param_counts: &callt_param_counts,
        callt_returns_value: &callt_returns_value,
        inline_single_use_temps: opts.inline_single_use_temps,
        emit_trace_comments: opts.emit_trace_comments,
        typed_declarations: opts.typed_declarations,
        slot_types_by_offset: &slot_types_by_offset,
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

/// Build per-method [`SlotTypes`] from the inferred [`TypeInfo`].
///
/// Static-field types are global (shared across methods), so they are resolved
/// once and cloned into each method's `SlotTypes`. Local types come from each
/// `MethodTypes.locals`. Unknowns map to `""` which the renderer treats as
/// "fall back to `var`".
fn build_slot_types_by_offset(types: &TypeInfo) -> BTreeMap<usize, SlotTypes> {
    let statics: Vec<&'static str> = types
        .statics
        .iter()
        .map(|t| inferred_type_to_csharp(*t))
        .collect();
    types
        .methods
        .iter()
        .map(|m| {
            let locals = m
                .locals
                .iter()
                .map(|t| inferred_type_to_csharp(*t))
                .collect();
            (
                m.method.offset,
                SlotTypes {
                    locals,
                    statics: statics.clone(),
                },
            )
        })
        .collect()
}
