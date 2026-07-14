use super::*;
pub(crate) fn method_argument_effects(
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
        // CONVERT and CAST can preserve the underlying VM object. Treat their
        // operand as escaping so a later alias cannot retain a stale shape.
        SsaExpr::Convert { value, .. } | SsaExpr::Cast { expr: value, .. } => {
            collect_argument_origins(value, origins, found);
        }
        SsaExpr::Literal(_)
        | SsaExpr::Variable(_)
        | SsaExpr::Binary { .. }
        | SsaExpr::Unary { .. }
        | SsaExpr::Index { .. }
        | SsaExpr::Member { .. }
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
