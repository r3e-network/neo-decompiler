//! C# skeleton renderer.
//!
//! The renderer produces a Neo SmartContract Framework-compatible skeleton
//! (methods, events, and manifest metadata) and optionally includes lifted
//! pseudo-bodies when method offsets are available.

use super::super::analysis::call_graph::CallGraph;
use super::super::analysis::method_contracts::MethodContracts;
use super::super::analysis::patterns::identify_patterns;
use super::super::analysis::types::{TypeInfo, ValueType};
use crate::decompiler::output_format::RenderOptions;
use crate::instruction::Instruction;
use crate::instruction::OpCode;
use crate::manifest::ContractManifest;
use crate::nef::NefFile;
use std::collections::{BTreeMap, HashSet};

use super::super::helpers::{
    extract_contract_name, inferred_method_starts, stack_item_type_tag, value_type_from_operand,
};
use super::helpers::{
    make_unique_identifier, sanitize_csharp_identifier, VM_ASSERT_MESSAGE_HELPER, VM_EXCEPTION_TYPE,
};

mod body;
pub(crate) mod events;
mod header;
mod methods;
mod structured;

pub(crate) use body::BodyBackend;

#[derive(Debug, Clone)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct MethodBodyCoverage {
    pub(crate) backend: BodyBackend,
    pub(crate) fidelity: crate::decompiler::cfg::method_body::FidelityReport,
    pub(crate) primary_issue: Option<crate::decompiler::cfg::method_body::LoweringIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct MethodCoverageKey {
    emitted_name: String,
    parameter_types: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct CSharpCoverage {
    pub(crate) methods: BTreeMap<usize, BTreeMap<MethodCoverageKey, MethodBodyCoverage>>,
    pub(crate) backend_counts: BTreeMap<&'static str, usize>,
    pub(crate) issue_counts:
        BTreeMap<crate::decompiler::cfg::method_body::LoweringIssueKind, usize>,
}

impl CSharpCoverage {
    #[cfg(test)]
    pub(crate) fn method(&self, start: usize, name: &str) -> Option<&MethodBodyCoverage> {
        self.methods
            .get(&start)?
            .iter()
            .find_map(|(key, coverage)| (key.emitted_name == name).then_some(coverage))
    }

    fn record(
        &mut self,
        method_plan: &structured::plan::CSharpMethodPlan,
        result: &body::BodyRenderResult,
    ) {
        let key = MethodCoverageKey {
            emitted_name: method_plan.emitted_name.clone(),
            parameter_types: method_plan
                .parameters
                .iter()
                .map(|parameter| parameter.ty.clone())
                .collect(),
        };
        self.methods.entry(method_plan.start).or_default().insert(
            key,
            MethodBodyCoverage {
                backend: result.backend,
                fidelity: result.fidelity.clone(),
                primary_issue: result.fidelity.primary_issue().cloned(),
            },
        );
        let backend = match result.backend {
            BodyBackend::Structured => "structured",
            BodyBackend::ThrowingStub => "throwing_stub",
        };
        *self.backend_counts.entry(backend).or_default() += 1;
        for issue in &result.fidelity.issues {
            *self.issue_counts.entry(issue.kind).or_default() += 1;
        }
    }
}

pub(crate) struct CSharpRender {
    pub(crate) source: String,
    pub(crate) warnings: Vec<String>,
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) coverage: CSharpCoverage,
}

#[derive(Debug, Clone)]
pub(super) struct TaggedOpcodeHelper {
    pub(super) opcode: OpCode,
    pub(super) target: ValueType,
    pub(super) name: String,
}

const UNPACK_PACKSTRUCT_HELPER: &str = "__NeoDecompilerUnpackPackStruct";
const BARE_THROW_HELPER: &str = "__NeoDecompilerBareThrow";

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
    let patterns = identify_patterns(nef, instructions, manifest);
    let event_signatures = manifest.map(events::event_signatures).unwrap_or_default();
    let mut output = String::new();
    let mut warnings = Vec::new();
    let mut coverage = CSharpCoverage::default();
    header::write_preamble(&mut output);

    let contract_name = extract_contract_name(manifest, sanitize_csharp_identifier);

    let inferred_starts = inferred_method_starts(instructions, manifest);
    let method_plans = structured::plan::build_csharp_method_plans(
        instructions,
        manifest,
        call_graph,
        method_contracts,
        types,
        &inferred_starts,
    );
    let method_symbols = method_plans.method_symbol_maps().iter().collect::<Vec<_>>();
    let contract_symbols = structured::plan::plan_contract_symbols(
        types,
        &method_symbols,
        opts.typed_declarations,
        method_plans.index_defined_statics(),
    );
    let static_field_types = contract_symbols
        .static_fields
        .iter()
        .map(|field| (field.name.clone(), field.csharp_type.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut used_member_names =
        contract_member_names(&contract_name, manifest, &method_plans, &contract_symbols);
    let vm_exception_type = vm_exception_type_name(instructions, &mut used_member_names);
    let vm_exception_type_ref = vm_exception_type.as_deref().unwrap_or(VM_EXCEPTION_TYPE);
    let assert_message_helper = assert_message_helper_name(instructions, &mut used_member_names);
    let assert_message_helper_call = assert_message_helper
        .as_ref()
        .map(|helper| format!("global::NeoDecompiler.Generated.{contract_name}.{helper}"));
    let unpack_packstruct_helper =
        unpack_packstruct_helper_name(instructions, &mut used_member_names);
    let unpack_packstruct_helper_call = unpack_packstruct_helper
        .as_ref()
        .map(|helper| format!("global::NeoDecompiler.Generated.{contract_name}.{helper}"));
    let bare_throw_helper = bare_throw_helper_name(instructions, &mut used_member_names);
    let bare_throw_helper_call = bare_throw_helper
        .as_ref()
        .map(|helper| format!("global::NeoDecompiler.Generated.{contract_name}.{helper}"));
    let tagged_opcode_helpers = tagged_opcode_helpers(instructions, &mut used_member_names);
    let tagged_opcode_helper_calls = tagged_opcode_helpers
        .iter()
        .filter_map(|helper| {
            structured::expr::tagged_opcode_helper_key(helper.opcode, helper.target).map(|key| {
                (
                    key,
                    format!(
                        "global::NeoDecompiler.Generated.{contract_name}.{}",
                        helper.name
                    ),
                )
            })
        })
        .collect();
    // Pre-resolve inferred C# slot types per method so that body-local
    // declarations can be rendered with concrete types (`BigInteger loc0`)
    // instead of `var` when `typed_declarations` is enabled. Built from the
    // already-computed `TypeInfo`; cheap (one entry per method).
    let body_context = body::LiftedBodyContext {
        method_labels_by_offset: method_plans.method_labels_by_offset(),
        method_arg_counts_by_offset: method_plans.method_arg_counts_by_offset(),
        method_return_types_by_offset: method_plans.method_return_types_by_offset(),
        inline_single_use_temps: opts.inline_single_use_temps,
        emit_trace_comments: opts.emit_trace_comments,
        typed_declarations: opts.typed_declarations,
        vm_exception_type: vm_exception_type_ref,
        assert_message_helper_call: assert_message_helper_call.as_deref(),
        bare_throw_helper_call: bare_throw_helper_call.as_deref(),
        unpack_packstruct_helper_call: unpack_packstruct_helper_call.as_deref(),
        tagged_opcode_helper_calls: &tagged_opcode_helper_calls,
        static_field_types: &static_field_types,
        event_signatures: &event_signatures,
    };
    let methods_context = methods::MethodsContext {
        instructions,
        inferred_method_starts: &inferred_starts,
        method_plans: &method_plans,
        body_context,
    };

    header::write_contract_open(&mut output, &contract_name, nef, manifest);
    header::write_pattern_comments(&mut output, &patterns);
    header::write_static_fields(&mut output, &contract_symbols);
    header::write_vm_exception_type(&mut output, vm_exception_type.as_deref());
    header::write_assert_message_helper(&mut output, assert_message_helper.as_deref());
    header::write_bare_throw_helper(&mut output, bare_throw_helper.as_deref());
    header::write_unpack_packstruct_helper(&mut output, unpack_packstruct_helper.as_deref());
    header::write_tagged_opcode_helpers(&mut output, &tagged_opcode_helpers);
    if call_graph.edges.iter().any(|edge| {
        matches!(
            edge.target,
            super::super::analysis::call_graph::CallTarget::Indirect { .. }
                | super::super::analysis::call_graph::CallTarget::UnresolvedInternal { .. }
        )
    }) {
        header::write_unresolved_call_helper(&mut output);
    }

    if let Some(manifest) = manifest {
        events::write_events(&mut output, manifest);
        methods::write_manifest_methods(
            &mut output,
            manifest,
            &methods_context,
            &mut warnings,
            &mut coverage,
        );
        methods::write_inferred_methods(
            &mut output,
            &methods_context,
            Some(manifest),
            &mut warnings,
            &mut coverage,
        );
    } else {
        methods::write_fallback_entry(&mut output, &methods_context, &mut warnings, &mut coverage);
        methods::write_inferred_methods(
            &mut output,
            &methods_context,
            None,
            &mut warnings,
            &mut coverage,
        );
    }

    header::write_contract_close(&mut output);
    CSharpRender {
        source: output,
        warnings,
        coverage,
    }
}

fn vm_exception_type_name(
    instructions: &[Instruction],
    used_names: &mut HashSet<String>,
) -> Option<String> {
    instructions
        .iter()
        .any(|instruction| {
            matches!(
                instruction.opcode,
                OpCode::Throw | OpCode::Try | OpCode::TryL
            )
        })
        .then(|| make_unique_identifier(VM_EXCEPTION_TYPE.to_string(), used_names))
}

fn contract_member_names(
    contract_name: &str,
    manifest: Option<&ContractManifest>,
    method_plans: &structured::plan::CSharpMethodPlans,
    contract_symbols: &structured::plan::CSharpContractSymbols,
) -> HashSet<String> {
    let mut used_names = HashSet::from([contract_name.to_string()]);
    used_names.extend(method_plans.emitted_names().map(str::to_string));
    used_names.extend(
        contract_symbols
            .static_fields
            .iter()
            .map(|field| field.name.clone()),
    );

    if let Some(manifest) = manifest {
        let mut event_names = HashSet::new();
        for event in &manifest.abi.events {
            let emitted =
                make_unique_identifier(sanitize_csharp_identifier(&event.name), &mut event_names);
            used_names.insert(emitted);
        }
    }

    used_names
}

fn assert_message_helper_name(
    instructions: &[Instruction],
    used_names: &mut HashSet<String>,
) -> Option<String> {
    if !instructions
        .iter()
        .any(|instruction| instruction.opcode == OpCode::Assertmsg)
    {
        return None;
    }

    Some(make_unique_identifier(
        VM_ASSERT_MESSAGE_HELPER.to_string(),
        used_names,
    ))
}

fn unpack_packstruct_helper_name(
    instructions: &[Instruction],
    used_names: &mut HashSet<String>,
) -> Option<String> {
    instructions
        .windows(2)
        .any(|pair| pair[0].opcode == OpCode::Unpack && pair[1].opcode == OpCode::Packstruct)
        .then(|| make_unique_identifier(UNPACK_PACKSTRUCT_HELPER.to_string(), used_names))
}

fn bare_throw_helper_name(
    instructions: &[Instruction],
    used_names: &mut HashSet<String>,
) -> Option<String> {
    instructions
        .windows(2)
        .any(|pair| pair[0].opcode == OpCode::Drop && pair[1].opcode == OpCode::Throw)
        .then(|| make_unique_identifier(BARE_THROW_HELPER.to_string(), used_names))
}

fn tagged_opcode_helpers(
    instructions: &[Instruction],
    used_names: &mut HashSet<String>,
) -> Vec<TaggedOpcodeHelper> {
    let mut required = BTreeMap::new();
    for instruction in instructions {
        let requirement = match instruction.opcode {
            OpCode::Convert | OpCode::Istype => instruction
                .operand
                .as_ref()
                .and_then(value_type_from_operand)
                .filter(|target| *target != ValueType::Any)
                .map(|target| (instruction.opcode, target)),
            OpCode::Packstruct => Some((OpCode::Convert, ValueType::Struct)),
            _ => None,
        };
        let Some((opcode, target)) = requirement else {
            continue;
        };
        let Some(tag) = stack_item_type_tag(target) else {
            continue;
        };
        required
            .entry((opcode.byte(), tag))
            .or_insert((opcode, target));
    }

    required
        .into_values()
        .map(|(opcode, target)| TaggedOpcodeHelper {
            opcode,
            target,
            name: make_unique_identifier(
                structured::expr::default_tagged_opcode_helper_name(opcode, target),
                used_names,
            ),
        })
        .collect()
}
