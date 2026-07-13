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

#[derive(Debug)]
struct MethodCollectionAnalysis {
    trustworthy: bool,
    analysis: SsaCollectionAnalysis,
}

fn infer_argument_field_writes(
    views_by_offset: &BTreeMap<usize, &MethodView>,
    call_graph: &CallGraph,
    contracts: &mut BTreeMap<usize, MethodContract>,
) {
    let iteration_limit = contracts.len().saturating_mul(2).saturating_add(4);
    let mut converged = false;
    for _ in 0..iteration_limit {
        let calls_by_offset = build_call_contracts(call_graph, contracts);
        let updates = views_by_offset
            .iter()
            .filter_map(|(offset, view)| {
                let contract = contracts.get(offset)?;
                let analysis = method_collection_analysis(
                    view,
                    &calls_by_offset,
                    contract,
                    &vec![CollectionShapeFacts::default(); contract.argument_count],
                    &BTreeMap::new(),
                );
                let writes = analysis
                    .filter(|analysis| analysis.trustworthy)
                    .map_or_else(
                        || vec![BTreeMap::new(); contract.argument_count],
                        |analysis| {
                            (0..contract.argument_count)
                                .map(|index| {
                                    if contract.argument_effects.get(index)
                                        == Some(&CollectionArgumentEffect::PreservesShape)
                                    {
                                        analysis
                                            .analysis
                                            .argument_field_writes
                                            .get(index)
                                            .cloned()
                                            .unwrap_or_default()
                                    } else {
                                        BTreeMap::new()
                                    }
                                })
                                .collect()
                        },
                    );
                Some((*offset, writes))
            })
            .collect::<Vec<_>>();
        if updates.iter().all(|(offset, writes)| {
            contracts
                .get(offset)
                .is_some_and(|contract| contract.argument_field_writes == *writes)
        }) {
            converged = true;
            break;
        }
        for (offset, writes) in updates {
            if let Some(contract) = contracts.get_mut(&offset) {
                contract.argument_field_writes = writes;
            }
        }
    }
    if !converged {
        for contract in contracts.values_mut() {
            contract.argument_field_writes = vec![BTreeMap::new(); contract.argument_count];
        }
    }
}

fn infer_entry_and_static_collection_facts(
    instructions: &[Instruction],
    manifest: Option<&ContractManifest>,
    call_graph: &CallGraph,
    views_by_offset: &BTreeMap<usize, &MethodView>,
    contracts: &mut BTreeMap<usize, MethodContract>,
) -> BTreeMap<usize, CollectionShapeFacts> {
    let abi_offsets = manifest
        .map(|manifest| {
            manifest
                .abi
                .methods
                .iter()
                .filter_map(|method| {
                    method
                        .offset
                        .and_then(|offset| usize::try_from(offset).ok())
                })
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let address_taken_offsets = instructions
        .iter()
        .filter_map(|instruction| {
            (instruction.opcode == OpCode::PushA)
                .then(|| match instruction.operand {
                    Some(Operand::I32(delta)) => {
                        instruction.offset.checked_add_signed(delta as isize)
                    }
                    _ => None,
                })
                .flatten()
        })
        .collect::<BTreeSet<_>>();
    let has_opaque_internal_call = call_graph.edges.iter().any(|edge| {
        matches!(
            edge.target,
            CallTarget::Indirect { .. } | CallTarget::UnresolvedInternal { .. }
        )
    });

    let iteration_limit = contracts.len().saturating_mul(4).saturating_add(8);
    let mut static_seed_facts = BTreeMap::new();
    let mut static_facts = BTreeMap::new();
    let mut converged = false;
    for _ in 0..iteration_limit {
        let calls_by_offset = build_call_contracts(call_graph, contracts);
        let effect_updates = views_by_offset
            .iter()
            .filter_map(|(offset, view)| {
                let contract = contracts.get(offset)?;
                Some((
                    *offset,
                    method_argument_effects(
                        view,
                        &calls_by_offset,
                        contract.argument_count,
                        &contract.argument_collection_facts,
                        &static_seed_facts,
                    ),
                ))
            })
            .collect::<Vec<_>>();
        let effects_changed = effect_updates.iter().any(|(offset, effects)| {
            contracts
                .get(offset)
                .is_some_and(|contract| contract.argument_effects != *effects)
        });
        for (offset, effects) in effect_updates {
            if let Some(contract) = contracts.get_mut(&offset) {
                contract.argument_effects = effects;
            }
        }
        let calls_by_offset = build_call_contracts(call_graph, contracts);
        let analyses = views_by_offset
            .iter()
            .filter_map(|(offset, view)| {
                let contract = contracts.get(offset)?;
                method_collection_analysis(
                    view,
                    &calls_by_offset,
                    contract,
                    &contract.argument_collection_facts,
                    &static_seed_facts,
                )
                .map(|analysis| (*offset, analysis))
            })
            .collect::<BTreeMap<_, _>>();

        let (new_static_seed_facts, new_static_facts) = if has_opaque_internal_call {
            (BTreeMap::new(), BTreeMap::new())
        } else {
            (
                aggregate_static_collection_facts(views_by_offset, &analyses, false),
                aggregate_static_collection_facts(views_by_offset, &analyses, true),
            )
        };
        let new_argument_facts = aggregate_private_argument_facts(
            call_graph,
            contracts,
            &analyses,
            &abi_offsets,
            &address_taken_offsets,
        );
        let arguments_unchanged = contracts.iter().all(|(offset, contract)| {
            new_argument_facts
                .get(offset)
                .is_some_and(|facts| *facts == contract.argument_collection_facts)
        });
        if new_static_seed_facts == static_seed_facts
            && new_static_facts == static_facts
            && arguments_unchanged
            && !effects_changed
        {
            static_facts = new_static_facts;
            converged = true;
            break;
        }
        static_seed_facts = new_static_seed_facts;
        static_facts = new_static_facts;
        for (offset, facts) in new_argument_facts {
            if let Some(contract) = contracts.get_mut(&offset) {
                contract.argument_collection_facts = facts;
            }
        }
    }

    if converged {
        static_facts
    } else {
        for contract in contracts.values_mut() {
            contract.argument_collection_facts =
                vec![CollectionShapeFacts::default(); contract.argument_count];
        }
        BTreeMap::new()
    }
}

fn method_collection_analysis(
    view: &MethodView,
    calls_by_offset: &BTreeMap<usize, CallContract>,
    contract: &MethodContract,
    argument_collection_facts: &[CollectionShapeFacts],
    static_collection_facts: &BTreeMap<usize, CollectionShapeFacts>,
) -> Option<MethodCollectionAnalysis> {
    if view.instructions.len() > crate::decompiler::high_level::MAX_HIGH_LEVEL_METHOD_INSTRUCTIONS {
        return None;
    }
    let context = MethodContext {
        argument_names: (0..contract.argument_count)
            .map(|index| format!("arg{index}"))
            .collect(),
        arguments_on_entry_stack: view
            .instructions
            .first()
            .is_none_or(|instruction| instruction.opcode != OpCode::Initslot),
        returns_value: Some(contract.return_behavior.returns_value()),
        calls_by_offset: calls_for_view(view, calls_by_offset),
        argument_collection_facts: argument_collection_facts.to_vec(),
        static_collection_facts: static_collection_facts.clone(),
    };
    let non_returning_calls = calls_by_offset
        .iter()
        .filter_map(|(offset, call)| {
            (!call.may_return && *offset >= view.method.offset && *offset < view.end)
                .then_some(*offset)
        })
        .collect::<BTreeSet<_>>();
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
    let built = SsaBuilder::new(cfg, &view.instructions)
        .with_method_context(&context)
        .build_with_report();
    Some(MethodCollectionAnalysis {
        trustworthy: built.fidelity.status != Fidelity::Incomplete,
        analysis: built.collection_analysis,
    })
}

fn aggregate_static_collection_facts(
    views_by_offset: &BTreeMap<usize, &MethodView>,
    analyses: &BTreeMap<usize, MethodCollectionAnalysis>,
    include_provisional: bool,
) -> BTreeMap<usize, CollectionShapeFacts> {
    let mut writes_by_index: BTreeMap<usize, Vec<StaticCollectionWrite>> = BTreeMap::new();
    for (offset, analysis) in analyses {
        if analysis.trustworthy {
            for write in &analysis.analysis.static_writes {
                if write.provisional && !include_provisional {
                    continue;
                }
                writes_by_index
                    .entry(write.index)
                    .or_default()
                    .push(write.clone());
            }
            continue;
        }
        for write in &analysis.analysis.static_writes {
            if write.provisional && !include_provisional {
                continue;
            }
            writes_by_index
                .entry(write.index)
                .or_default()
                .push(StaticCollectionWrite {
                    index: write.index,
                    facts: None,
                    is_null: write.is_null,
                    provisional: write.provisional,
                });
        }
        if let Some(view) = views_by_offset.get(offset) {
            for index in view.instructions.iter().filter_map(static_load_index) {
                writes_by_index
                    .entry(index)
                    .or_default()
                    .push(StaticCollectionWrite {
                        index,
                        facts: None,
                        is_null: false,
                        provisional: false,
                    });
            }
        }
    }
    for (offset, view) in views_by_offset {
        if analyses.contains_key(offset) {
            continue;
        }
        for index in view.instructions.iter().filter_map(|instruction| {
            static_load_index(instruction).or_else(|| static_store_index(instruction))
        }) {
            writes_by_index
                .entry(index)
                .or_default()
                .push(StaticCollectionWrite {
                    index,
                    facts: None,
                    is_null: false,
                    provisional: false,
                });
        }
    }

    writes_by_index
        .into_iter()
        .filter_map(|(index, writes)| intersect_static_writes(&writes).map(|facts| (index, facts)))
        .collect()
}

fn intersect_static_writes(writes: &[StaticCollectionWrite]) -> Option<CollectionShapeFacts> {
    let mut non_null = writes.iter().filter(|write| !write.is_null);
    let mut facts = non_null.next()?.facts.clone()?;
    for write in non_null {
        let next = write.facts.as_ref()?;
        if facts.shape != next.shape {
            facts.shape = None;
        }
        facts.indexed.retain(|index, shape| {
            next.indexed
                .get(index)
                .is_some_and(|next_shape| next_shape == shape)
        });
    }
    (!facts.is_empty()).then_some(facts)
}

fn aggregate_private_argument_facts(
    call_graph: &CallGraph,
    contracts: &BTreeMap<usize, MethodContract>,
    analyses: &BTreeMap<usize, MethodCollectionAnalysis>,
    abi_offsets: &BTreeSet<usize>,
    address_taken_offsets: &BTreeSet<usize>,
) -> BTreeMap<usize, Vec<CollectionShapeFacts>> {
    contracts
        .iter()
        .map(|(offset, contract)| {
            let empty = vec![CollectionShapeFacts::default(); contract.argument_count];
            if abi_offsets.contains(offset) || address_taken_offsets.contains(offset) {
                return (*offset, empty);
            }
            let incoming = call_graph
                .edges
                .iter()
                .filter(|edge| {
                    matches!(
                        &edge.target,
                        CallTarget::Internal { method } if method.offset == *offset
                    )
                })
                .collect::<Vec<_>>();
            if incoming.is_empty()
                || incoming
                    .iter()
                    .any(|edge| !matches!(edge.opcode.as_str(), "CALL" | "CALL_L"))
            {
                return (*offset, empty);
            }
            let facts = (0..contract.argument_count)
                .map(|argument_index| {
                    let mut agreed: Option<CollectionShapeFacts> = None;
                    for edge in &incoming {
                        let Some(analysis) = analyses
                            .get(&edge.caller.offset)
                            .filter(|analysis| analysis.trustworthy)
                        else {
                            return CollectionShapeFacts::default();
                        };
                        let Some(facts) = analysis
                            .analysis
                            .call_argument_facts
                            .get(&edge.call_offset)
                            .and_then(|facts| facts.get(argument_index))
                            .filter(|facts| !facts.is_empty())
                        else {
                            return CollectionShapeFacts::default();
                        };
                        if agreed.as_ref().is_some_and(|agreed| agreed != facts) {
                            return CollectionShapeFacts::default();
                        }
                        agreed = Some(facts.clone());
                    }
                    agreed.unwrap_or_default()
                })
                .collect();
            (*offset, facts)
        })
        .collect()
}

fn static_load_index(instruction: &Instruction) -> Option<usize> {
    match instruction.opcode {
        OpCode::Ldsfld0 => Some(0),
        OpCode::Ldsfld1 => Some(1),
        OpCode::Ldsfld2 => Some(2),
        OpCode::Ldsfld3 => Some(3),
        OpCode::Ldsfld4 => Some(4),
        OpCode::Ldsfld5 => Some(5),
        OpCode::Ldsfld6 => Some(6),
        OpCode::Ldsfld => match instruction.operand {
            Some(Operand::U8(index)) => Some(usize::from(index)),
            _ => None,
        },
        _ => None,
    }
}

fn static_store_index(instruction: &Instruction) -> Option<usize> {
    match instruction.opcode {
        OpCode::Stsfld0 => Some(0),
        OpCode::Stsfld1 => Some(1),
        OpCode::Stsfld2 => Some(2),
        OpCode::Stsfld3 => Some(3),
        OpCode::Stsfld4 => Some(4),
        OpCode::Stsfld5 => Some(5),
        OpCode::Stsfld6 => Some(6),
        OpCode::Stsfld => match instruction.operand {
            Some(Operand::U8(index)) => Some(usize::from(index)),
            _ => None,
        },
        _ => None,
    }
}

fn method_argument_effects(
    view: &MethodView,
    calls_by_offset: &BTreeMap<usize, CallContract>,
    argument_count: usize,
    argument_collection_facts: &[CollectionShapeFacts],
    static_collection_facts: &BTreeMap<usize, CollectionShapeFacts>,
) -> Vec<CollectionArgumentEffect> {
    let unknown = vec![CollectionArgumentEffect::Unknown; argument_count];
    if argument_count == 0
        || view.instructions.len()
            > crate::decompiler::high_level::MAX_HIGH_LEVEL_METHOD_INSTRUCTIONS
    {
        return unknown;
    }
    let has_internal_or_indirect_call =
        view.instructions
            .iter()
            .any(|instruction| match instruction.opcode {
                OpCode::Call | OpCode::Call_L | OpCode::CallA => true,
                OpCode::CallT => {
                    !calls_by_offset
                        .get(&instruction.offset)
                        .is_some_and(|contract| {
                            matches!(contract.target, SemanticCallTarget::MethodToken { .. })
                        })
                }
                _ => false,
            });
    let may_resize_collection = view.instructions.iter().any(|instruction| {
        matches!(
            instruction.opcode,
            OpCode::Append | OpCode::Remove | OpCode::Clearitems | OpCode::Popitem
        )
    });
    if has_internal_or_indirect_call || may_resize_collection {
        return unknown;
    }

    let context = MethodContext {
        argument_names: (0..argument_count)
            .map(|index| format!("arg{index}"))
            .collect(),
        arguments_on_entry_stack: view
            .instructions
            .first()
            .is_none_or(|instruction| instruction.opcode != OpCode::Initslot),
        calls_by_offset: calls_for_view(view, calls_by_offset),
        argument_collection_facts: argument_collection_facts.to_vec(),
        static_collection_facts: static_collection_facts.clone(),
        ..MethodContext::default()
    };
    let built = SsaBuilder::new(&view.cfg, &view.instructions)
        .with_method_context(&context)
        .build_with_report();
    if built.fidelity.status == Fidelity::Incomplete {
        return unknown;
    }

    let mut origins = (0..argument_count)
        .map(|index| (SsaVariable::initial(format!("arg{index}")), index))
        .collect::<BTreeMap<_, _>>();
    loop {
        let mut changed = false;
        for (_, block) in built.ssa.blocks_iter() {
            for phi in &block.phi_nodes {
                let mut operand_origins = phi
                    .operands
                    .values()
                    .filter_map(|operand| origins.get(operand).copied());
                let Some(first) = operand_origins.next() else {
                    continue;
                };
                if phi
                    .operands
                    .values()
                    .all(|operand| origins.get(operand) == Some(&first))
                {
                    changed |= origins.insert(phi.target.clone(), first).is_none();
                }
            }
            for statement in &block.stmts {
                let SsaStmt::Assign {
                    target,
                    value: SsaExpr::Variable(source),
                } = statement
                else {
                    continue;
                };
                if let Some(origin) = origins.get(source).copied() {
                    changed |= origins.insert(target.clone(), origin).is_none();
                }
            }
        }
        if !changed {
            break;
        }
    }

    let mut unsafe_arguments = BTreeSet::new();
    let mut shape_preserving_receivers = BTreeSet::new();
    for (_, block) in built.ssa.blocks_iter() {
        for phi in &block.phi_nodes {
            if !origins.contains_key(&phi.target) {
                for operand in phi.operands.values() {
                    if let Some(origin) = origins.get(operand) {
                        unsafe_arguments.insert(*origin);
                    }
                }
            }
        }
        for statement in &block.stmts {
            match statement {
                SsaStmt::Assign {
                    target,
                    value: SsaExpr::Variable(source),
                } => {
                    if target.base.starts_with("static") {
                        if let Some(origin) = origins.get(source) {
                            unsafe_arguments.insert(*origin);
                        }
                    }
                }
                SsaStmt::Assign { value, .. } => {
                    collect_escaping_argument_origins(value, &origins, &mut unsafe_arguments);
                }
                SsaStmt::Expr(SsaExpr::Call {
                    target:
                        SemanticCallTarget::Intrinsic(Intrinsic::Opcode(
                            opcode @ (OpCode::Setitem | OpCode::Reverseitems | OpCode::Memcpy),
                        )),
                    args,
                }) => {
                    let receiver_origin = args.first().and_then(|receiver| match receiver {
                        SsaExpr::Variable(variable) => origins.get(variable).copied(),
                        _ => None,
                    });
                    if let Some(origin) = receiver_origin {
                        shape_preserving_receivers.insert(origin);
                    } else if let Some(receiver) = args.first() {
                        collect_argument_origins(receiver, &origins, &mut unsafe_arguments);
                    }
                    if *opcode == OpCode::Setitem {
                        if let Some(value) = args.get(2) {
                            collect_argument_origins(value, &origins, &mut unsafe_arguments);
                        }
                    }
                }
                SsaStmt::Expr(expression)
                | SsaStmt::Return(Some(expression))
                | SsaStmt::Throw(Some(expression))
                | SsaStmt::Abort(Some(expression)) => {
                    collect_argument_origins(expression, &origins, &mut unsafe_arguments);
                }
                SsaStmt::Assert { .. } => {}
                SsaStmt::Return(None) | SsaStmt::Throw(None) | SsaStmt::Abort(None) => {}
                SsaStmt::Phi(_) | SsaStmt::Other(_) => return unknown,
            }
        }
    }

    (0..argument_count)
        .map(|index| {
            if shape_preserving_receivers.contains(&index) && !unsafe_arguments.contains(&index) {
                CollectionArgumentEffect::PreservesShape
            } else if !unsafe_arguments.contains(&index) {
                CollectionArgumentEffect::ReadOnly
            } else {
                CollectionArgumentEffect::Unknown
            }
        })
        .collect()
}

fn collect_escaping_argument_origins(
    expression: &SsaExpr,
    origins: &BTreeMap<SsaVariable, usize>,
    found: &mut BTreeSet<usize>,
) {
    match expression {
        SsaExpr::Call {
            target: SemanticCallTarget::Intrinsic(_),
            ..
        } => {}
        SsaExpr::Call { args, .. } | SsaExpr::Array(args) | SsaExpr::Struct(args) => {
            for argument in args {
                collect_argument_origins(argument, origins, found);
            }
        }
        SsaExpr::Map(entries) => {
            for (key, value) in entries {
                collect_argument_origins(key, origins, found);
                collect_argument_origins(value, origins, found);
            }
        }
        SsaExpr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_argument_origins(condition, origins, found);
            collect_argument_origins(then_expr, origins, found);
            collect_argument_origins(else_expr, origins, found);
        }
        SsaExpr::Literal(_)
        | SsaExpr::Variable(_)
        | SsaExpr::Binary { .. }
        | SsaExpr::Unary { .. }
        | SsaExpr::Index { .. }
        | SsaExpr::Member { .. }
        | SsaExpr::Cast { .. }
        | SsaExpr::Convert { .. }
        | SsaExpr::IsType { .. }
        | SsaExpr::NewArray { .. } => {}
    }
}

fn collect_argument_origins(
    expression: &SsaExpr,
    origins: &BTreeMap<SsaVariable, usize>,
    found: &mut BTreeSet<usize>,
) {
    match expression {
        SsaExpr::Variable(variable) => {
            if let Some(origin) = origins.get(variable) {
                found.insert(*origin);
            }
        }
        SsaExpr::Literal(_) => {}
        SsaExpr::Binary { left, right, .. } => {
            collect_argument_origins(left, origins, found);
            collect_argument_origins(right, origins, found);
        }
        SsaExpr::Unary { operand, .. }
        | SsaExpr::Member { base: operand, .. }
        | SsaExpr::Cast { expr: operand, .. }
        | SsaExpr::Convert { value: operand, .. }
        | SsaExpr::IsType { value: operand, .. }
        | SsaExpr::NewArray {
            length: operand, ..
        } => collect_argument_origins(operand, origins, found),
        SsaExpr::Call { args, .. } | SsaExpr::Array(args) | SsaExpr::Struct(args) => {
            for argument in args {
                collect_argument_origins(argument, origins, found);
            }
        }
        SsaExpr::Index { base, index } => {
            collect_argument_origins(base, origins, found);
            collect_argument_origins(index, origins, found);
        }
        SsaExpr::Map(entries) => {
            for (key, value) in entries {
                collect_argument_origins(key, origins, found);
                collect_argument_origins(value, origins, found);
            }
        }
        SsaExpr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_argument_origins(condition, origins, found);
            collect_argument_origins(then_expr, origins, found);
            collect_argument_origins(else_expr, origins, found);
        }
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
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use crate::decompiler::analysis::call_graph::{
        build_call_graph, CallEdge, CallGraph, CallTarget,
    };
    use crate::decompiler::analysis::MethodRef;
    use crate::disassembler::Disassembler;
    use crate::manifest::ContractManifest;
    use crate::nef::{MethodToken, NefFile, NefHeader};

    use super::{
        aggregate_private_argument_facts, infer_method_contracts, intersect_static_writes,
        CollectionArgumentEffect, CollectionShape, CollectionShapeFacts, MethodCollectionAnalysis,
        MethodContract, MethodContracts, ReturnBehavior, SsaCollectionAnalysis,
        StaticCollectionWrite,
    };

    const PRIVATE_VOID_LEAF: &[u8] = &[
        0x19, 0x11, 0x34, 0x05, 0x40, 0x21, 0x21, 0x57, 0x00, 0x01, 0x78, 0x45, 0x40,
    ];

    fn manifest(json: &str) -> ContractManifest {
        ContractManifest::from_json_str(json).expect("manifest parses")
    }

    fn analyze(script: &[u8], manifest: Option<&ContractManifest>) -> MethodContracts {
        analyze_with_tokens(script, manifest, Vec::new())
    }

    fn analyze_with_tokens(
        script: &[u8],
        manifest: Option<&ContractManifest>,
        method_tokens: Vec<MethodToken>,
    ) -> MethodContracts {
        let instructions = Disassembler::new()
            .disassemble(script)
            .expect("script disassembles");
        let nef = NefFile {
            header: NefHeader {
                magic: *b"NEF3",
                compiler: "test".to_string(),
                source: String::new(),
            },
            method_tokens,
            script: script.to_vec(),
            checksum: 0,
        };
        let call_graph = build_call_graph(&nef, &instructions, manifest);
        infer_method_contracts(&instructions, manifest, &call_graph)
    }

    fn standard_manifest() -> ContractManifest {
        manifest(
            r#"{
                "name": "Contract",
                "abi": { "methods": [{
                    "name": "main", "parameters": [], "returntype": "Integer", "offset": 0
                }] }
            }"#,
        )
    }

    #[test]
    fn infers_private_void_leaf_with_entry_arity() {
        let manifest = standard_manifest();

        let contracts = analyze(PRIVATE_VOID_LEAF, Some(&manifest));
        let helper = contracts.get(7).expect("private helper contract");

        assert_eq!(helper.argument_count, 1);
        assert_eq!(helper.return_behavior, ReturnBehavior::Void);
    }

    #[test]
    fn infers_fixed_struct_shape_from_all_reachable_returns() {
        let manifest = manifest(
            r#"{
                "name": "StructReturn",
                "abi": { "methods": [
                    {"name":"main","parameters":[],"returntype":"Array","offset":0},
                    {"name":"pair","parameters":[],"returntype":"Array","offset":4}
                ] }
            }"#,
        );
        let script = [0x34, 0x04, 0x40, 0x21, 0x11, 0x12, 0x12, 0xBF, 0x40];

        let contracts = analyze(&script, Some(&manifest));

        assert_eq!(
            contracts.get(4).expect("pair contract").return_shape,
            Some(CollectionShape::Struct(2))
        );
    }

    #[test]
    fn infers_nested_private_entry_facts_through_static_constructor_chain() {
        let manifest = manifest(
            r#"{
                "name": "NestedStatic",
                "abi": { "methods": [
                    {"name":"main","parameters":[],"returntype":"Void","offset":0}
                ] }
            }"#,
        );
        let script = [
            0x0B, 0x0B, 0x12, 0xC0, 0x4A, 0x34, 0x07, 0x60, 0x58, 0x34, 0x0E, 0x40, 0x57, 0x00,
            0x01, 0x78, 0x10, 0x11, 0x11, 0x12, 0xC0, 0xD0, 0x40, 0x57, 0x00, 0x01, 0x78, 0x10,
            0xCE, 0xC1, 0x45, 0x45, 0x45, 0x40,
        ];

        let contracts = analyze(&script, Some(&manifest));

        assert_eq!(
            contracts.static_collection_facts.get(&0),
            Some(&CollectionShapeFacts {
                shape: Some(CollectionShape::Array(2)),
                indexed: BTreeMap::from([(0, CollectionShape::Array(2))]),
            })
        );
        let constructor = contracts.get(12).expect("constructor contract");
        assert_eq!(
            constructor.argument_field_writes,
            vec![BTreeMap::from([(0, CollectionShape::Array(2))])]
        );
        let consumer = contracts.get(23).expect("consumer contract");
        assert_eq!(
            consumer.argument_effects,
            vec![CollectionArgumentEffect::ReadOnly]
        );
        assert_eq!(
            consumer.argument_collection_facts,
            vec![CollectionShapeFacts {
                shape: Some(CollectionShape::Array(2)),
                indexed: BTreeMap::from([(0, CollectionShape::Array(2))]),
            }]
        );
    }

    #[test]
    fn distinguishes_shape_preserving_and_resizing_argument_effects() {
        let manifest = standard_manifest();
        let shape_preserving = [
            0x34, 0x04, 0x40, 0x21, 0x57, 0x00, 0x03, 0x79, 0x78, 0x10, 0x51, 0xD0, 0x7A, 0x78,
            0x11, 0x51, 0xD0, 0x40,
        ];
        let resizing = [
            0x34, 0x04, 0x40, 0x21, 0x57, 0x00, 0x01, 0x78, 0x11, 0xCF, 0x40,
        ];

        let preserving_contracts = analyze(&shape_preserving, Some(&manifest));
        let resizing_contracts = analyze(&resizing, Some(&manifest));

        assert_eq!(
            preserving_contracts
                .get(4)
                .expect("SETITEM helper contract")
                .argument_effects,
            vec![
                CollectionArgumentEffect::PreservesShape,
                CollectionArgumentEffect::Unknown,
                CollectionArgumentEffect::Unknown,
            ]
        );
        assert_eq!(
            resizing_contracts
                .get(4)
                .expect("APPEND helper contract")
                .argument_effects,
            vec![CollectionArgumentEffect::Unknown]
        );
    }

    #[test]
    fn returned_argument_alias_does_not_preserve_collection_shape() {
        let manifest = standard_manifest();
        let identity = [0x34, 0x04, 0x40, 0x21, 0x57, 0x00, 0x01, 0x78, 0x40];

        let contracts = analyze(&identity, Some(&manifest));

        assert_eq!(
            contracts
                .get(4)
                .expect("identity helper contract")
                .argument_effects,
            vec![CollectionArgumentEffect::Unknown]
        );
    }

    #[test]
    fn known_zero_argument_syscall_does_not_hide_shape_preserving_receiver() {
        let manifest = standard_manifest();
        let script = [
            0x34, 0x04, 0x40, 0x21, 0x57, 0x00, 0x01, 0x41, 0x9B, 0xF6, 0x67, 0xCE, 0x45, 0x78,
            0x10, 0x11, 0x11, 0x12, 0xC0, 0xD0, 0x40,
        ];

        let contracts = analyze(&script, Some(&manifest));

        assert_eq!(
            contracts
                .get(4)
                .expect("syscall constructor contract")
                .argument_effects,
            vec![CollectionArgumentEffect::PreservesShape]
        );
    }

    #[test]
    fn static_and_nested_argument_aliases_remain_unknown() {
        let manifest = standard_manifest();
        let static_escape = [0x34, 0x04, 0x40, 0x21, 0x57, 0x00, 0x01, 0x78, 0x60, 0x40];
        let nested_alias = [
            0x34, 0x04, 0x40, 0x21, 0x57, 0x00, 0x01, 0x78, 0x11, 0xC0, 0x45, 0x40,
        ];

        for script in [&static_escape[..], &nested_alias[..]] {
            assert_eq!(
                analyze(script, Some(&manifest))
                    .get(4)
                    .expect("escaping helper contract")
                    .argument_effects,
                vec![CollectionArgumentEffect::Unknown]
            );
        }
    }

    #[test]
    fn static_fact_intersection_rejects_unknown_and_conflicting_writes() {
        let array_two = CollectionShapeFacts {
            shape: Some(CollectionShape::Array(2)),
            indexed: BTreeMap::new(),
        };
        let known = StaticCollectionWrite {
            index: 0,
            facts: Some(array_two.clone()),
            is_null: false,
            provisional: false,
        };
        let null = StaticCollectionWrite {
            index: 0,
            facts: None,
            is_null: true,
            provisional: false,
        };
        assert_eq!(
            intersect_static_writes(&[null, known.clone()]),
            Some(array_two)
        );
        assert_eq!(
            intersect_static_writes(&[
                known.clone(),
                StaticCollectionWrite {
                    index: 0,
                    facts: Some(CollectionShapeFacts {
                        shape: Some(CollectionShape::Array(3)),
                        indexed: BTreeMap::new(),
                    }),
                    is_null: false,
                    provisional: false,
                },
            ]),
            None
        );
        assert_eq!(
            intersect_static_writes(&[
                known,
                StaticCollectionWrite {
                    index: 0,
                    facts: None,
                    is_null: false,
                    provisional: false,
                },
            ]),
            None
        );
    }

    #[test]
    fn private_entry_facts_require_every_direct_incoming_call_and_exclude_public_entries() {
        let target = MethodRef {
            offset: 20,
            name: "target".to_string(),
        };
        let graph = CallGraph {
            methods: vec![target.clone()],
            edges: vec![
                CallEdge {
                    caller: MethodRef {
                        offset: 0,
                        name: "left".to_string(),
                    },
                    call_offset: 5,
                    opcode: "CALL".to_string(),
                    target: CallTarget::Internal {
                        method: target.clone(),
                    },
                },
                CallEdge {
                    caller: MethodRef {
                        offset: 10,
                        name: "right".to_string(),
                    },
                    call_offset: 15,
                    opcode: "CALL_L".to_string(),
                    target: CallTarget::Internal { method: target },
                },
            ],
        };
        let fact = CollectionShapeFacts {
            shape: Some(CollectionShape::Array(2)),
            indexed: BTreeMap::from([(0, CollectionShape::Struct(2))]),
        };
        let mut target_contract = contract(20, ReturnBehavior::Void);
        target_contract.argument_count = 1;
        target_contract.argument_collection_facts = vec![CollectionShapeFacts::default()];
        target_contract.argument_field_writes = vec![BTreeMap::new()];
        target_contract.argument_effects = vec![CollectionArgumentEffect::Unknown];
        let contracts = BTreeMap::from([(20, target_contract)]);
        let analysis = |call_offset| MethodCollectionAnalysis {
            trustworthy: true,
            analysis: SsaCollectionAnalysis {
                call_argument_facts: BTreeMap::from([(call_offset, vec![fact.clone()])]),
                ..SsaCollectionAnalysis::default()
            },
        };
        let mut analyses = BTreeMap::from([(0, analysis(5)), (10, analysis(15))]);

        assert_eq!(
            aggregate_private_argument_facts(
                &graph,
                &contracts,
                &analyses,
                &BTreeSet::new(),
                &BTreeSet::new(),
            )[&20],
            vec![fact.clone()]
        );

        analyses
            .get_mut(&10)
            .expect("right analysis")
            .analysis
            .call_argument_facts
            .insert(15, vec![CollectionShapeFacts::default()]);
        assert_eq!(
            aggregate_private_argument_facts(
                &graph,
                &contracts,
                &analyses,
                &BTreeSet::new(),
                &BTreeSet::new(),
            )[&20],
            vec![CollectionShapeFacts::default()]
        );

        let excluded = BTreeSet::from([20]);
        assert_eq!(
            aggregate_private_argument_facts(
                &graph,
                &contracts,
                &BTreeMap::from([(0, analysis(5)), (10, analysis(15))]),
                &excluded,
                &BTreeSet::new(),
            )[&20],
            vec![CollectionShapeFacts::default()]
        );
        assert_eq!(
            aggregate_private_argument_facts(
                &graph,
                &contracts,
                &BTreeMap::from([(0, analysis(5)), (10, analysis(15))]),
                &BTreeSet::new(),
                &excluded,
            )[&20],
            vec![CollectionShapeFacts::default()]
        );
    }

    #[test]
    fn infers_five_entry_arguments_for_private_memcpy_helper() {
        let manifest = standard_manifest();
        let script = [0x34, 0x04, 0x40, 0x21, 0x89, 0x40];

        let contracts = analyze(&script, Some(&manifest));
        let helper = contracts.get(4).expect("private MEMCPY helper contract");

        assert_eq!(helper.argument_count, 5);
        assert_eq!(helper.return_behavior, ReturnBehavior::Void);
    }

    #[test]
    fn converges_private_void_wrapper_chain_from_leaf_to_caller() {
        let manifest = standard_manifest();
        let script = [
            0x19, 0x34, 0x05, 0x40, 0x21, 0x21, 0x34, 0x04, 0x40, 0x21, 0x40,
        ];

        let contracts = analyze(&script, Some(&manifest));

        assert_eq!(
            contracts.get(6).map(|contract| contract.return_behavior),
            Some(ReturnBehavior::Void)
        );
        assert_eq!(
            contracts.get(10).map(|contract| contract.return_behavior),
            Some(ReturnBehavior::Void)
        );
    }

    #[test]
    fn keeps_recursive_private_method_unknown() {
        let manifest = standard_manifest();
        let script = [0x19, 0x34, 0x05, 0x40, 0x21, 0x21, 0x34, 0x00, 0x40];

        let contracts = analyze(&script, Some(&manifest));

        assert_eq!(
            contracts.get(6).map(|contract| contract.return_behavior),
            Some(ReturnBehavior::Unknown)
        );
    }

    #[test]
    fn keeps_mixed_return_private_method_unknown() {
        let manifest = standard_manifest();
        let script = [
            0x34, 0x06, 0x40, 0x21, 0x21, 0x21, 0x11, 0x26, 0x04, 0x11, 0x40, 0x40,
        ];

        let contracts = analyze(&script, Some(&manifest));

        assert_eq!(
            contracts.get(6).map(|contract| contract.return_behavior),
            Some(ReturnBehavior::Unknown)
        );
    }

    #[test]
    fn keeps_private_method_without_return_unknown() {
        let manifest = standard_manifest();
        let script = [0x34, 0x04, 0x40, 0x21, 0x38];

        let contracts = analyze(&script, Some(&manifest));

        assert_eq!(
            contracts.get(4).map(|contract| contract.return_behavior),
            Some(ReturnBehavior::Unknown)
        );
    }

    #[test]
    fn method_token_contract_drives_private_void_inference() {
        let manifest = standard_manifest();
        let script = [0x34, 0x06, 0x40, 0x21, 0x21, 0x21, 0x37, 0x00, 0x00, 0x40];
        let token = MethodToken {
            hash: [0; 20],
            method: "notify".to_string(),
            parameters_count: 0,
            has_return_value: false,
            call_flags: 0,
        };

        let contracts = analyze_with_tokens(&script, Some(&manifest), vec![token]);

        assert_eq!(
            contracts.get(6).map(|contract| contract.return_behavior),
            Some(ReturnBehavior::Void)
        );
    }

    #[test]
    fn manifest_declaration_overrides_private_return_inference_and_arity() {
        let manifest = manifest(
            r#"{
                "name": "DeclaredHelper",
                "abi": { "methods": [
                    { "name": "main", "parameters": [], "returntype": "Integer", "offset": 0 },
                    {
                        "name": "helper",
                        "parameters": [{ "name": "value", "type": "Integer" }],
                        "returntype": "Integer",
                        "offset": 4
                    }
                ] }
            }"#,
        );
        let script = [0x34, 0x04, 0x40, 0x21, 0x40];

        let contracts = analyze(&script, Some(&manifest));
        let helper = contracts.get(4).expect("declared helper contract");

        assert_eq!(helper.argument_count, 1);
        assert_eq!(helper.return_behavior, ReturnBehavior::Value);
    }

    #[test]
    fn manifest_void_declaration_overrides_value_left_on_stack() {
        let manifest = manifest(
            r#"{
                "name": "DeclaredVoidHelper",
                "abi": { "methods": [
                    { "name": "main", "parameters": [], "returntype": "Void", "offset": 0 },
                    {
                        "name": "helper",
                        "parameters": [],
                        "returntype": "Void",
                        "offset": 4
                    }
                ] }
            }"#,
        );
        let script = [0x34, 0x04, 0x40, 0x21, 0x11, 0x40];

        let contracts = analyze(&script, Some(&manifest));

        assert_eq!(
            contracts.get(4).map(|contract| contract.return_behavior),
            Some(ReturnBehavior::Void)
        );
    }

    #[test]
    fn offsetless_manifest_entry_uses_declared_contract() {
        let manifest = manifest(
            r#"{
                "name": "OffsetlessEntry",
                "abi": { "methods": [{
                    "name": "main",
                    "parameters": [
                        { "name": "left", "type": "Integer" },
                        { "name": "right", "type": "Integer" }
                    ],
                    "returntype": "Integer"
                }] }
            }"#,
        );

        let contracts = analyze(&[0x40], Some(&manifest));
        let entry = contracts.get(0).expect("entry contract");

        assert_eq!(entry.method.name, "main");
        assert_eq!(entry.argument_count, 2);
        assert_eq!(entry.return_behavior, ReturnBehavior::Value);
    }

    #[test]
    fn sorts_and_deduplicates_call_graph_methods_by_offset() {
        let manifest = standard_manifest();
        let instructions = Disassembler::new()
            .disassemble(PRIVATE_VOID_LEAF)
            .expect("script disassembles");
        let nef = NefFile {
            header: NefHeader {
                magic: *b"NEF3",
                compiler: "test".to_string(),
                source: String::new(),
            },
            method_tokens: Vec::new(),
            script: PRIVATE_VOID_LEAF.to_vec(),
            checksum: 0,
        };
        let mut call_graph = build_call_graph(&nef, &instructions, Some(&manifest));
        let duplicate = call_graph.methods[1].clone();
        call_graph.methods.reverse();
        call_graph.methods.push(duplicate);

        let contracts = infer_method_contracts(&instructions, Some(&manifest), &call_graph);
        let offsets: Vec<_> = contracts
            .methods
            .iter()
            .map(|contract| contract.method.offset)
            .collect();

        assert_eq!(offsets, vec![0, 7]);
    }

    #[test]
    fn serializes_return_behaviors_as_lowercase_strings() {
        let contracts = MethodContracts {
            methods: vec![
                contract(0, ReturnBehavior::Value),
                contract(1, ReturnBehavior::Void),
                contract(2, ReturnBehavior::Unknown),
            ],
            static_collection_facts: BTreeMap::new(),
        };

        let value = serde_json::to_value(contracts).expect("contracts serialize");
        let behaviors: Vec<_> = value["methods"]
            .as_array()
            .expect("methods array")
            .iter()
            .map(|method| method["return_behavior"].as_str().expect("behavior"))
            .collect();

        assert_eq!(behaviors, vec!["value", "void", "unknown"]);
        assert!(value["methods"]
            .as_array()
            .expect("methods array")
            .iter()
            .all(|method| method["may_return"] == true));
    }

    #[test]
    fn infers_non_returning_effect_through_manifest_wrapper() {
        let manifest = manifest(
            r#"{
                "name": "AbortWrapper",
                "abi": { "methods": [
                    {"name":"main","parameters":[],"returntype":"Integer","offset":0},
                    {"name":"abortLeaf","parameters":[],"returntype":"Integer","offset":4}
                ] }
            }"#,
        );
        let contracts = analyze(&[0x34, 0x04, 0x40, 0x21, 0x38], Some(&manifest));

        let main = contracts.get(0).expect("main contract");
        let leaf = contracts.get(4).expect("leaf contract");
        assert_eq!(main.return_behavior, ReturnBehavior::Value);
        assert_eq!(leaf.return_behavior, ReturnBehavior::Value);
        assert!(!main.may_return);
        assert!(!leaf.may_return);
    }

    #[test]
    fn keeps_may_return_when_any_reachable_path_returns() {
        let manifest = manifest(
            r#"{
                "name": "ConditionalAbort",
                "abi": { "methods": [
                    {"name":"main","parameters":[],"returntype":"Integer","offset":0},
                    {"name":"abortLeaf","parameters":[],"returntype":"Integer","offset":8}
                ] }
            }"#,
        );
        let script = [0x11, 0x24, 0x04, 0x11, 0x40, 0x34, 0x03, 0x40, 0x38];
        let contracts = analyze(&script, Some(&manifest));

        assert!(contracts.get(0).expect("main contract").may_return);
        assert!(!contracts.get(8).expect("leaf contract").may_return);
    }

    #[test]
    fn get_returns_contract_at_requested_offset() {
        let contracts = MethodContracts {
            methods: vec![contract(2, ReturnBehavior::Unknown)],
            static_collection_facts: BTreeMap::new(),
        };

        assert_eq!(contracts.get(2), contracts.methods.first());
        assert_eq!(contracts.get(3), None);
    }

    #[test]
    fn map_projections_include_all_contracts_and_treat_unknown_as_value() {
        let contracts = MethodContracts {
            methods: vec![
                MethodContract {
                    argument_count: 3,
                    ..contract(0, ReturnBehavior::Value)
                },
                MethodContract {
                    argument_count: 2,
                    ..contract(1, ReturnBehavior::Void)
                },
                MethodContract {
                    argument_count: 1,
                    ..contract(2, ReturnBehavior::Unknown)
                },
            ],
            static_collection_facts: BTreeMap::new(),
        };

        assert_eq!(
            contracts.argument_counts_by_offset(),
            BTreeMap::from([(0, 3), (1, 2), (2, 1)])
        );
        assert_eq!(
            contracts.returns_value_by_offset(),
            BTreeMap::from([(0, true), (1, false), (2, true)])
        );
    }

    fn contract(offset: usize, return_behavior: ReturnBehavior) -> MethodContract {
        MethodContract {
            method: MethodRef {
                offset,
                name: format!("method_{offset}"),
            },
            argument_count: 0,
            return_behavior,
            may_return: true,
            return_shape: None,
            argument_effects: vec![CollectionArgumentEffect::Unknown; 0],
            argument_collection_facts: Vec::new(),
            argument_field_writes: Vec::new(),
        }
    }
}
