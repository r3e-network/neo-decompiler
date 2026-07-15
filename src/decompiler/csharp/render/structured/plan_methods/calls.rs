//! Build the resolved call contracts used by each C# method plan.

use std::collections::BTreeMap;

use crate::decompiler::analysis::call_graph::{CallGraph, CallTarget};
use crate::decompiler::analysis::method_contracts::MethodContracts;
use crate::decompiler::cfg::method_body::{Fidelity, LoweringIssue, LoweringIssueKind};
use crate::decompiler::cfg::ssa::CallContract;
use crate::decompiler::ir::SemanticCallTarget;
use crate::instruction::Instruction;

use super::super::plan_helpers::cross_range_tail_target;
use super::super::CSharpMethodPlan;

/// Attach per-offset call contracts and planning diagnostics to every method.
///
/// Keeping this interprocedural pass separate from draft construction makes the
/// method planner easier to audit: declaration shape and call resolution are
/// independent phases, even though both feed the final `MethodContext`.
pub(super) fn attach_call_plans(
    plans: &mut [CSharpMethodPlan],
    plans_by_offset: &BTreeMap<usize, Vec<usize>>,
    instructions: &[Instruction],
    call_graph: &CallGraph,
    method_contracts: &MethodContracts,
) {
    let instructions_by_offset = instructions
        .iter()
        .map(|instruction| (instruction.offset, instruction))
        .collect::<BTreeMap<_, _>>();
    let mut calls_and_issues = Vec::with_capacity(plans.len());
    for plan in plans.iter() {
        let mut calls_by_offset = BTreeMap::new();
        let mut planning_issues = Vec::new();
        for edge in call_graph
            .edges
            .iter()
            .filter(|edge| edge.call_offset >= plan.start && edge.call_offset < plan.end)
        {
            let Some(instruction) = instructions_by_offset.get(&edge.call_offset) else {
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
                        .with_return_facts(
                            method_contracts
                                .get(method.offset)
                                .and_then(|contract| contract.return_collection_facts.clone()),
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
                .with_return_shape(target_contract.and_then(|contract| contract.return_shape))
                .with_return_facts(
                    target_contract.and_then(|contract| contract.return_collection_facts.clone()),
                )
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
}
