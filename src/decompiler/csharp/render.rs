//! C# skeleton renderer.
//!
//! The renderer produces a Neo SmartContract Framework-compatible skeleton
//! (methods, events, and manifest metadata) and optionally includes lifted
//! pseudo-bodies when method offsets are available.

use super::super::analysis::call_graph::CallGraph;
use super::super::analysis::types::TypeInfo;
use crate::decompiler::output_format::RenderOptions;
use crate::instruction::Instruction;
use crate::manifest::ContractManifest;
use crate::native_contracts;
use crate::nef::NefFile;
use std::collections::{BTreeMap, HashSet};

use super::super::helpers::{
    build_call_targets_by_offset, build_calla_targets_by_offset, build_method_arg_counts_by_offset,
    extract_contract_name, find_manifest_entry_method, inferred_method_starts,
    inferred_type_to_csharp, make_unique_identifier, offset_as_usize,
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
    let method_labels_by_offset =
        build_method_labels_by_offset(instructions, &inferred_starts, manifest);
    let method_arg_counts_by_offset =
        build_method_arg_counts_by_offset(instructions, &inferred_starts, manifest);
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
