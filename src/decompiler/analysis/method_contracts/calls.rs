//! Call-contract construction and return-shape analysis.

use super::*;

pub(super) fn method_return_facts(
    view: &MethodView,
    calls_by_offset: &BTreeMap<usize, CallContract>,
    argument_count: usize,
) -> Option<CollectionShapeFacts> {
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
        .return_facts
}

pub(super) fn method_may_return(
    view: &MethodView,
    calls_by_offset: &BTreeMap<usize, CallContract>,
) -> bool {
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

pub(super) fn method_has_only_bare_returns(
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

pub(super) fn calls_for_view(
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

pub(super) fn build_call_contracts(
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
                .with_return_facts(
                    method_contract.and_then(|contract| contract.return_collection_facts.clone()),
                )
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
