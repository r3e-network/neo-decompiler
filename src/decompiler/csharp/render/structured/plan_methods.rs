use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::decompiler::analysis::call_graph::{CallGraph, CallTarget};
use crate::decompiler::analysis::method_contracts::{MethodContracts, ReturnBehavior};
use crate::decompiler::analysis::types::{TypeInfo, ValueType};
use crate::decompiler::cfg::method_body::{
    lower_method_body, Fidelity, LoweringIssue, LoweringIssueKind, MethodIrRequest, SymbolOrigin,
};
use crate::decompiler::cfg::ssa::CallContract;
use crate::decompiler::helpers::{
    build_method_arg_counts_by_offset, find_manifest_entry_method, offset_as_usize,
};
use crate::decompiler::ir::SemanticCallTarget;
use crate::instruction::{Instruction, OpCode};
use crate::manifest::ContractManifest;

use super::{
    collect_index_defined_symbols, plan_contract_symbols, CSharpMethodPlan, CSharpMethodPlans,
};
use crate::decompiler::csharp::helpers::{sanitize_csharp_identifier, CSharpParameter};

use super::plan_helpers::{
    cross_range_tail_target, draft_method_context, make_unique_method_name, manifest_method_draft,
    method_end, method_symbol_types, parameter_type_signature, synthetic_entry_draft,
};
use super::MethodPlanDraft;

pub(in crate::decompiler::csharp::render) fn build_csharp_method_plans(
    instructions: &[Instruction],
    manifest: Option<&ContractManifest>,
    call_graph: &CallGraph,
    method_contracts: &MethodContracts,
    types: &TypeInfo,
    inferred_method_starts: &[usize],
) -> CSharpMethodPlans {
    let entry_offset = instructions
        .first()
        .map_or(0, |instruction| instruction.offset);
    let script_end = instructions
        .last()
        .map_or(entry_offset, |instruction| instruction.offset + 1);
    let inferred_argument_counts =
        build_method_arg_counts_by_offset(instructions, inferred_method_starts, manifest);
    let mut drafts = Vec::new();
    let mut synthetic_entry = None;
    let mut fallback_entry = None;
    let mut manifest_methods = Vec::new();
    let mut inferred_methods = BTreeMap::new();

    if let Some(manifest) = manifest {
        let entry_method = instructions
            .first()
            .and_then(|_| find_manifest_entry_method(manifest, entry_offset));
        if !instructions.is_empty() && entry_method.is_none() {
            let index = drafts.len();
            drafts.push(synthetic_entry_draft(
                instructions,
                inferred_method_starts,
                entry_offset,
                script_end,
            ));
            synthetic_entry = Some(index);
        }

        let mut sorted_methods: Vec<_> = manifest.abi.methods.iter().collect();
        sorted_methods.sort_by_key(|method| method.offset.unwrap_or(i32::MAX));
        let (with_offsets, without_offsets): (Vec<_>, Vec<_>) = sorted_methods
            .into_iter()
            .partition(|method| method.offset.is_some());

        for method in with_offsets.into_iter().chain(without_offsets) {
            let explicit_start = offset_as_usize(method.offset);
            let is_offsetless_entry = explicit_start.is_none()
                && entry_method
                    .as_ref()
                    .is_some_and(|(entry, _)| std::ptr::eq(*entry, method));
            let addressable_offset = explicit_start.or(is_offsetless_entry.then_some(entry_offset));
            let start = addressable_offset.unwrap_or(entry_offset);
            let end = addressable_offset.map_or(start, |start| {
                method_end(inferred_method_starts, start, script_end)
            });
            let index = drafts.len();
            drafts.push(manifest_method_draft(
                method,
                start,
                end,
                addressable_offset,
                instructions,
            ));
            manifest_methods.push(index);
        }
    } else {
        let index = drafts.len();
        drafts.push(synthetic_entry_draft(
            instructions,
            inferred_method_starts,
            entry_offset,
            script_end,
        ));
        fallback_entry = Some(index);
    }

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
    for start in inferred_method_starts {
        if *start == entry_offset || manifest_offsets.contains(start) {
            continue;
        }
        let end = method_end(inferred_method_starts, *start, script_end);
        let slice = instructions
            .iter()
            .filter(|instruction| instruction.offset >= *start && instruction.offset < end)
            .collect::<Vec<_>>();
        if slice.is_empty()
            || slice
                .iter()
                .all(|instruction| instruction.opcode == OpCode::Nop)
        {
            continue;
        }

        let method_contract = method_contracts.get(*start);
        let argument_count = method_contract.map_or_else(
            || inferred_argument_counts.get(start).copied().unwrap_or(0),
            |contract| contract.argument_count,
        );
        let return_behavior =
            method_contract.map_or(ReturnBehavior::Unknown, |contract| contract.return_behavior);
        let parameters = (0..argument_count)
            .map(|index| CSharpParameter {
                name: format!("arg{index}"),
                ty: "dynamic".to_string(),
            })
            .collect();
        let index = drafts.len();
        drafts.push(MethodPlanDraft {
            start: *start,
            end,
            raw_name: format!("sub_0x{start:04X}"),
            parameters,
            return_type: if return_behavior == ReturnBehavior::Void {
                "void".to_string()
            } else {
                "dynamic".to_string()
            },
            return_behavior,
            arguments_on_entry_stack: instructions
                .iter()
                .find(|instruction| instruction.offset == *start)
                .is_none_or(|instruction| instruction.opcode != OpCode::Initslot),
            addressable_offset: Some(*start),
        });
        inferred_methods.insert(*start, index);
    }

    for draft in &mut drafts {
        let null_checked = super::super::nullability::null_checked_argument_indices(
            instructions,
            draft.start,
            draft.end,
        );
        for index in null_checked {
            let Some(parameter) = draft.parameters.get_mut(index) else {
                continue;
            };
            if matches!(parameter.ty.as_str(), "BigInteger" | "bool") {
                parameter.ty = "dynamic".to_string();
            }
        }
    }

    let method_symbol_maps = drafts
        .iter()
        .filter(|draft| draft.end > draft.start)
        .map(|draft| {
            lower_method_body(MethodIrRequest {
                start: draft.start,
                end: draft.end,
                instructions,
                context: draft_method_context(draft, method_contracts),
                symbol_types: method_symbol_types(types, draft.start, &draft.parameters),
            })
            .symbols
        })
        .collect::<Vec<_>>();
    let method_symbols = method_symbol_maps.iter().collect::<Vec<_>>();
    let reserved_member_names =
        plan_contract_symbols(types, &method_symbols, false, &BTreeSet::new())
            .static_fields
            .into_iter()
            .map(|field| field.name)
            .collect();
    let mut used_signatures = HashSet::new();
    let mut base_occurrences = HashMap::new();
    let mut plans = drafts
        .iter()
        .map(|draft| {
            let base_name = sanitize_csharp_identifier(&draft.raw_name);
            let emitted_name = make_unique_method_name(
                base_name,
                &parameter_type_signature(&draft.parameters),
                &mut used_signatures,
                &mut base_occurrences,
                &reserved_member_names,
            );
            CSharpMethodPlan {
                start: draft.start,
                end: draft.end,
                raw_name: draft.raw_name.clone(),
                emitted_name,
                parameters: draft.parameters.clone(),
                return_type: draft.return_type.clone(),
                return_behavior: draft.return_behavior,
                method_context: draft_method_context(draft, method_contracts),
                symbol_types: method_symbol_types(types, draft.start, &draft.parameters),
                planning_issues: Vec::new(),
            }
        })
        .collect::<Vec<_>>();

    let mut plans_by_offset: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (index, draft) in drafts.iter().enumerate() {
        if let Some(offset) = draft.addressable_offset {
            plans_by_offset.entry(offset).or_default().push(index);
        }
    }

    let mut calls_and_issues = Vec::with_capacity(plans.len());
    for plan in &plans {
        let mut calls_by_offset = BTreeMap::new();
        let mut planning_issues = Vec::new();
        for edge in call_graph
            .edges
            .iter()
            .filter(|edge| edge.call_offset >= plan.start && edge.call_offset < plan.end)
        {
            let Some(instruction) = instructions
                .iter()
                .find(|instruction| instruction.offset == edge.call_offset)
            else {
                continue;
            };
            let contract = match &edge.target {
                CallTarget::Internal { method } => match plans_by_offset.get(&method.offset) {
                    Some(candidates) if candidates.len() == 1 => {
                        let target = &plans[candidates[0]];
                        CallContract::new(
                            SemanticCallTarget::Internal {
                                offset: method.offset,
                                name: target.emitted_name.clone(),
                            },
                            target.parameters.len(),
                            target.return_behavior.returns_value(),
                        )
                        .with_may_return(
                            method_contracts
                                .get(method.offset)
                                .is_none_or(|contract| contract.may_return),
                        )
                        .with_return_shape(
                            method_contracts
                                .get(method.offset)
                                .and_then(|contract| contract.return_shape),
                        )
                        .with_argument_effects(
                            method_contracts
                                .get(method.offset)
                                .map(|contract| contract.argument_effects.clone())
                                .unwrap_or_default(),
                        )
                        .with_argument_field_writes(
                            method_contracts
                                .get(method.offset)
                                .map(|contract| contract.argument_field_writes.clone())
                                .unwrap_or_default(),
                        )
                    }
                    candidates => {
                        let declaration_count = candidates.map_or(0, Vec::len);
                        planning_issues.push(LoweringIssue {
                            offset: edge.call_offset,
                            opcode: instruction.opcode,
                            kind: LoweringIssueKind::UnresolvedCall,
                            fidelity: Fidelity::Incomplete,
                            detail: format!(
                                "internal call target 0x{:04X} matches {declaration_count} emitted C# declarations",
                                method.offset
                            ),
                        });
                        let method_contract = method_contracts.get(method.offset);
                        CallContract::new(
                            SemanticCallTarget::Unresolved {
                                display_name: format!("call_0x{:04X}", method.offset),
                            },
                            method_contract.map_or(0, |contract| contract.argument_count),
                            method_contract
                                .is_none_or(|contract| contract.return_behavior.returns_value()),
                        )
                        .with_may_return(method_contract.is_none_or(|contract| contract.may_return))
                    }
                },
                CallTarget::MethodToken {
                    index,
                    hash_le,
                    method,
                    parameters_count,
                    has_return_value,
                    call_flags,
                    ..
                } => CallContract::new(
                    SemanticCallTarget::MethodToken {
                        index: usize::from(*index),
                        name: method.clone(),
                        hash_le: Some(hash_le.clone()),
                        call_flags: Some(*call_flags),
                    },
                    usize::from(*parameters_count),
                    *has_return_value,
                ),
                _ => continue,
            };
            calls_by_offset.insert(edge.call_offset, contract);
        }
        for instruction in instructions
            .iter()
            .filter(|instruction| instruction.offset >= plan.start && instruction.offset < plan.end)
        {
            let Some(target_offset) = cross_range_tail_target(instruction, plan.start, plan.end)
            else {
                continue;
            };
            let Some(candidates) = plans_by_offset.get(&target_offset) else {
                continue;
            };
            if candidates.len() != 1 {
                continue;
            }
            let target = &plans[candidates[0]];
            let target_contract = method_contracts.get(target_offset);
            if target_contract.is_some_and(|contract| !contract.may_return) {
                continue;
            }
            calls_by_offset.insert(
                instruction.offset,
                CallContract::new(
                    SemanticCallTarget::Internal {
                        offset: target_offset,
                        name: target.emitted_name.clone(),
                    },
                    target.parameters.len(),
                    plan.return_behavior.returns_value(),
                )
                .with_may_return(true)
                .with_argument_effects(
                    target_contract
                        .map(|contract| contract.argument_effects.clone())
                        .unwrap_or_default(),
                )
                .with_argument_field_writes(
                    target_contract
                        .map(|contract| contract.argument_field_writes.clone())
                        .unwrap_or_default(),
                ),
            );
        }
        planning_issues.sort_by(|left, right| {
            (
                left.offset,
                left.opcode.byte(),
                left.kind,
                left.detail.as_str(),
            )
                .cmp(&(
                    right.offset,
                    right.opcode.byte(),
                    right.kind,
                    right.detail.as_str(),
                ))
        });
        planning_issues.dedup();
        calls_and_issues.push((calls_by_offset, planning_issues));
    }
    for (plan, (calls_by_offset, planning_issues)) in plans.iter_mut().zip(calls_and_issues) {
        plan.method_context.calls_by_offset = calls_by_offset;
        plan.planning_issues = planning_issues;
    }

    let mut parameter_index_definitions: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    let mut index_defined_statics = BTreeSet::new();
    for (plan_index, plan) in plans.iter().enumerate() {
        if plan.end <= plan.start {
            continue;
        }
        let lowered = lower_method_body(MethodIrRequest {
            start: plan.start,
            end: plan.end,
            instructions,
            context: plan.method_context.clone(),
            symbol_types: plan.symbol_types.clone(),
        });
        for name in collect_index_defined_symbols(&lowered.body) {
            match lowered.symbols.get(&name).map(|symbol| &symbol.origin) {
                Some(SymbolOrigin::Parameter(index)) => {
                    parameter_index_definitions
                        .entry(plan_index)
                        .or_default()
                        .insert(*index);
                }
                Some(SymbolOrigin::Static(index)) => {
                    index_defined_statics.insert(*index);
                }
                _ => {}
            }
        }
    }

    let mut parameter_types_changed = false;
    for (plan_index, indices) in parameter_index_definitions {
        let plan = &mut plans[plan_index];
        for index in indices {
            if let Some(parameter) = plan.parameters.get_mut(index) {
                parameter_types_changed |= parameter.ty != "dynamic";
                parameter.ty = "dynamic".to_string();
            }
            if let Some(value_type) = plan.symbol_types.parameters.get_mut(index) {
                *value_type = ValueType::Unknown;
            }
        }
    }
    for plan in &mut plans {
        if let Some(last_index) = index_defined_statics.last().copied() {
            plan.symbol_types
                .statics
                .resize(last_index + 1, ValueType::Unknown);
        }
        for index in &index_defined_statics {
            plan.symbol_types.statics[*index] = ValueType::Unknown;
        }
    }

    if parameter_types_changed {
        let mut used_signatures = HashSet::new();
        let mut base_occurrences = HashMap::new();
        for plan in &mut plans {
            plan.emitted_name = make_unique_method_name(
                sanitize_csharp_identifier(&plan.raw_name),
                &parameter_type_signature(&plan.parameters),
                &mut used_signatures,
                &mut base_occurrences,
                &reserved_member_names,
            );
        }
        let emitted_names_by_offset = plans_by_offset
            .iter()
            .filter(|(_, candidates)| candidates.len() == 1)
            .map(|(offset, candidates)| (*offset, plans[candidates[0]].emitted_name.clone()))
            .collect::<BTreeMap<_, _>>();
        for plan in &mut plans {
            for contract in plan.method_context.calls_by_offset.values_mut() {
                let SemanticCallTarget::Internal { offset, name } = &mut contract.target else {
                    continue;
                };
                if let Some(emitted_name) = emitted_names_by_offset.get(offset) {
                    *name = emitted_name.clone();
                }
            }
        }
    }

    let mut method_labels_by_offset = BTreeMap::new();
    let mut method_arg_counts_by_offset = BTreeMap::new();
    let mut method_return_types_by_offset = BTreeMap::new();
    for (offset, candidates) in &plans_by_offset {
        if candidates.len() != 1 {
            continue;
        }
        let plan = &plans[candidates[0]];
        method_labels_by_offset.insert(*offset, plan.emitted_name.clone());
        method_arg_counts_by_offset.insert(*offset, plan.parameters.len());
        method_return_types_by_offset.insert(*offset, plan.return_type.clone());
    }

    CSharpMethodPlans {
        plans,
        method_symbol_maps,
        synthetic_entry,
        fallback_entry,
        manifest_methods,
        inferred_methods,
        method_labels_by_offset,
        method_arg_counts_by_offset,
        method_return_types_by_offset,
        index_defined_statics,
    }
}
