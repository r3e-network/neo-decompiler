//! Conservative C# parameter inference for private helper methods.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::decompiler::analysis::call_graph::{CallGraph, CallTarget};
use crate::decompiler::cfg::method_body::{
    lower_method_body, Fidelity, MethodIrRequest, SymbolOrigin,
};
use crate::decompiler::csharp::render::structured::nullability;
use crate::decompiler::ir::SemanticCallTarget;

use super::super::{
    collect_indexed_base_symbols, csharp_type_value_type,
    plan_declarations_with_known_types_and_calls, CSharpMethodPlan,
};
use super::parameter_calls::collect_internal_call_arguments;

/// Promote a private-helper parameter only when every resolved incoming call
/// supplies the same concrete C# type. Manifest signatures remain authoritative.
pub(super) fn infer_private_parameter_types(
    plans: &mut [CSharpMethodPlan],
    inferred_methods: &BTreeMap<usize, usize>,
    plans_by_offset: &BTreeMap<usize, Vec<usize>>,
    instructions: &[crate::instruction::Instruction],
    call_graph: &CallGraph,
) -> bool {
    let known_call_types = concrete_return_types_by_offset(plans, plans_by_offset);
    let expected_calls = expected_private_calls(plans, inferred_methods);
    let invalid_targets = invalid_private_targets(call_graph, inferred_methods);
    let mut candidates = inferred_methods
        .keys()
        .map(|offset| {
            let count = plans_by_offset
                .get(offset)
                .and_then(|indices| indices.first())
                .and_then(|index| plans.get(*index))
                .map_or(0, |plan| plan.parameters.len());
            let mut candidates = vec![None; count];
            if invalid_targets.contains(offset) {
                invalidate(&mut candidates);
            }
            (*offset, candidates)
        })
        .collect::<BTreeMap<_, _>>();
    let mut observed_calls = BTreeMap::new();

    for caller in plans.iter() {
        let expected_targets = caller
            .method_context
            .calls_by_offset
            .values()
            .filter_map(|contract| match contract.target {
                SemanticCallTarget::Internal { offset, .. }
                    if inferred_methods.contains_key(&offset) =>
                {
                    Some(offset)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if expected_targets.is_empty() || caller.end <= caller.start {
            continue;
        }

        let lowered = lower_method_body(MethodIrRequest {
            start: caller.start,
            end: caller.end,
            instructions,
            context: caller.method_context.clone(),
            symbol_types: caller.symbol_types.clone(),
            reduce_temps: false,
        });
        let call_arguments = if lowered.fidelity.status == Fidelity::Incomplete {
            HashMap::new()
        } else {
            let parameter_types = caller
                .parameters
                .iter()
                .filter(|parameter| is_concrete_type(&parameter.ty))
                .map(|parameter| (parameter.name.clone(), parameter.ty.clone()))
                .collect::<BTreeMap<_, _>>();
            let declarations = plan_declarations_with_known_types_and_calls(
                &lowered.body,
                &lowered.symbols,
                true,
                &parameter_types,
                &known_call_types,
            );
            let known_types = parameter_types
                .into_iter()
                .chain(
                    declarations
                        .declarations
                        .iter()
                        .filter(|(_, declaration)| is_concrete_type(&declaration.csharp_type))
                        .map(|(name, declaration)| (name.clone(), declaration.csharp_type.clone())),
                )
                .collect::<BTreeMap<_, _>>();
            collect_internal_call_arguments(
                &lowered.body,
                &lowered,
                &known_types,
                &known_call_types,
            )
        };

        let expected_by_target = expected_targets.into_iter().fold(
            BTreeMap::<usize, usize>::new(),
            |mut counts, target| {
                *counts.entry(target).or_default() += 1;
                counts
            },
        );
        for (target, expected_count) in expected_by_target {
            *observed_calls.entry(target).or_default() += expected_count;
            let Some(target_candidates) = candidates.get_mut(&target) else {
                continue;
            };
            let Some(argument_lists) = call_arguments.get(&target) else {
                invalidate(target_candidates);
                continue;
            };
            let Some(target_index) = inferred_methods.get(&target).copied() else {
                continue;
            };
            let Some(target_plan) = plans.get(target_index) else {
                continue;
            };
            if argument_lists.len() != expected_count {
                invalidate(target_candidates);
                continue;
            }
            for arguments in argument_lists {
                if arguments.len() != target_plan.parameters.len() {
                    invalidate(target_candidates);
                    continue;
                }
                for (index, argument) in arguments.iter().enumerate() {
                    let Some(candidate) = argument.as_deref() else {
                        target_candidates[index] = Some(String::new());
                        continue;
                    };
                    match target_candidates[index].as_deref() {
                        None => target_candidates[index] = Some(candidate.to_string()),
                        Some(existing) if existing.is_empty() || existing == candidate => {}
                        Some(_) => target_candidates[index] = Some(String::new()),
                    }
                }
            }
        }
    }

    let mut changed = false;
    for (offset, target_candidates) in candidates {
        if observed_calls.get(&offset).copied().unwrap_or(0)
            != expected_calls.get(&offset).copied().unwrap_or(0)
        {
            continue;
        }
        let Some(target_index) = inferred_methods.get(&offset).copied() else {
            continue;
        };
        let Some(target_plan) = plans.get_mut(target_index) else {
            continue;
        };
        let null_checked = nullability::null_checked_argument_indices(
            instructions,
            target_plan.start,
            target_plan.end,
        );
        let indexed = indexed_parameter_indices(target_plan, instructions);
        for (index, candidate) in target_candidates.into_iter().enumerate() {
            let Some(candidate) = candidate.filter(|candidate| !candidate.is_empty()) else {
                continue;
            };
            if (null_checked.contains(&index) && !is_nullable_csharp_type(&candidate))
                || (indexed.contains(&index) && !is_indexable_csharp_type(&candidate))
            {
                continue;
            }
            let Some(parameter) = target_plan.parameters.get_mut(index) else {
                continue;
            };
            if parameter.ty != "dynamic" || !is_concrete_type(&candidate) {
                continue;
            }
            parameter.ty = candidate.clone();
            if let Some(value_type) = csharp_type_value_type(&candidate) {
                if let Some(slot_type) = target_plan.symbol_types.parameters.get_mut(index) {
                    *slot_type = value_type;
                }
            }
            changed = true;
        }
    }
    changed
}

fn indexed_parameter_indices(
    plan: &CSharpMethodPlan,
    instructions: &[crate::instruction::Instruction],
) -> BTreeSet<usize> {
    if plan.end <= plan.start {
        return BTreeSet::new();
    }
    let lowered = lower_method_body(MethodIrRequest {
        start: plan.start,
        end: plan.end,
        instructions,
        context: plan.method_context.clone(),
        symbol_types: plan.symbol_types.clone(),
        reduce_temps: false,
    });
    collect_indexed_base_symbols(&lowered.body)
        .into_iter()
        .filter_map(
            |name| match lowered.symbols.get(&name).map(|symbol| &symbol.origin) {
                Some(SymbolOrigin::Parameter(index)) => Some(*index),
                _ => None,
            },
        )
        .collect()
}

fn expected_private_calls(
    plans: &[CSharpMethodPlan],
    inferred_methods: &BTreeMap<usize, usize>,
) -> BTreeMap<usize, usize> {
    let mut expected = BTreeMap::new();
    for plan in plans {
        for contract in plan.method_context.calls_by_offset.values() {
            if let SemanticCallTarget::Internal { offset, .. } = contract.target {
                if inferred_methods.contains_key(&offset) {
                    *expected.entry(offset).or_default() += 1;
                }
            }
        }
    }
    expected
}

fn invalid_private_targets(
    call_graph: &CallGraph,
    inferred_methods: &BTreeMap<usize, usize>,
) -> std::collections::BTreeSet<usize> {
    let has_indirect_call = call_graph
        .edges
        .iter()
        .any(|edge| matches!(&edge.target, CallTarget::Indirect { .. }));
    let mut invalid = if has_indirect_call {
        inferred_methods.keys().copied().collect()
    } else {
        std::collections::BTreeSet::new()
    };
    for edge in &call_graph.edges {
        let CallTarget::UnresolvedInternal { target } = &edge.target else {
            continue;
        };
        let Ok(target) = usize::try_from(*target) else {
            continue;
        };
        if inferred_methods.contains_key(&target) {
            invalid.insert(target);
        }
    }
    invalid
}

fn concrete_return_types_by_offset(
    plans: &[CSharpMethodPlan],
    plans_by_offset: &BTreeMap<usize, Vec<usize>>,
) -> BTreeMap<usize, String> {
    plans_by_offset
        .iter()
        .filter_map(|(offset, candidates)| {
            let [plan_index] = candidates.as_slice() else {
                return None;
            };
            let return_type = plans.get(*plan_index)?.return_type.as_str();
            is_concrete_type(return_type).then(|| (*offset, return_type.to_string()))
        })
        .collect()
}

fn invalidate(candidates: &mut [Option<String>]) {
    for candidate in candidates {
        *candidate = Some(String::new());
    }
}

fn is_concrete_type(type_name: &str) -> bool {
    !matches!(type_name, "" | "dynamic" | "object" | "void")
        && csharp_type_value_type(type_name).is_some()
}

/// A proven concrete parameter may still be indexed when its C# type exposes
/// the same family of index operation as the VM value. Unknown or scalar
/// candidates remain dynamic so an invalid VM call is not turned into a C#
/// compile-time type error.
fn is_indexable_csharp_type(type_name: &str) -> bool {
    type_name.ends_with("[]")
        || matches!(type_name, "ByteString" | "Map<object, object>" | "string")
}

fn is_nullable_csharp_type(type_name: &str) -> bool {
    type_name.ends_with("[]")
        || matches!(
            type_name,
            "ByteString" | "Map<object, object>" | "string" | "object"
        )
}
