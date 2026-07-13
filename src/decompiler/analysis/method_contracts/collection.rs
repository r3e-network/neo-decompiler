use super::*;

#[derive(Debug)]
pub(super) struct MethodCollectionAnalysis {
    pub(super) trustworthy: bool,
    pub(super) analysis: SsaCollectionAnalysis,
}

pub(super) fn infer_argument_field_writes(
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

pub(super) fn infer_entry_and_static_collection_facts(
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

pub(super) fn intersect_static_writes(
    writes: &[StaticCollectionWrite],
) -> Option<CollectionShapeFacts> {
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

pub(super) fn aggregate_private_argument_facts(
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

pub(super) fn method_argument_effects(
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
