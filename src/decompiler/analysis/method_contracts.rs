//! Shared stack-call contracts for manifest and inferred methods.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::decompiler::cfg::method_body::{build_method_cfg_with_non_returning_calls, Fidelity};
use crate::decompiler::cfg::method_view::{extract_method_cfgs, MethodView};
use crate::decompiler::cfg::ssa::builder::{SsaCollectionAnalysis, StaticCollectionWrite};
use crate::decompiler::cfg::ssa::{
    CallContract, MethodContext, SsaBuilder, SsaExpr, SsaStmt, SsaVariable,
};
pub use crate::decompiler::cfg::ssa::{
    CollectionArgumentEffect, CollectionShape, CollectionShapeFacts,
};
use crate::decompiler::cfg::Terminator;
use crate::decompiler::helpers::{
    build_method_arg_counts_by_offset, build_method_returns_value_by_offset,
};
use crate::decompiler::ir::{Intrinsic, SemanticCallTarget};
use crate::instruction::{Instruction, OpCode, Operand};
use crate::manifest::ContractManifest;

use super::call_graph::{CallGraph, CallTarget};
use super::{MethodRef, MethodTable};

mod collection;

use collection::{
    infer_argument_field_writes, infer_entry_and_static_collection_facts, method_argument_effects,
};

/// Whether a method is known to return a value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ReturnBehavior {
    /// A manifest declaration guarantees that the method returns a value.
    Value,
    /// A manifest declaration or conservative inference proves a bare return.
    Void,
    /// No declaration or safe inference establishes the return behavior.
    #[default]
    Unknown,
}

impl ReturnBehavior {
    pub(crate) const fn returns_value(self) -> bool {
        !matches!(self, Self::Void)
    }
}

/// Stack-call metadata for one method in a script.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MethodContract {
    /// Method identified by bytecode offset and stable display name.
    pub method: MethodRef,
    /// Number of values consumed from the evaluation stack by a call.
    pub argument_count: usize,
    /// Declared or inferred return behavior.
    pub return_behavior: ReturnBehavior,
    /// Whether execution can return normally to a caller.
    pub may_return: bool,
    /// Exact collection shape shared by every reachable normal return.
    pub return_shape: Option<CollectionShape>,
    /// Per-argument effects on fixed collection shape.
    pub argument_effects: Vec<CollectionArgumentEffect>,
    /// Fixed collection facts shared by every resolved incoming call.
    pub argument_collection_facts: Vec<CollectionShapeFacts>,
    /// Fixed constant-index shapes guaranteed on every normal return.
    pub argument_field_writes: Vec<BTreeMap<usize, CollectionShape>>,
}

/// Deterministic method-contract analysis for a script.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct MethodContracts {
    /// Contracts sorted by method entry offset.
    pub methods: Vec<MethodContract>,
    /// Fixed shapes shared by every non-null write to each static slot.
    pub static_collection_facts: BTreeMap<usize, CollectionShapeFacts>,
}

impl MethodContracts {
    /// Find the contract whose method begins at `offset`.
    #[must_use]
    pub fn get(&self, offset: usize) -> Option<&MethodContract> {
        self.methods
            .binary_search_by_key(&offset, |contract| contract.method.offset)
            .ok()
            .map(|index| &self.methods[index])
    }

    pub(crate) fn argument_counts_by_offset(&self) -> BTreeMap<usize, usize> {
        self.methods
            .iter()
            .map(|contract| (contract.method.offset, contract.argument_count))
            .collect()
    }

    pub(crate) fn returns_value_by_offset(&self) -> BTreeMap<usize, bool> {
        self.methods
            .iter()
            .map(|contract| {
                (
                    contract.method.offset,
                    contract.return_behavior.returns_value(),
                )
            })
            .collect()
    }
}

/// Infer shared stack-call contracts for every stable method in `call_graph`.
///
/// Manifest declarations are authoritative. Undeclared internal callees begin
/// as [`ReturnBehavior::Unknown`] and transition only to
/// [`ReturnBehavior::Void`] when SSA observes at least one return and every
/// observed return is bare. Unknown calls remain conservatively value-producing
/// while the fixed point is evaluated.
#[must_use]
pub fn infer_method_contracts(
    instructions: &[Instruction],
    manifest: Option<&ContractManifest>,
    call_graph: &CallGraph,
) -> MethodContracts {
    let methods_by_offset: BTreeMap<usize, MethodRef> = call_graph
        .methods
        .iter()
        .cloned()
        .map(|method| (method.offset, method))
        .collect();
    let method_starts: Vec<_> = methods_by_offset.keys().copied().collect();
    let argument_counts = build_method_arg_counts_by_offset(instructions, &method_starts, manifest);
    let declared_returns = build_method_returns_value_by_offset(instructions, manifest);

    let mut contracts: BTreeMap<usize, MethodContract> = methods_by_offset
        .into_iter()
        .map(|(offset, method)| {
            let return_behavior =
                declared_returns
                    .get(&offset)
                    .map_or(ReturnBehavior::Unknown, |returns_value| {
                        if *returns_value {
                            ReturnBehavior::Value
                        } else {
                            ReturnBehavior::Void
                        }
                    });
            (
                offset,
                MethodContract {
                    method,
                    argument_count: argument_counts.get(&offset).copied().unwrap_or(0),
                    return_behavior,
                    may_return: true,
                    return_shape: None,
                    argument_effects: vec![
                        CollectionArgumentEffect::Unknown;
                        argument_counts.get(&offset).copied().unwrap_or(0)
                    ],
                    argument_collection_facts: vec![
                        CollectionShapeFacts::default();
                        argument_counts
                            .get(&offset)
                            .copied()
                            .unwrap_or(0)
                    ],
                    argument_field_writes: vec![
                        BTreeMap::new();
                        argument_counts.get(&offset).copied().unwrap_or(0)
                    ],
                },
            )
        })
        .collect();

    let candidates: BTreeSet<_> = call_graph
        .edges
        .iter()
        .filter_map(|edge| match &edge.target {
            CallTarget::Internal { method }
                if !declared_returns.contains_key(&method.offset)
                    && contracts.contains_key(&method.offset) =>
            {
                Some(method.offset)
            }
            _ => None,
        })
        .collect();
    let table = MethodTable::new(instructions, manifest);
    let views = extract_method_cfgs(&table, instructions);
    let views_by_offset: BTreeMap<_, _> = views
        .iter()
        .map(|view| (view.method.offset, view))
        .collect();

    loop {
        let calls_by_offset = build_call_contracts(call_graph, &contracts);
        let newly_non_returning: Vec<_> = views_by_offset
            .iter()
            .filter(|(offset, _)| {
                contracts
                    .get(*offset)
                    .is_some_and(|contract| contract.may_return)
            })
            .filter(|(_, view)| !method_may_return(view, &calls_by_offset))
            .map(|(offset, _)| *offset)
            .collect();
        let newly_void: Vec<_> = candidates
            .iter()
            .copied()
            .filter(|offset| {
                contracts
                    .get(offset)
                    .is_some_and(|contract| contract.return_behavior == ReturnBehavior::Unknown)
            })
            .filter(|offset| {
                let Some(view) = views_by_offset.get(offset).copied() else {
                    return false;
                };
                let argument_count = contracts
                    .get(offset)
                    .map_or(0, |contract| contract.argument_count);
                method_has_only_bare_returns(view, &calls_by_offset, argument_count)
            })
            .collect();

        if newly_void.is_empty() && newly_non_returning.is_empty() {
            break;
        }
        for offset in newly_non_returning {
            if let Some(contract) = contracts.get_mut(&offset) {
                contract.may_return = false;
            }
        }
        for offset in newly_void {
            if let Some(contract) = contracts.get_mut(&offset) {
                contract.return_behavior = ReturnBehavior::Void;
            }
        }
    }

    let calls_by_offset = build_call_contracts(call_graph, &contracts);
    let argument_effects = views_by_offset
        .iter()
        .filter_map(|(offset, view)| {
            let contract = contracts.get(offset)?;
            Some((
                *offset,
                method_argument_effects(
                    view,
                    &calls_by_offset,
                    contract.argument_count,
                    &[],
                    &BTreeMap::new(),
                ),
            ))
        })
        .collect::<Vec<_>>();
    for (offset, effects) in argument_effects {
        if let Some(contract) = contracts.get_mut(&offset) {
            contract.argument_effects = effects;
        }
    }
    let calls_by_offset = build_call_contracts(call_graph, &contracts);
    let return_shapes = views_by_offset
        .iter()
        .filter_map(|(offset, view)| {
            let contract = contracts.get(offset)?;
            (contract.may_return && contract.return_behavior.returns_value()).then(|| {
                (
                    *offset,
                    method_return_shape(view, &calls_by_offset, contract.argument_count),
                )
            })
        })
        .collect::<Vec<_>>();
    for (offset, return_shape) in return_shapes {
        if let Some(contract) = contracts.get_mut(&offset) {
            contract.return_shape = return_shape;
        }
    }

    infer_argument_field_writes(&views_by_offset, call_graph, &mut contracts);
    let static_collection_facts = infer_entry_and_static_collection_facts(
        instructions,
        manifest,
        call_graph,
        &views_by_offset,
        &mut contracts,
    );

    MethodContracts {
        methods: contracts.into_values().collect(),
        static_collection_facts,
    }
}

fn method_return_shape(
    view: &MethodView,
    calls_by_offset: &BTreeMap<usize, CallContract>,
    argument_count: usize,
) -> Option<CollectionShape> {
    if view.instructions.len() > crate::decompiler::high_level::MAX_HIGH_LEVEL_METHOD_INSTRUCTIONS {
        return None;
    }
    let context = MethodContext {
        argument_names: (0..argument_count)
            .map(|index| format!("arg{index}"))
            .collect(),
        arguments_on_entry_stack: view
            .instructions
            .first()
            .is_none_or(|instruction| instruction.opcode != OpCode::Initslot),
        returns_value: Some(true),
        calls_by_offset: calls_for_view(view, calls_by_offset),
        argument_collection_facts: Vec::new(),
        static_collection_facts: BTreeMap::new(),
    };
    let non_returning_calls: BTreeSet<usize> = calls_by_offset
        .iter()
        .filter_map(|(offset, contract)| {
            (!contract.may_return && *offset >= view.method.offset && *offset < view.end)
                .then_some(*offset)
        })
        .collect();
    let rebuilt_cfg;
    let cfg = if non_returning_calls.is_empty() {
        &view.cfg
    } else {
        rebuilt_cfg = build_method_cfg_with_non_returning_calls(
            &view.instructions,
            view.method.offset,
            view.end,
            &non_returning_calls,
        );
        &rebuilt_cfg
    };

    SsaBuilder::new(cfg, &view.instructions)
        .with_method_context(&context)
        .build_with_report()
        .return_shape
}

fn method_may_return(view: &MethodView, calls_by_offset: &BTreeMap<usize, CallContract>) -> bool {
    let non_returning_calls: BTreeSet<usize> = calls_by_offset
        .iter()
        .filter_map(|(offset, contract)| {
            (!contract.may_return && *offset >= view.method.offset && *offset < view.end)
                .then_some(*offset)
        })
        .collect();
    let rebuilt_cfg;
    let cfg = if non_returning_calls.is_empty() {
        &view.cfg
    } else {
        rebuilt_cfg = build_method_cfg_with_non_returning_calls(
            &view.instructions,
            view.method.offset,
            view.end,
            &non_returning_calls,
        );
        &rebuilt_cfg
    };
    cfg.reachable_blocks().into_iter().any(|block_id| {
        cfg.block(block_id).is_some_and(|block| {
            matches!(block.terminator, Terminator::Return | Terminator::Unknown)
        })
    })
}

fn method_has_only_bare_returns(
    view: &MethodView,
    calls_by_offset: &BTreeMap<usize, CallContract>,
    argument_count: usize,
) -> bool {
    let context = MethodContext {
        argument_names: (0..argument_count)
            .map(|index| format!("arg{index}"))
            .collect(),
        arguments_on_entry_stack: view
            .instructions
            .first()
            .is_none_or(|instruction| instruction.opcode != OpCode::Initslot),
        calls_by_offset: calls_for_view(view, calls_by_offset),
        ..MethodContext::default()
    };
    let non_returning_calls: BTreeSet<usize> = calls_by_offset
        .iter()
        .filter_map(|(offset, contract)| {
            (!contract.may_return && *offset >= view.method.offset && *offset < view.end)
                .then_some(*offset)
        })
        .collect();
    let rebuilt_cfg;
    let cfg = if non_returning_calls.is_empty() {
        &view.cfg
    } else {
        rebuilt_cfg = build_method_cfg_with_non_returning_calls(
            &view.instructions,
            view.method.offset,
            view.end,
            &non_returning_calls,
        );
        &rebuilt_cfg
    };
    let ssa = SsaBuilder::new(cfg, &view.instructions)
        .with_method_context(&context)
        .build();
    let returns: Vec<_> = ssa
        .blocks_iter()
        .flat_map(|(_, block)| &block.stmts)
        .filter_map(|statement| match statement {
            SsaStmt::Return(value) => Some(value.is_none()),
            _ => None,
        })
        .collect();

    !returns.is_empty() && returns.iter().all(|is_bare| *is_bare)
}

fn calls_for_view(
    view: &MethodView,
    calls_by_offset: &BTreeMap<usize, CallContract>,
) -> BTreeMap<usize, CallContract> {
    view.instructions
        .iter()
        .filter_map(|instruction| {
            calls_by_offset
                .get(&instruction.offset)
                .cloned()
                .map(|contract| (instruction.offset, contract))
        })
        .collect()
}

fn build_call_contracts(
    call_graph: &CallGraph,
    contracts: &BTreeMap<usize, MethodContract>,
) -> BTreeMap<usize, CallContract> {
    let mut calls = BTreeMap::new();
    for edge in &call_graph.edges {
        let contract = match &edge.target {
            CallTarget::Internal { method } => {
                let method_contract = contracts.get(&method.offset);
                CallContract::new(
                    SemanticCallTarget::Internal {
                        offset: method.offset,
                        name: method.name.clone(),
                    },
                    method_contract.map_or(0, |contract| contract.argument_count),
                    method_contract.is_none_or(|contract| contract.return_behavior.returns_value()),
                )
                .with_may_return(method_contract.is_none_or(|contract| contract.may_return))
                .with_return_shape(method_contract.and_then(|contract| contract.return_shape))
                .with_argument_effects(
                    method_contract
                        .map(|contract| contract.argument_effects.clone())
                        .unwrap_or_default(),
                )
                .with_argument_field_writes(
                    method_contract
                        .map(|contract| contract.argument_field_writes.clone())
                        .unwrap_or_default(),
                )
            }
            CallTarget::MethodToken {
                hash_le,
                method,
                parameters_count,
                has_return_value,
                index,
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
        calls.insert(edge.call_offset, contract);
    }
    calls
}

#[cfg(test)]
mod tests;
