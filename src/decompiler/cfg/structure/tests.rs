use super::*;
use crate::decompiler::cfg::ssa::DominanceInfo;
use crate::decompiler::cfg::ssa::SsaForm;
use crate::decompiler::cfg::ssa::{PhiNode, SsaBlock, SsaExpr, SsaStmt, SsaVariable};
use crate::decompiler::cfg::{BasicBlock, BlockId, Cfg, EdgeKind, Terminator};
use crate::decompiler::ir::{BinOp, Literal, Stmt};

fn v(base: &str, n: usize) -> SsaVariable {
    SsaVariable::new(base.to_string(), n)
}

/// Build a diamond: BB0 branches to BB1/BB2, both jump to BB3 (merge/ret).
fn diamond_cfg() -> Cfg {
    let mut cfg = Cfg::new();
    cfg.add_block(BasicBlock::new(
        BlockId(0),
        0,
        1,
        0..1,
        Terminator::Branch {
            then_target: BlockId(1),
            else_target: BlockId(2),
        },
    ));
    cfg.add_block(BasicBlock::new(
        BlockId(1),
        1,
        2,
        1..2,
        Terminator::Jump { target: BlockId(3) },
    ));
    cfg.add_block(BasicBlock::new(
        BlockId(2),
        2,
        3,
        2..3,
        Terminator::Jump { target: BlockId(3) },
    ));
    cfg.add_block(BasicBlock::new(BlockId(3), 3, 4, 3..4, Terminator::Return));
    cfg.add_edge(BlockId(0), BlockId(1), EdgeKind::ConditionalTrue);
    cfg.add_edge(BlockId(0), BlockId(2), EdgeKind::ConditionalFalse);
    cfg.add_edge(BlockId(1), BlockId(3), EdgeKind::Unconditional);
    cfg.add_edge(BlockId(2), BlockId(3), EdgeKind::Unconditional);
    cfg
}

fn block_with(stmts: Vec<SsaStmt>) -> SsaBlock {
    let mut b = SsaBlock::new();
    for s in stmts {
        b.add_stmt(s);
    }
    b
}

fn phi(target: SsaVariable, operands: &[(BlockId, SsaVariable)]) -> PhiNode {
    let mut phi = PhiNode::new(target);
    for (predecessor, operand) in operands {
        phi.add_operand(*predecessor, operand.clone());
    }
    phi
}

fn block_contains_call(block: &IrBlock, expected: &str) -> bool {
    block
        .stmts
        .iter()
        .any(|stmt| stmt_contains_call(stmt, expected))
}

fn stmt_contains_call(stmt: &Stmt, expected: &str) -> bool {
    match stmt {
        Stmt::Assign { value, .. } | Stmt::ExprStmt(value) => expr_contains_call(value, expected),
        Stmt::Return(value) => value
            .as_ref()
            .is_some_and(|value| expr_contains_call(value, expected)),
        Stmt::Throw(value) | Stmt::Abort(value) => value
            .as_ref()
            .is_some_and(|value| expr_contains_call(value, expected)),
        Stmt::Assert { condition, message } => {
            expr_contains_call(condition, expected)
                || message
                    .as_ref()
                    .is_some_and(|message| expr_contains_call(message, expected))
        }
        Stmt::ControlFlow(control_flow) => control_flow_contains_call(control_flow, expected),
        Stmt::Comment(_) | Stmt::Break | Stmt::Continue | Stmt::Label(_) | Stmt::Goto(_) => false,
    }
}

fn control_flow_contains_call(control_flow: &ControlFlow, expected: &str) -> bool {
    match control_flow {
        ControlFlow::If {
            condition,
            then_branch,
            else_branch,
        } => {
            expr_contains_call(condition, expected)
                || block_contains_call(then_branch, expected)
                || else_branch
                    .as_ref()
                    .is_some_and(|branch| block_contains_call(branch, expected))
        }
        ControlFlow::While { condition, body } | ControlFlow::DoWhile { body, condition } => {
            expr_contains_call(condition, expected) || block_contains_call(body, expected)
        }
        ControlFlow::For {
            init,
            condition,
            update,
            body,
        } => {
            init.as_ref()
                .is_some_and(|stmt| stmt_contains_call(stmt, expected))
                || condition
                    .as_ref()
                    .is_some_and(|expr| expr_contains_call(expr, expected))
                || update
                    .as_ref()
                    .is_some_and(|expr| expr_contains_call(expr, expected))
                || block_contains_call(body, expected)
        }
        ControlFlow::TryCatch {
            try_body,
            catch_body,
            finally_body,
            ..
        } => {
            block_contains_call(try_body, expected)
                || catch_body
                    .as_ref()
                    .is_some_and(|body| block_contains_call(body, expected))
                || finally_body
                    .as_ref()
                    .is_some_and(|body| block_contains_call(body, expected))
        }
        ControlFlow::Switch {
            expr,
            cases,
            default,
        } => {
            expr_contains_call(expr, expected)
                || cases.iter().any(|(value, body)| {
                    expr_contains_call(value, expected) || block_contains_call(body, expected)
                })
                || default
                    .as_ref()
                    .is_some_and(|body| block_contains_call(body, expected))
        }
    }
}

fn expr_contains_call(expr: &Expr, expected: &str) -> bool {
    match expr {
        Expr::Call { target, args } => {
            target.display_name() == expected
                || args
                    .iter()
                    .any(|argument| expr_contains_call(argument, expected))
        }
        Expr::Binary { left, right, .. } => {
            expr_contains_call(left, expected) || expr_contains_call(right, expected)
        }
        Expr::Unary { operand, .. } => expr_contains_call(operand, expected),
        Expr::Index { base, index } => {
            expr_contains_call(base, expected) || expr_contains_call(index, expected)
        }
        Expr::Member { base, .. } => expr_contains_call(base, expected),
        Expr::Cast { expr, .. } => expr_contains_call(expr, expected),
        Expr::Convert { value, .. } | Expr::IsType { value, .. } => {
            expr_contains_call(value, expected)
        }
        Expr::NewArray { length, .. } => expr_contains_call(length, expected),
        Expr::Array(elements) | Expr::Struct(elements) => elements
            .iter()
            .any(|element| expr_contains_call(element, expected)),
        Expr::Map(pairs) => pairs.iter().any(|(key, value)| {
            expr_contains_call(key, expected) || expr_contains_call(value, expected)
        }),
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_contains_call(condition, expected)
                || expr_contains_call(then_expr, expected)
                || expr_contains_call(else_expr, expected)
        }
        Expr::Unknown | Expr::Literal(_) | Expr::Variable(_) | Expr::StackTemp(_) => false,
    }
}

fn collect_transfers<'a>(block: &'a IrBlock, transfers: &mut Vec<&'a Stmt>) {
    for statement in &block.stmts {
        match statement {
            Stmt::Break | Stmt::Continue | Stmt::Label(_) | Stmt::Goto(_) => {
                transfers.push(statement);
            }
            Stmt::ControlFlow(control) => match control.as_ref() {
                ControlFlow::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    collect_transfers(then_branch, transfers);
                    if let Some(else_branch) = else_branch {
                        collect_transfers(else_branch, transfers);
                    }
                }
                ControlFlow::While { body, .. } | ControlFlow::DoWhile { body, .. } => {
                    collect_transfers(body, transfers);
                }
                ControlFlow::For { init, body, .. } => {
                    if matches!(
                        init.as_deref(),
                        Some(Stmt::Break | Stmt::Continue | Stmt::Label(_) | Stmt::Goto(_))
                    ) {
                        transfers.push(init.as_deref().expect("matched initializer"));
                    }
                    collect_transfers(body, transfers);
                }
                ControlFlow::TryCatch {
                    try_body,
                    catch_body,
                    finally_body,
                    ..
                } => {
                    collect_transfers(try_body, transfers);
                    if let Some(catch_body) = catch_body {
                        collect_transfers(catch_body, transfers);
                    }
                    if let Some(finally_body) = finally_body {
                        collect_transfers(finally_body, transfers);
                    }
                }
                ControlFlow::Switch { cases, default, .. } => {
                    for (_, body) in cases {
                        collect_transfers(body, transfers);
                    }
                    if let Some(default) = default {
                        collect_transfers(default, transfers);
                    }
                }
            },
            _ => {}
        }
    }
}

fn entry_self_loop_structure() -> IrBlock {
    const VIRTUAL_ENTRY: BlockId = BlockId(usize::MAX);

    let entry = BlockId(0);
    let mut cfg = Cfg::new();
    cfg.add_block(BasicBlock::new(
        entry,
        0,
        1,
        0..1,
        Terminator::Jump { target: entry },
    ));
    cfg.add_edge(entry, entry, EdgeKind::Unconditional);

    let state = v("state", 1);
    let initial = v("initial", 0);
    let next = v("next", 0);
    let mut block = block_with(vec![
        SsaStmt::assign(next.clone(), SsaExpr::lit(Literal::Int(2))),
        SsaStmt::expr(SsaExpr::unresolved_call(
            "check".to_string(),
            vec![SsaExpr::var(state.clone())],
        )),
    ]);
    block.add_phi(phi(
        state.clone(),
        &[(VIRTUAL_ENTRY, initial.clone()), (entry, next.clone())],
    ));
    let ssa = SsaForm {
        dominance: crate::decompiler::cfg::ssa::compute(&cfg),
        cfg,
        blocks: BTreeMap::from([(entry, block)]),
        definitions: BTreeMap::new(),
        uses: BTreeMap::from([(state, BTreeSet::from([UseSite::new(entry, 1)]))]),
    };

    structure(&ssa)
}

#[test]
fn structure_initializes_virtual_entry_phi_once() {
    let structured = entry_self_loop_structure();

    assert_eq!(
        structured.stmts,
        vec![
            Stmt::Assign {
                target: "state_1".to_string(),
                value: Expr::var("initial_0"),
            },
            Stmt::Assign {
                target: "next_0".to_string(),
                value: Expr::int(2),
            },
            Stmt::ExprStmt(Expr::unresolved_call("check", vec![Expr::var("state_1")])),
            Stmt::Assign {
                target: "state_1".to_string(),
                value: Expr::var("next_0"),
            },
        ]
    );
    assert!(!block_contains_call(&structured, "phi"));
}

#[test]
fn entry_self_loop_keeps_virtual_initialization_separate() {
    let structured = entry_self_loop_structure();

    assert!(matches!(
        structured.stmts.first(),
        Some(Stmt::Assign {
            target,
            value: Expr::Variable(source),
        }) if target == "state_1" && source == "initial_0"
    ));
    assert!(matches!(
        structured.stmts.last(),
        Some(Stmt::Assign {
            target,
            value: Expr::Variable(source),
        }) if target == "state_1" && source == "next_0"
    ));
    assert_eq!(
        structured
            .stmts
            .iter()
            .filter(|stmt| matches!(stmt, Stmt::Assign { target, .. } if target == "state_1"))
            .count(),
        2
    );
    assert!(!block_contains_call(&structured, "phi"));
}

#[test]
fn structure_emits_jump_edge_copy_before_merge_body() {
    let source = BlockId(0);
    let merge = BlockId(1);
    let mut cfg = Cfg::new();
    cfg.add_block(BasicBlock::new(
        source,
        0,
        1,
        0..1,
        Terminator::Jump { target: merge },
    ));
    cfg.add_block(BasicBlock::new(merge, 1, 2, 1..2, Terminator::Return));
    cfg.add_edge(source, merge, EdgeKind::Unconditional);

    let incoming = v("incoming", 0);
    let merged = v("merged", 0);
    let source_block = block_with(vec![SsaStmt::assign(
        incoming.clone(),
        SsaExpr::lit(Literal::Int(7)),
    )]);
    let mut merge_block = block_with(vec![
        SsaStmt::expr(SsaExpr::unresolved_call(
            "check".to_string(),
            vec![SsaExpr::var(merged.clone())],
        )),
        SsaStmt::ret(None),
    ]);
    merge_block.add_phi(phi(merged.clone(), &[(source, incoming)]));
    let ssa = SsaForm {
        dominance: crate::decompiler::cfg::ssa::compute(&cfg),
        cfg,
        blocks: BTreeMap::from([(source, source_block), (merge, merge_block)]),
        definitions: BTreeMap::new(),
        uses: BTreeMap::from([(merged, BTreeSet::from([UseSite::new(merge, 0)]))]),
    };

    let structured = structure(&ssa);

    assert_eq!(
        structured.stmts,
        vec![
            Stmt::Assign {
                target: "incoming_0".to_string(),
                value: Expr::int(7),
            },
            Stmt::Assign {
                target: "merged_0".to_string(),
                value: Expr::var("incoming_0"),
            },
            Stmt::ExprStmt(Expr::unresolved_call("check", vec![Expr::var("merged_0")])),
            Stmt::Return(None),
        ]
    );
    assert!(!block_contains_call(&structured, "phi"));
}

fn single_block_ssa(
    statements: Vec<SsaStmt>,
    uses: BTreeMap<SsaVariable, BTreeSet<UseSite>>,
) -> SsaForm {
    let mut cfg = Cfg::new();
    cfg.add_block(BasicBlock::new(BlockId(0), 0, 1, 0..1, Terminator::Return));
    let dominance = crate::decompiler::cfg::ssa::compute(&cfg);
    let blocks = BTreeMap::from([(BlockId(0), block_with(statements))]);
    SsaForm {
        cfg,
        dominance,
        blocks,
        definitions: BTreeMap::new(),
        uses,
    }
}

#[test]
fn adjacent_single_use_call_temp_is_returned_directly() {
    let temp = v("t", 0);
    let statements = vec![
        SsaStmt::assign(temp.clone(), SsaExpr::unresolved_call("read", vec![])),
        SsaStmt::ret(Some(SsaExpr::var(temp.clone()))),
    ];
    let uses = BTreeMap::from([(temp, BTreeSet::from([UseSite::new(BlockId(0), 1)]))]);

    let structured = structure(&single_block_ssa(statements, uses));

    assert_eq!(
        structured.stmts,
        vec![Stmt::Return(Some(Expr::unresolved_call("read", vec![])))]
    );
}

#[test]
fn unused_call_temp_is_an_expression_statement() {
    let structured = structure(&single_block_ssa(
        vec![
            SsaStmt::assign(v("t", 0), SsaExpr::unresolved_call("notify", vec![])),
            SsaStmt::ret(None),
        ],
        BTreeMap::new(),
    ));

    assert_eq!(
        structured.stmts,
        vec![
            Stmt::ExprStmt(Expr::unresolved_call("notify", vec![])),
            Stmt::Return(None),
        ]
    );
}

#[test]
fn missing_use_index_keeps_referenced_call_temp_assigned() {
    let temp = v("t", 0);
    let structured = structure(&single_block_ssa(
        vec![
            SsaStmt::assign(temp.clone(), SsaExpr::unresolved_call("read", vec![])),
            SsaStmt::ret(Some(SsaExpr::var(temp))),
        ],
        BTreeMap::new(),
    ));

    assert!(matches!(
        structured.stmts.as_slice(),
        [
            Stmt::Assign { target, .. },
            Stmt::Return(Some(Expr::Variable(returned)))
        ] if target == "t_0" && returned == "t_0"
    ));
}

#[test]
fn missing_cross_block_use_index_keeps_call_temp_assigned() {
    let temp = v("t", 0);
    let mut cfg = Cfg::new();
    cfg.add_block(BasicBlock::new(
        BlockId(0),
        0,
        1,
        0..1,
        Terminator::Jump { target: BlockId(1) },
    ));
    cfg.add_block(BasicBlock::new(BlockId(1), 1, 2, 1..2, Terminator::Return));
    cfg.add_edge(BlockId(0), BlockId(1), EdgeKind::Unconditional);
    let dominance = crate::decompiler::cfg::ssa::compute(&cfg);
    let blocks = BTreeMap::from([
        (
            BlockId(0),
            block_with(vec![SsaStmt::assign(
                temp.clone(),
                SsaExpr::unresolved_call("read", vec![]),
            )]),
        ),
        (
            BlockId(1),
            block_with(vec![SsaStmt::ret(Some(SsaExpr::var(temp)))]),
        ),
    ]);
    let ssa = SsaForm {
        cfg,
        dominance,
        blocks,
        definitions: BTreeMap::new(),
        uses: BTreeMap::new(),
    };

    let structured = structure(&ssa);

    assert!(matches!(
        structured.stmts.as_slice(),
        [
            Stmt::Assign { target, .. },
            Stmt::Return(Some(Expr::Variable(returned)))
        ] if target == "t_0" && returned == "t_0"
    ));
}

#[test]
fn multi_use_call_temp_remains_assigned() {
    let temp = v("t", 0);
    let structured = structure(&single_block_ssa(
        vec![
            SsaStmt::assign(temp.clone(), SsaExpr::unresolved_call("read", vec![])),
            SsaStmt::assign(v("loc0", 0), SsaExpr::var(temp.clone())),
            SsaStmt::ret(Some(SsaExpr::var(temp.clone()))),
        ],
        BTreeMap::from([(
            temp,
            BTreeSet::from([UseSite::new(BlockId(0), 1), UseSite::new(BlockId(0), 2)]),
        )]),
    ));

    assert!(matches!(
        structured.stmts.first(),
        Some(Stmt::Assign { target, .. }) if target == "t_0"
    ));
}

#[test]
fn named_slot_call_remains_assigned_when_unused() {
    let structured = structure(&single_block_ssa(
        vec![
            SsaStmt::assign(v("loc0", 0), SsaExpr::unresolved_call("read", vec![])),
            SsaStmt::ret(None),
        ],
        BTreeMap::new(),
    ));

    assert!(matches!(
        structured.stmts.first(),
        Some(Stmt::Assign { target, .. }) if target == "loc0_0"
    ));
}

#[test]
fn unused_non_call_temp_remains_assigned() {
    let structured = structure(&single_block_ssa(
        vec![
            SsaStmt::assign(v("t", 0), SsaExpr::lit(Literal::Int(7))),
            SsaStmt::ret(None),
        ],
        BTreeMap::new(),
    ));

    assert!(matches!(
        structured.stmts.first(),
        Some(Stmt::Assign { target, .. }) if target == "t_0"
    ));
}

#[test]
fn call_temp_used_as_call_argument_remains_assigned() {
    let temp = v("t", 0);
    let structured = structure(&single_block_ssa(
        vec![
            SsaStmt::assign(temp.clone(), SsaExpr::unresolved_call("read", vec![])),
            SsaStmt::expr(SsaExpr::unresolved_call(
                "consume".to_string(),
                vec![SsaExpr::var(temp.clone())],
            )),
            SsaStmt::ret(None),
        ],
        BTreeMap::from([(temp, BTreeSet::from([UseSite::new(BlockId(0), 1)]))]),
    ));

    assert!(matches!(
        structured.stmts.first(),
        Some(Stmt::Assign { target, .. }) if target == "t_0"
    ));
}

#[test]
fn bypassable_loop_node_is_not_a_shared_merge() {
    // B0 is an infinite-loop header. B1 can reach B3 or bypass it and
    // return directly to B0, while B2 always reaches B3. Reachability from
    // both arms is therefore insufficient to make B3 a shared tail.
    let mut cfg = Cfg::new();
    cfg.add_block(BasicBlock::new(
        BlockId(0),
        0,
        1,
        0..1,
        Terminator::Branch {
            then_target: BlockId(1),
            else_target: BlockId(2),
        },
    ));
    cfg.add_block(BasicBlock::new(
        BlockId(1),
        1,
        2,
        1..2,
        Terminator::Branch {
            then_target: BlockId(3),
            else_target: BlockId(0),
        },
    ));
    cfg.add_block(BasicBlock::new(
        BlockId(2),
        2,
        3,
        2..3,
        Terminator::Jump { target: BlockId(3) },
    ));
    cfg.add_block(BasicBlock::new(
        BlockId(3),
        3,
        4,
        3..4,
        Terminator::Jump { target: BlockId(0) },
    ));
    cfg.add_edge(BlockId(0), BlockId(1), EdgeKind::ConditionalTrue);
    cfg.add_edge(BlockId(0), BlockId(2), EdgeKind::ConditionalFalse);
    cfg.add_edge(BlockId(1), BlockId(3), EdgeKind::ConditionalTrue);
    cfg.add_edge(BlockId(1), BlockId(0), EdgeKind::ConditionalFalse);
    cfg.add_edge(BlockId(2), BlockId(3), EdgeKind::Unconditional);
    cfg.add_edge(BlockId(3), BlockId(0), EdgeKind::Unconditional);

    let dominance = crate::decompiler::cfg::ssa::compute(&cfg);
    let loop_headers = compute_loop_headers(&cfg, &dominance);
    let mut blocks = std::collections::BTreeMap::new();
    blocks.insert(
        BlockId(0),
        block_with(vec![SsaStmt::assign(
            v("header_cond", 0),
            SsaExpr::var(v("arg0", 0)),
        )]),
    );
    blocks.insert(
        BlockId(1),
        block_with(vec![SsaStmt::assign(
            v("inner_cond", 0),
            SsaExpr::var(v("arg1", 0)),
        )]),
    );
    blocks.insert(
        BlockId(2),
        block_with(vec![SsaStmt::assign(
            v("else_value", 0),
            SsaExpr::lit(Literal::Int(7)),
        )]),
    );
    blocks.insert(
        BlockId(3),
        block_with(vec![SsaStmt::assign(
            v("shared", 0),
            SsaExpr::lit(Literal::Int(42)),
        )]),
    );
    let ssa = SsaForm {
        cfg: cfg.clone(),
        dominance,
        blocks,
        definitions: std::collections::BTreeMap::new(),
        uses: std::collections::BTreeMap::new(),
    };
    let source_names = BTreeMap::new();
    let structural_uses = collect_structural_uses(&ssa);
    let phi_lowering = PhiLowering::new(&ssa, &source_names);
    let ctx = StructCtx {
        cfg: &ssa.cfg,
        ssa: &ssa,
        source_names: &source_names,
        loop_headers,
        postdominators: compute_postdominators(&ssa.cfg),
        structural_uses,
        leave_targets: collect_leave_targets(&ssa.cfg),
        phi_lowering,
    };
    let members = ctx.natural_loop_blocks(BlockId(0));

    assert_eq!(
        ctx.closest_loop_merge(BlockId(1), BlockId(2), BlockId(0), &members),
        None,
        "B3 is reachable from both arms but does not post-dominate B1"
    );

    let rendered = crate::decompiler::ir::render_block(&structure(&ssa), 0);
    assert_eq!(
        rendered.matches("shared_0 = 42;").count(),
        2,
        "bypassable shared code must remain in every branch that executes it:\n{rendered}"
    );
}

#[test]
fn bypassable_acyclic_join_is_not_selected_as_branch_merge() {
    // B3 is reachable from both outer arms, but B1 can bypass it and go
    // directly to the real merge B4. Selecting B3 drops the B2 -> B3 path.
    let mut cfg = Cfg::new();
    let terminators = [
        Terminator::Branch {
            then_target: BlockId(1),
            else_target: BlockId(2),
        },
        Terminator::Branch {
            then_target: BlockId(4),
            else_target: BlockId(3),
        },
        Terminator::Jump { target: BlockId(3) },
        Terminator::Jump { target: BlockId(4) },
        Terminator::Return,
    ];
    for (id, terminator) in terminators.into_iter().enumerate() {
        cfg.add_block(BasicBlock::new(
            BlockId(id),
            id,
            id + 1,
            id..id + 1,
            terminator,
        ));
    }
    cfg.add_edge(BlockId(0), BlockId(1), EdgeKind::ConditionalTrue);
    cfg.add_edge(BlockId(0), BlockId(2), EdgeKind::ConditionalFalse);
    cfg.add_edge(BlockId(1), BlockId(4), EdgeKind::ConditionalTrue);
    cfg.add_edge(BlockId(1), BlockId(3), EdgeKind::ConditionalFalse);
    cfg.add_edge(BlockId(2), BlockId(3), EdgeKind::Unconditional);
    cfg.add_edge(BlockId(3), BlockId(4), EdgeKind::Unconditional);

    let outer_condition = v("outer_condition", 0);
    let inner_condition = v("inner_condition", 0);
    let blocks = BTreeMap::from([
        (
            BlockId(0),
            block_with(vec![SsaStmt::assign(
                outer_condition.clone(),
                SsaExpr::var(v("arg0", 0)),
            )]),
        ),
        (
            BlockId(1),
            block_with(vec![SsaStmt::assign(
                inner_condition.clone(),
                SsaExpr::var(v("arg1", 0)),
            )]),
        ),
        (BlockId(2), SsaBlock::new()),
        (
            BlockId(3),
            block_with(vec![SsaStmt::assign(
                v("shared", 0),
                SsaExpr::lit(Literal::Int(42)),
            )]),
        ),
        (BlockId(4), block_with(vec![SsaStmt::ret(None)])),
    ]);
    let ssa = SsaForm {
        dominance: crate::decompiler::cfg::ssa::compute(&cfg),
        cfg,
        blocks,
        definitions: BTreeMap::new(),
        uses: BTreeMap::from([
            (
                outer_condition,
                BTreeSet::from([UseSite::terminator(BlockId(0))]),
            ),
            (
                inner_condition,
                BTreeSet::from([UseSite::terminator(BlockId(1))]),
            ),
        ]),
    };

    let rendered = crate::decompiler::ir::render_block(&structure(&ssa), 0);
    assert_eq!(
        rendered.matches("shared_0 = 42;").count(),
        2,
        "the bypassable block must remain on both paths that execute it:\n{rendered}"
    );
    assert!(
        rendered.lines().any(|line| line == "return;"),
        "the real post-dominating return must remain after the outer branch:\n{rendered}"
    );
}

#[test]
fn branch_headed_loop_with_terminal_exit_is_unconditional() {
    // Both successors of B0 remain in the natural loop. The only outgoing
    // edge is B6 -> B7 (return), so B0's comparison is an internal branch,
    // not the loop condition.
    let mut cfg = Cfg::new();
    let terminators = [
        Terminator::Branch {
            then_target: BlockId(1),
            else_target: BlockId(2),
        },
        Terminator::Branch {
            then_target: BlockId(6),
            else_target: BlockId(4),
        },
        Terminator::Jump { target: BlockId(4) },
        Terminator::Unknown,
        Terminator::Branch {
            then_target: BlockId(6),
            else_target: BlockId(5),
        },
        Terminator::Jump { target: BlockId(6) },
        Terminator::Branch {
            then_target: BlockId(7),
            else_target: BlockId(0),
        },
        Terminator::Return,
    ];
    for (id, terminator) in terminators.into_iter().enumerate() {
        cfg.add_block(BasicBlock::new(
            BlockId(id),
            id,
            id + 1,
            id..id + 1,
            terminator,
        ));
    }
    cfg.add_edge(BlockId(0), BlockId(1), EdgeKind::ConditionalTrue);
    cfg.add_edge(BlockId(0), BlockId(2), EdgeKind::ConditionalFalse);
    cfg.add_edge(BlockId(1), BlockId(6), EdgeKind::ConditionalTrue);
    cfg.add_edge(BlockId(1), BlockId(4), EdgeKind::ConditionalFalse);
    cfg.add_edge(BlockId(2), BlockId(4), EdgeKind::Unconditional);
    cfg.add_edge(BlockId(4), BlockId(6), EdgeKind::ConditionalTrue);
    cfg.add_edge(BlockId(4), BlockId(5), EdgeKind::ConditionalFalse);
    cfg.add_edge(BlockId(5), BlockId(6), EdgeKind::Unconditional);
    cfg.add_edge(BlockId(6), BlockId(7), EdgeKind::ConditionalTrue);
    cfg.add_edge(BlockId(6), BlockId(0), EdgeKind::ConditionalFalse);

    let low = v("low", 0);
    let high = v("high", 0);
    let masked = v("masked", 0);
    let done = v("done", 0);
    let result = v("result", 0);
    let blocks = BTreeMap::from([
        (
            BlockId(0),
            block_with(vec![
                SsaStmt::assign(
                    v("iteration", 0),
                    SsaExpr::unresolved_call("advance", vec![]),
                ),
                SsaStmt::assign(low.clone(), SsaExpr::var(v("arg0", 0))),
            ]),
        ),
        (
            BlockId(1),
            block_with(vec![SsaStmt::assign(
                high.clone(),
                SsaExpr::var(v("arg1", 0)),
            )]),
        ),
        (BlockId(2), SsaBlock::new()),
        (BlockId(3), SsaBlock::new()),
        (
            BlockId(4),
            block_with(vec![SsaStmt::assign(
                masked.clone(),
                SsaExpr::var(v("arg2", 0)),
            )]),
        ),
        (
            BlockId(5),
            block_with(vec![SsaStmt::assign(
                v("adjusted", 0),
                SsaExpr::lit(Literal::Int(1)),
            )]),
        ),
        (
            BlockId(6),
            block_with(vec![SsaStmt::assign(
                done.clone(),
                SsaExpr::var(v("arg3", 0)),
            )]),
        ),
        (
            BlockId(7),
            block_with(vec![SsaStmt::ret(Some(SsaExpr::var(result.clone())))]),
        ),
    ]);
    let ssa = SsaForm {
        dominance: crate::decompiler::cfg::ssa::compute(&cfg),
        cfg,
        blocks,
        definitions: BTreeMap::new(),
        uses: BTreeMap::from([
            (low, BTreeSet::from([UseSite::terminator(BlockId(0))])),
            (high, BTreeSet::from([UseSite::terminator(BlockId(1))])),
            (masked, BTreeSet::from([UseSite::terminator(BlockId(4))])),
            (done, BTreeSet::from([UseSite::terminator(BlockId(6))])),
            (result, BTreeSet::from([UseSite::new(BlockId(7), 0)])),
        ]),
    };

    let rendered = crate::decompiler::ir::render_block(&structure(&ssa), 0);
    assert!(rendered.contains("while (true)"), "{rendered}");
    assert!(rendered.contains("advance()"), "{rendered}");
    assert!(rendered.contains("return result_0;"), "{rendered}");
    assert!(
        !rendered.contains("while (low_0)"),
        "the internal header comparison must not become the loop condition:\n{rendered}"
    );
}

#[test]
fn removes_unreferenced_leave_label_after_terminal_try() {
    let inner = Stmt::ControlFlow(Box::new(ControlFlow::try_catch(
        IrBlock::with_stmts(vec![Stmt::Return(Some(Expr::int(1)))]),
        Some("exception_0".to_string()),
        Some(IrBlock::with_stmts(vec![Stmt::Return(Some(Expr::int(2)))])),
        Some(IrBlock::new()),
    )));
    let outer = Stmt::ControlFlow(Box::new(ControlFlow::try_catch(
        IrBlock::with_stmts(vec![inner]),
        None,
        None,
        Some(IrBlock::new()),
    )));
    let mut block = IrBlock::with_stmts(vec![
        outer.clone(),
        Stmt::Label(crate::decompiler::ir::BlockLabel(31)),
        Stmt::Return(Some(Expr::var("pending_return"))),
    ]);

    simplify_unreachable_control(&mut block);

    assert_eq!(block.stmts, vec![outer]);
}

#[test]
fn keeps_referenced_label_after_terminal_transfer() {
    let label = crate::decompiler::ir::BlockLabel(7);
    let mut block = IrBlock::with_stmts(vec![
        Stmt::Goto(label),
        Stmt::Return(None),
        Stmt::Label(label),
        Stmt::ExprStmt(Expr::unresolved_call("resume", vec![])),
    ]);

    simplify_unreachable_control(&mut block);

    assert_eq!(
        block.stmts,
        vec![
            Stmt::Goto(label),
            Stmt::Label(label),
            Stmt::ExprStmt(Expr::unresolved_call("resume", vec![])),
        ]
    );
}

#[test]
fn unreachable_goto_does_not_keep_its_label_alive() {
    let label = crate::decompiler::ir::BlockLabel(9);
    let mut block = IrBlock::with_stmts(vec![
        Stmt::Return(None),
        Stmt::Goto(label),
        Stmt::Label(label),
        Stmt::ExprStmt(Expr::unresolved_call("unreachable", vec![])),
    ]);

    simplify_unreachable_control(&mut block);

    assert_eq!(block.stmts, vec![Stmt::Return(None)]);
}

#[test]
fn constant_false_continue_self_loop_recovers_do_while() {
    let mut block =
        IrBlock::with_stmts(vec![Stmt::ControlFlow(Box::new(ControlFlow::while_loop(
            Expr::Literal(Literal::Bool(false)),
            IrBlock::with_stmts(vec![Stmt::Continue]),
        )))]);

    simplify_unreachable_control(&mut block);

    assert!(matches!(
        block.stmts.as_slice(),
        [Stmt::ControlFlow(control)]
            if matches!(control.as_ref(), ControlFlow::DoWhile {
                condition: Expr::Literal(Literal::Bool(false)),
                body,
            } if body.stmts.as_slice() == [Stmt::Continue])
    ));
}

#[test]
fn structures_a_diamond_into_an_if_else() {
    let cfg = diamond_cfg();
    let mut blocks = std::collections::BTreeMap::new();
    // BB0: condition def  b0_0 = (loc0 < 1)
    blocks.insert(
        BlockId(0),
        block_with(vec![SsaStmt::assign(
            v("b0", 0),
            SsaExpr::binary(
                BinOp::Lt,
                SsaExpr::var(v("loc0", 0)),
                SsaExpr::lit(Literal::Int(1)),
            ),
        )]),
    );
    // BB1 (then): b1_0 = 10
    blocks.insert(
        BlockId(1),
        block_with(vec![SsaStmt::assign(
            v("b1", 0),
            SsaExpr::lit(Literal::Int(10)),
        )]),
    );
    // BB2 (else): b2_0 = 20
    blocks.insert(
        BlockId(2),
        block_with(vec![SsaStmt::assign(
            v("b2", 0),
            SsaExpr::lit(Literal::Int(20)),
        )]),
    );
    blocks.insert(BlockId(3), SsaBlock::new());

    let ssa = SsaForm {
        cfg,
        dominance: DominanceInfo::new(),
        blocks,
        definitions: std::collections::BTreeMap::new(),
        uses: std::collections::BTreeMap::new(),
    };

    let structured = structure(&ssa);

    // Expect: condition assign, then an If ControlFlow with both branches.
    let has_if = structured
        .stmts
        .iter()
        .any(|s| matches!(s, Stmt::ControlFlow(cf) if matches!(**cf, ControlFlow::If { .. })));
    assert!(
        has_if,
        "expected an If ControlFlow; got {:?}",
        structured.stmts
    );

    let if_cf = structured
        .stmts
        .iter()
        .rev()
        .find_map(|s| match s {
            Stmt::ControlFlow(cf) => Some(cf),
            _ => None,
        })
        .expect("an If node");
    let ControlFlow::If {
        then_branch,
        else_branch,
        ..
    } = if_cf.as_ref()
    else {
        panic!("expected If, got {if_cf:?}");
    };
    assert!(!then_branch.is_empty(), "then-branch should carry BB1 body");
    assert!(
        else_branch.is_some(),
        "an if-else diamond should yield an else branch"
    );
}

#[test]
fn direct_branch_to_merge_copy_stays_inside_selected_arm() {
    let branch = BlockId(0);
    let merge = BlockId(1);
    let indirect = BlockId(2);
    let mut cfg = Cfg::new();
    cfg.add_block(BasicBlock::new(
        branch,
        0,
        1,
        0..1,
        Terminator::Branch {
            then_target: merge,
            else_target: indirect,
        },
    ));
    cfg.add_block(BasicBlock::new(merge, 1, 2, 1..2, Terminator::Return));
    cfg.add_block(BasicBlock::new(
        indirect,
        2,
        3,
        2..3,
        Terminator::Jump { target: merge },
    ));
    cfg.add_edge(branch, merge, EdgeKind::ConditionalTrue);
    cfg.add_edge(branch, indirect, EdgeKind::ConditionalFalse);
    cfg.add_edge(indirect, merge, EdgeKind::Unconditional);

    let direct_value = v("direct", 0);
    let indirect_value = v("indirect", 0);
    let condition = v("condition", 0);
    let merged = v("merged", 0);
    let branch_block = block_with(vec![
        SsaStmt::assign(direct_value.clone(), SsaExpr::lit(Literal::Int(10))),
        SsaStmt::assign(condition.clone(), SsaExpr::var(v("arg0", 0))),
    ]);
    let indirect_block = block_with(vec![SsaStmt::assign(
        indirect_value.clone(),
        SsaExpr::lit(Literal::Int(20)),
    )]);
    let mut merge_block = block_with(vec![
        SsaStmt::expr(SsaExpr::unresolved_call(
            "check".to_string(),
            vec![SsaExpr::var(merged.clone())],
        )),
        SsaStmt::ret(None),
    ]);
    merge_block.add_phi(phi(
        merged.clone(),
        &[(branch, direct_value), (indirect, indirect_value)],
    ));
    let ssa = SsaForm {
        dominance: crate::decompiler::cfg::ssa::compute(&cfg),
        cfg,
        blocks: BTreeMap::from([
            (branch, branch_block),
            (merge, merge_block),
            (indirect, indirect_block),
        ]),
        definitions: BTreeMap::new(),
        uses: BTreeMap::from([
            (condition, BTreeSet::from([UseSite::terminator(branch)])),
            (merged, BTreeSet::from([UseSite::new(merge, 0)])),
        ]),
    };

    let structured = structure(&ssa);
    let branch_stmt = structured
        .stmts
        .iter()
        .find_map(|stmt| match stmt {
            Stmt::ControlFlow(control_flow) => Some(control_flow.as_ref()),
            _ => None,
        })
        .expect("structured branch");
    let ControlFlow::If {
        then_branch,
        else_branch: Some(else_branch),
        ..
    } = branch_stmt
    else {
        panic!("expected an if/else branch, got {branch_stmt:?}");
    };

    assert_eq!(
        then_branch.stmts,
        vec![Stmt::Assign {
            target: "merged_0".to_string(),
            value: Expr::var("direct_0"),
        }],
        "the direct critical-edge copy must stay in the selected arm"
    );
    assert!(else_branch.stmts.contains(&Stmt::Assign {
        target: "merged_0".to_string(),
        value: Expr::var("indirect_0"),
    }));
    assert!(!block_contains_call(&structured, "phi"));
}

#[test]
fn degenerate_same_target_branch_emits_one_edge_copy() {
    let branch = BlockId(0);
    let target = BlockId(1);
    let mut cfg = Cfg::new();
    cfg.add_block(BasicBlock::new(
        branch,
        0,
        1,
        0..1,
        Terminator::Branch {
            then_target: target,
            else_target: target,
        },
    ));
    cfg.add_block(BasicBlock::new(target, 1, 2, 1..2, Terminator::Return));
    cfg.add_edge(branch, target, EdgeKind::ConditionalTrue);
    cfg.add_edge(branch, target, EdgeKind::ConditionalFalse);

    let incoming = v("incoming", 0);
    let condition = v("condition", 0);
    let merged = v("merged", 0);
    let mut target_block = block_with(vec![SsaStmt::ret(Some(SsaExpr::var(merged.clone())))]);
    target_block.add_phi(phi(merged.clone(), &[(branch, incoming.clone())]));
    let ssa = SsaForm {
        dominance: crate::decompiler::cfg::ssa::compute(&cfg),
        cfg,
        blocks: BTreeMap::from([
            (
                branch,
                block_with(vec![
                    SsaStmt::assign(incoming, SsaExpr::lit(Literal::Int(4))),
                    SsaStmt::assign(condition.clone(), SsaExpr::var(v("arg0", 0))),
                ]),
            ),
            (target, target_block),
        ]),
        definitions: BTreeMap::new(),
        uses: BTreeMap::from([
            (condition, BTreeSet::from([UseSite::terminator(branch)])),
            (merged, BTreeSet::from([UseSite::new(target, 0)])),
        ]),
    };

    let structured = structure(&ssa);
    let copy = Stmt::Assign {
        target: "merged_0".to_string(),
        value: Expr::var("incoming_0"),
    };
    assert_eq!(
        structured
            .stmts
            .iter()
            .filter(|stmt| *stmt == &copy)
            .count(),
        1
    );
    let copy_index = structured
        .stmts
        .iter()
        .position(|stmt| stmt == &copy)
        .expect("degenerate edge copy");
    assert!(matches!(
        structured.stmts.get(copy_index + 1),
        Some(Stmt::Return(Some(Expr::Variable(value)))) if value == "merged_0"
    ));
    assert!(!block_contains_call(&structured, "phi"));
}

#[test]
fn analysis_ssa_retains_phi_while_structured_ir_lowers_it() {
    let cfg = diamond_cfg();
    let left = v("left", 0);
    let right = v("right", 0);
    let merged = v("merged", 0);
    let mut merge_block = block_with(vec![
        SsaStmt::expr(SsaExpr::unresolved_call(
            "consume".to_string(),
            vec![SsaExpr::var(merged.clone())],
        )),
        SsaStmt::ret(None),
    ]);
    merge_block.add_phi(phi(
        merged.clone(),
        &[(BlockId(1), left.clone()), (BlockId(2), right.clone())],
    ));
    let ssa = SsaForm {
        dominance: crate::decompiler::cfg::ssa::compute(&cfg),
        cfg,
        blocks: BTreeMap::from([
            (
                BlockId(0),
                block_with(vec![SsaStmt::assign(
                    v("condition", 0),
                    SsaExpr::var(v("arg0", 0)),
                )]),
            ),
            (
                BlockId(1),
                block_with(vec![SsaStmt::assign(left, SsaExpr::lit(Literal::Int(1)))]),
            ),
            (
                BlockId(2),
                block_with(vec![SsaStmt::assign(right, SsaExpr::lit(Literal::Int(2)))]),
            ),
            (BlockId(3), merge_block),
        ]),
        definitions: BTreeMap::new(),
        uses: BTreeMap::from([(merged, BTreeSet::from([UseSite::new(BlockId(3), 0)]))]),
    };

    let analysis = crate::decompiler::cfg::ssa::render_ssa_form(&ssa);
    assert!(
        analysis.contains("merged_0 = φ(1: left_0, 2: right_0)"),
        "analysis SSA must retain predecessor-labelled phi semantics:\n{analysis}"
    );

    let structured = structure(&ssa);
    assert!(!block_contains_call(&structured, "phi"));
}

/// The if-condition must inline the comparison that drives the branch, not
/// render the bare reaching-definition variable. When BB0's last def is
/// `t = (loc0 < 1)`, the condition must be `(loc0 < 1)` and the def must
/// NOT be duplicated as a body statement.
#[test]
fn inlines_branch_comparison_condition_and_does_not_duplicate_it() {
    let mut cfg = Cfg::new();
    cfg.add_block(BasicBlock::new(
        BlockId(0),
        0,
        1,
        0..1,
        Terminator::Branch {
            then_target: BlockId(1),
            else_target: BlockId(2),
        },
    ));
    cfg.add_block(BasicBlock::new(
        BlockId(1),
        1,
        2,
        1..2,
        Terminator::Jump { target: BlockId(3) },
    ));
    cfg.add_block(BasicBlock::new(
        BlockId(2),
        2,
        3,
        2..3,
        Terminator::Jump { target: BlockId(3) },
    ));
    cfg.add_block(BasicBlock::new(BlockId(3), 3, 4, 3..4, Terminator::Return));
    cfg.add_edge(BlockId(0), BlockId(1), EdgeKind::ConditionalTrue);
    cfg.add_edge(BlockId(0), BlockId(2), EdgeKind::ConditionalFalse);
    cfg.add_edge(BlockId(1), BlockId(3), EdgeKind::Unconditional);
    cfg.add_edge(BlockId(2), BlockId(3), EdgeKind::Unconditional);

    let mut blocks = std::collections::BTreeMap::new();
    // BB0: only the comparison def — it IS the branch condition.
    blocks.insert(
        BlockId(0),
        block_with(vec![SsaStmt::assign(
            v("t", 0),
            SsaExpr::binary(
                BinOp::Lt,
                SsaExpr::var(v("loc0", 0)),
                SsaExpr::lit(Literal::Int(1)),
            ),
        )]),
    );
    blocks.insert(
        BlockId(1),
        block_with(vec![SsaStmt::assign(
            v("b1", 0),
            SsaExpr::lit(Literal::Int(10)),
        )]),
    );
    blocks.insert(
        BlockId(2),
        block_with(vec![SsaStmt::assign(
            v("b2", 0),
            SsaExpr::lit(Literal::Int(20)),
        )]),
    );
    blocks.insert(BlockId(3), SsaBlock::new());

    let ssa = SsaForm {
        cfg,
        dominance: DominanceInfo::new(),
        blocks,
        definitions: std::collections::BTreeMap::new(),
        uses: std::collections::BTreeMap::new(),
    };

    let structured = structure(&ssa);
    let rendered = crate::decompiler::ir::render_block(&structured, 0);

    // The condition must be the inlined comparison (versioned SSA name `loc0_0`,
    // double parens are a renderer quirk around a parenthesised Binary).
    assert!(
        rendered.contains("loc0_0 < 1"),
        "branch condition should inline the comparison; got:\n{rendered}"
    );
    // And it must NOT render the bare reaching-definition variable as the
    // condition.
    assert!(
        !rendered.contains("if (t_0)") && !rendered.contains("if (t)"),
        "branch condition should not be the bare t_0; got:\n{rendered}"
    );
    // The comparison def must not be duplicated as a body assignment.
    assert!(
            !rendered.contains("t_0 ="),
            "the comparison def must be consumed by the condition, not emitted in the body; got:\n{rendered}"
        );
}

#[test]
fn straight_line_cfg_emits_flat_block() {
    let mut cfg = Cfg::new();
    cfg.add_block(BasicBlock::new(BlockId(0), 0, 1, 0..2, Terminator::Return));
    let mut blocks = std::collections::BTreeMap::new();
    blocks.insert(
        BlockId(0),
        block_with(vec![
            SsaStmt::assign(v("b0", 0), SsaExpr::lit(Literal::Int(1))),
            SsaStmt::assign(v("b0", 1), SsaExpr::lit(Literal::Int(2))),
        ]),
    );
    let ssa = SsaForm {
        cfg,
        dominance: DominanceInfo::new(),
        blocks,
        definitions: std::collections::BTreeMap::new(),
        uses: std::collections::BTreeMap::new(),
    };
    let structured = structure(&ssa);
    let assigns = structured
        .stmts
        .iter()
        .filter(|s| matches!(s, Stmt::Assign { .. }))
        .count();
    assert_eq!(assigns, 2, "two assignments should be emitted as-is");
    assert!(matches!(structured.stmts[0], Stmt::Assign { .. }));
}

/// A while loop: BB0 (header) branches to BB1 (body) / BB2 (exit); BB1
/// jumps back to BB0. dominance(BB0 ≥ BB1) makes BB0 a loop header.
#[test]
fn structures_a_back_edge_into_a_while_loop() {
    let mut cfg = Cfg::new();
    cfg.add_block(BasicBlock::new(
        BlockId(0),
        0,
        1,
        0..1,
        Terminator::Branch {
            then_target: BlockId(1),
            else_target: BlockId(2),
        },
    ));
    cfg.add_block(BasicBlock::new(
        BlockId(1),
        1,
        2,
        1..2,
        Terminator::Jump { target: BlockId(0) },
    ));
    cfg.add_block(BasicBlock::new(BlockId(2), 2, 3, 2..3, Terminator::Return));
    cfg.add_edge(BlockId(0), BlockId(1), EdgeKind::ConditionalTrue);
    cfg.add_edge(BlockId(0), BlockId(2), EdgeKind::ConditionalFalse);
    cfg.add_edge(BlockId(1), BlockId(0), EdgeKind::Unconditional);
    let dominance = crate::decompiler::cfg::ssa::compute(&cfg);

    let mut blocks = std::collections::BTreeMap::new();
    // header condition def: b0_0 = (loc0 < 3)
    blocks.insert(
        BlockId(0),
        block_with(vec![SsaStmt::assign(
            v("t", 0),
            SsaExpr::binary(
                BinOp::Lt,
                SsaExpr::var(v("loc0", 0)),
                SsaExpr::lit(Literal::Int(3)),
            ),
        )]),
    );
    // body: b1_0 = 1
    blocks.insert(
        BlockId(1),
        block_with(vec![SsaStmt::assign(
            v("t", 1),
            SsaExpr::lit(Literal::Int(1)),
        )]),
    );
    blocks.insert(BlockId(2), SsaBlock::new());

    let ssa = SsaForm {
        cfg,
        dominance,
        blocks,
        definitions: std::collections::BTreeMap::new(),
        uses: std::collections::BTreeMap::new(),
    };

    let structured = structure(&ssa);
    let has_while = structured
        .stmts
        .iter()
        .any(|s| matches!(s, Stmt::ControlFlow(cf) if matches!(**cf, ControlFlow::While { .. })));
    assert!(
        has_while,
        "a back-edge branch should structure as a While; got {:?}",
        structured.stmts
    );
}

#[test]
fn nearest_loop_diamond_merge_stays_after_both_branch_arms() {
    let mut cfg = Cfg::new();
    let terminators = [
        Terminator::Branch {
            then_target: BlockId(1),
            else_target: BlockId(6),
        },
        Terminator::Branch {
            then_target: BlockId(2),
            else_target: BlockId(3),
        },
        Terminator::Jump { target: BlockId(4) },
        Terminator::Jump { target: BlockId(4) },
        Terminator::Jump { target: BlockId(5) },
        Terminator::Jump { target: BlockId(0) },
        Terminator::Return,
    ];
    for (id, terminator) in terminators.into_iter().enumerate() {
        cfg.add_block(BasicBlock::new(
            BlockId(id),
            id,
            id + 1,
            id..id + 1,
            terminator,
        ));
    }
    cfg.add_edge(BlockId(0), BlockId(1), EdgeKind::ConditionalTrue);
    cfg.add_edge(BlockId(0), BlockId(6), EdgeKind::ConditionalFalse);
    cfg.add_edge(BlockId(1), BlockId(2), EdgeKind::ConditionalTrue);
    cfg.add_edge(BlockId(1), BlockId(3), EdgeKind::ConditionalFalse);
    cfg.add_edge(BlockId(2), BlockId(4), EdgeKind::Unconditional);
    cfg.add_edge(BlockId(3), BlockId(4), EdgeKind::Unconditional);
    cfg.add_edge(BlockId(4), BlockId(5), EdgeKind::Unconditional);
    cfg.add_edge(BlockId(5), BlockId(0), EdgeKind::Unconditional);

    let blocks = BTreeMap::from([
        (
            BlockId(0),
            block_with(vec![SsaStmt::assign(
                v("outer_condition", 0),
                SsaExpr::var(v("arg0", 0)),
            )]),
        ),
        (
            BlockId(1),
            block_with(vec![SsaStmt::assign(
                v("inner_condition", 0),
                SsaExpr::var(v("arg1", 0)),
            )]),
        ),
        (
            BlockId(2),
            block_with(vec![SsaStmt::assign(
                v("then_marker", 0),
                SsaExpr::lit(Literal::Int(1)),
            )]),
        ),
        (
            BlockId(3),
            block_with(vec![SsaStmt::assign(
                v("else_marker", 0),
                SsaExpr::lit(Literal::Int(2)),
            )]),
        ),
        (
            BlockId(4),
            block_with(vec![SsaStmt::assign(
                v("shared", 0),
                SsaExpr::lit(Literal::Int(3)),
            )]),
        ),
        (BlockId(5), SsaBlock::new()),
        (BlockId(6), SsaBlock::new()),
    ]);
    let ssa = SsaForm {
        dominance: crate::decompiler::cfg::ssa::compute(&cfg),
        cfg,
        blocks,
        definitions: BTreeMap::new(),
        uses: BTreeMap::new(),
    };

    let rendered = crate::decompiler::ir::render_block(&structure(&ssa), 0);
    assert_eq!(rendered.matches("shared_0 = 3;").count(), 1, "{rendered}");
    assert!(
        rendered.lines().any(|line| line == "    shared_0 = 3;"),
        "the shared update must follow the inner if at loop-body indentation:\n{rendered}"
    );
}

#[test]
fn unconditional_backedge_to_try_entry_becomes_while_true() {
    let mut cfg = Cfg::new();
    cfg.add_block(BasicBlock::new(
        BlockId(0),
        0,
        1,
        0..1,
        Terminator::TryEntry {
            body_target: BlockId(1),
            catch_target: Some(BlockId(2)),
            finally_target: None,
        },
    ));
    cfg.add_block(BasicBlock::new(
        BlockId(1),
        1,
        2,
        1..2,
        Terminator::EndTry {
            continuation: BlockId(3),
            nonlocal: false,
        },
    ));
    cfg.add_block(BasicBlock::new(
        BlockId(2),
        2,
        3,
        2..3,
        Terminator::EndTry {
            continuation: BlockId(3),
            nonlocal: false,
        },
    ));
    cfg.add_block(BasicBlock::new(
        BlockId(3),
        3,
        4,
        3..4,
        Terminator::Jump { target: BlockId(0) },
    ));
    cfg.add_edge(BlockId(0), BlockId(1), EdgeKind::Unconditional);
    cfg.add_edge(BlockId(0), BlockId(2), EdgeKind::Exception);
    cfg.add_edge(BlockId(1), BlockId(3), EdgeKind::Unconditional);
    cfg.add_edge(BlockId(2), BlockId(3), EdgeKind::Unconditional);
    cfg.add_edge(BlockId(3), BlockId(0), EdgeKind::Unconditional);

    let ssa = SsaForm {
        dominance: crate::decompiler::cfg::ssa::compute(&cfg),
        cfg,
        blocks: (0..4).map(|id| (BlockId(id), SsaBlock::new())).collect(),
        definitions: BTreeMap::new(),
        uses: BTreeMap::new(),
    };
    let rendered = crate::decompiler::ir::render_block(&structure(&ssa), 0);

    assert!(rendered.contains("while (true)"), "{rendered}");
    assert!(rendered.contains("try {"), "{rendered}");
    assert!(rendered.contains("catch(exception_b2_0)"), "{rendered}");
}

#[test]
fn nonlocal_plain_endtry_returns_from_try_entry_loop() {
    let mut cfg = Cfg::new();
    let blocks = [
        Terminator::TryEntry {
            body_target: BlockId(1),
            catch_target: Some(BlockId(4)),
            finally_target: None,
        },
        Terminator::Branch {
            then_target: BlockId(2),
            else_target: BlockId(3),
        },
        Terminator::EndTry {
            continuation: BlockId(6),
            nonlocal: true,
        },
        Terminator::EndTry {
            continuation: BlockId(5),
            nonlocal: false,
        },
        Terminator::EndTry {
            continuation: BlockId(5),
            nonlocal: false,
        },
        Terminator::Jump { target: BlockId(0) },
        Terminator::Return,
    ];
    for (id, terminator) in blocks.into_iter().enumerate() {
        cfg.add_block(BasicBlock::new(
            BlockId(id),
            id,
            id + 1,
            id..id + 1,
            terminator,
        ));
    }
    cfg.add_edge(BlockId(0), BlockId(1), EdgeKind::Unconditional);
    cfg.add_edge(BlockId(0), BlockId(4), EdgeKind::Exception);
    cfg.add_edge(BlockId(1), BlockId(2), EdgeKind::ConditionalTrue);
    cfg.add_edge(BlockId(1), BlockId(3), EdgeKind::ConditionalFalse);
    cfg.add_edge(BlockId(2), BlockId(6), EdgeKind::Unconditional);
    cfg.add_edge(BlockId(3), BlockId(5), EdgeKind::Unconditional);
    cfg.add_edge(BlockId(4), BlockId(5), EdgeKind::Unconditional);
    cfg.add_edge(BlockId(5), BlockId(0), EdgeKind::Unconditional);

    let condition = v("condition", 0);
    let result = v("result", 0);
    let ssa = SsaForm {
        dominance: crate::decompiler::cfg::ssa::compute(&cfg),
        cfg,
        blocks: BTreeMap::from([
            (BlockId(0), SsaBlock::new()),
            (
                BlockId(1),
                block_with(vec![SsaStmt::assign(
                    condition.clone(),
                    SsaExpr::var(v("arg0", 0)),
                )]),
            ),
            (
                BlockId(2),
                block_with(vec![SsaStmt::assign(
                    result.clone(),
                    SsaExpr::lit(Literal::Int(7)),
                )]),
            ),
            (BlockId(3), SsaBlock::new()),
            (BlockId(4), SsaBlock::new()),
            (BlockId(5), SsaBlock::new()),
            (
                BlockId(6),
                block_with(vec![SsaStmt::ret(Some(SsaExpr::var(result.clone())))]),
            ),
        ]),
        definitions: BTreeMap::from([(result.clone(), BlockId(2))]),
        uses: BTreeMap::from([
            (condition, BTreeSet::from([UseSite::terminator(BlockId(1))])),
            (result, BTreeSet::from([UseSite::new(BlockId(6), 0)])),
        ]),
    };
    let rendered = crate::decompiler::ir::render_block(&structure(&ssa), 0);

    assert!(rendered.contains("while (true)"), "{rendered}");
    assert!(rendered.contains("return result_0;"), "{rendered}");
    assert!(
        rendered.find("return result_0;") < rendered.find("catch(exception_b4_0)"),
        "{rendered}"
    );
}

#[test]
fn structures_early_break_and_continue() {
    let header = BlockId(0);
    let continue_branch = BlockId(1);
    let break_branch = BlockId(2);
    let latch = BlockId(3);
    let follow = BlockId(4);
    let mut cfg = Cfg::new();
    cfg.add_block(BasicBlock::new(
        header,
        0,
        1,
        0..1,
        Terminator::Branch {
            then_target: continue_branch,
            else_target: follow,
        },
    ));
    cfg.add_block(BasicBlock::new(
        continue_branch,
        1,
        2,
        1..2,
        Terminator::Branch {
            then_target: header,
            else_target: break_branch,
        },
    ));
    cfg.add_block(BasicBlock::new(
        break_branch,
        2,
        3,
        2..3,
        Terminator::Branch {
            then_target: follow,
            else_target: latch,
        },
    ));
    cfg.add_block(BasicBlock::new(
        latch,
        3,
        4,
        3..4,
        Terminator::Jump { target: header },
    ));
    cfg.add_block(BasicBlock::new(follow, 4, 5, 4..5, Terminator::Return));
    cfg.add_edge(header, continue_branch, EdgeKind::ConditionalTrue);
    cfg.add_edge(header, follow, EdgeKind::ConditionalFalse);
    cfg.add_edge(continue_branch, header, EdgeKind::ConditionalTrue);
    cfg.add_edge(continue_branch, break_branch, EdgeKind::ConditionalFalse);
    cfg.add_edge(break_branch, follow, EdgeKind::ConditionalTrue);
    cfg.add_edge(break_branch, latch, EdgeKind::ConditionalFalse);
    cfg.add_edge(latch, header, EdgeKind::Unconditional);

    let header_condition = v("header_condition", 0);
    let continue_condition = v("continue_condition", 0);
    let break_condition = v("break_condition", 0);
    let ssa = SsaForm {
        dominance: crate::decompiler::cfg::ssa::compute(&cfg),
        cfg,
        blocks: BTreeMap::from([
            (
                header,
                block_with(vec![SsaStmt::assign(
                    header_condition.clone(),
                    SsaExpr::var(v("arg0", 0)),
                )]),
            ),
            (
                continue_branch,
                block_with(vec![SsaStmt::assign(
                    continue_condition.clone(),
                    SsaExpr::var(v("arg1", 0)),
                )]),
            ),
            (
                break_branch,
                block_with(vec![SsaStmt::assign(
                    break_condition.clone(),
                    SsaExpr::var(v("arg2", 0)),
                )]),
            ),
            (
                latch,
                block_with(vec![SsaStmt::expr(SsaExpr::unresolved_call(
                    "step".to_string(),
                    vec![],
                ))]),
            ),
            (follow, block_with(vec![SsaStmt::ret(None)])),
        ]),
        definitions: BTreeMap::new(),
        uses: BTreeMap::from([
            (
                header_condition,
                BTreeSet::from([UseSite::terminator(header)]),
            ),
            (
                continue_condition,
                BTreeSet::from([UseSite::terminator(continue_branch)]),
            ),
            (
                break_condition,
                BTreeSet::from([UseSite::terminator(break_branch)]),
            ),
        ]),
    };

    let structured = structure(&ssa);
    let mut transfers = Vec::new();
    collect_transfers(&structured, &mut transfers);

    assert!(
        transfers
            .iter()
            .any(|statement| matches!(statement, Stmt::Break)),
        "early loop exit must become break: {structured:?}"
    );
    assert!(
        transfers
            .iter()
            .any(|statement| matches!(statement, Stmt::Continue)),
        "early back-edge must become continue: {structured:?}"
    );
    assert!(
        transfers
            .iter()
            .all(|statement| !matches!(statement, Stmt::Label(_) | Stmt::Goto(_))),
        "reducible loops must not use labels or gotos: {structured:?}"
    );
}

#[test]
fn structures_false_edge_loop_body_with_nested_break() {
    let header = BlockId(0);
    let body_branch = BlockId(1);
    let update = BlockId(2);
    let follow = BlockId(3);
    let mut cfg = Cfg::new();
    cfg.add_block(BasicBlock::new(
        header,
        0,
        1,
        0..1,
        Terminator::Branch {
            then_target: follow,
            else_target: body_branch,
        },
    ));
    cfg.add_block(BasicBlock::new(
        body_branch,
        1,
        2,
        1..2,
        Terminator::Branch {
            then_target: follow,
            else_target: update,
        },
    ));
    cfg.add_block(BasicBlock::new(
        update,
        2,
        3,
        2..3,
        Terminator::Jump { target: header },
    ));
    cfg.add_block(BasicBlock::new(follow, 3, 4, 3..4, Terminator::Return));
    cfg.add_edge(header, follow, EdgeKind::ConditionalTrue);
    cfg.add_edge(header, body_branch, EdgeKind::ConditionalFalse);
    cfg.add_edge(body_branch, follow, EdgeKind::ConditionalTrue);
    cfg.add_edge(body_branch, update, EdgeKind::ConditionalFalse);
    cfg.add_edge(update, header, EdgeKind::Unconditional);

    let header_condition = v("header_condition", 0);
    let break_condition = v("break_condition", 0);
    let ssa = SsaForm {
        dominance: crate::decompiler::cfg::ssa::compute(&cfg),
        cfg,
        blocks: BTreeMap::from([
            (
                header,
                block_with(vec![SsaStmt::assign(
                    header_condition.clone(),
                    SsaExpr::binary(
                        BinOp::Ge,
                        SsaExpr::var(v("index", 0)),
                        SsaExpr::var(v("length", 0)),
                    ),
                )]),
            ),
            (
                body_branch,
                block_with(vec![SsaStmt::assign(
                    break_condition.clone(),
                    SsaExpr::var(v("stop", 0)),
                )]),
            ),
            (
                update,
                block_with(vec![SsaStmt::expr(SsaExpr::unresolved_call(
                    "step".to_string(),
                    vec![],
                ))]),
            ),
            (follow, block_with(vec![SsaStmt::ret(None)])),
        ]),
        definitions: BTreeMap::new(),
        uses: BTreeMap::from([
            (
                header_condition,
                BTreeSet::from([UseSite::terminator(header)]),
            ),
            (
                break_condition,
                BTreeSet::from([UseSite::terminator(body_branch)]),
            ),
        ]),
    };

    let structured = structure(&ssa);
    let (condition, body) = structured
        .stmts
        .iter()
        .find_map(|statement| match statement {
            Stmt::ControlFlow(control) => match control.as_ref() {
                ControlFlow::While { condition, body } => Some((condition, body)),
                _ => None,
            },
            _ => None,
        })
        .expect("false-edge loop must remain a while loop");

    assert!(
        matches!(
            condition,
            Expr::Unary {
                op: crate::decompiler::ir::UnaryOp::LogicalNot,
                operand,
            } if matches!(operand.as_ref(), Expr::Binary { op: BinOp::Ge, .. })
        ),
        "the exit-edge condition must be negated: {structured:?}"
    );
    let mut transfers = Vec::new();
    collect_transfers(body, &mut transfers);
    assert!(
        transfers
            .iter()
            .any(|statement| matches!(statement, Stmt::Break)),
        "the early exit must stay inside the loop body: {structured:?}"
    );
    assert!(
        structured
            .stmts
            .iter()
            .all(|statement| !matches!(statement, Stmt::Break)),
        "a loop transfer must not escape to method scope: {structured:?}"
    );
    assert!(
        structured
            .stmts
            .iter()
            .any(|statement| matches!(statement, Stmt::Return(None))),
        "the true-edge follow must resume after the loop: {structured:?}"
    );
}

#[test]
fn irreducible_region_uses_typed_labels() {
    let entry = BlockId(0);
    let left_entry = BlockId(1);
    let right_entry = BlockId(2);
    let exit = BlockId(3);
    let mut cfg = Cfg::new();
    cfg.add_block(BasicBlock::new(
        entry,
        0,
        1,
        0..1,
        Terminator::Branch {
            then_target: left_entry,
            else_target: right_entry,
        },
    ));
    cfg.add_block(BasicBlock::new(
        left_entry,
        1,
        2,
        1..2,
        Terminator::Jump {
            target: right_entry,
        },
    ));
    cfg.add_block(BasicBlock::new(
        right_entry,
        2,
        3,
        2..3,
        Terminator::Branch {
            then_target: left_entry,
            else_target: exit,
        },
    ));
    cfg.add_block(BasicBlock::new(exit, 3, 4, 3..4, Terminator::Return));
    cfg.add_edge(entry, left_entry, EdgeKind::ConditionalTrue);
    cfg.add_edge(entry, right_entry, EdgeKind::ConditionalFalse);
    cfg.add_edge(left_entry, right_entry, EdgeKind::Unconditional);
    cfg.add_edge(right_entry, left_entry, EdgeKind::ConditionalTrue);
    cfg.add_edge(right_entry, exit, EdgeKind::ConditionalFalse);

    let entry_condition = v("entry_condition", 0);
    let loop_condition = v("loop_condition", 0);
    let ssa = SsaForm {
        dominance: crate::decompiler::cfg::ssa::compute(&cfg),
        cfg,
        blocks: BTreeMap::from([
            (
                entry,
                block_with(vec![SsaStmt::assign(
                    entry_condition.clone(),
                    SsaExpr::var(v("arg0", 0)),
                )]),
            ),
            (
                left_entry,
                block_with(vec![SsaStmt::expr(SsaExpr::unresolved_call(
                    "left".to_string(),
                    vec![],
                ))]),
            ),
            (
                right_entry,
                block_with(vec![SsaStmt::assign(
                    loop_condition.clone(),
                    SsaExpr::var(v("arg1", 0)),
                )]),
            ),
            (exit, block_with(vec![SsaStmt::ret(None)])),
        ]),
        definitions: BTreeMap::new(),
        uses: BTreeMap::from([
            (
                entry_condition,
                BTreeSet::from([UseSite::terminator(entry)]),
            ),
            (
                loop_condition,
                BTreeSet::from([UseSite::terminator(right_entry)]),
            ),
        ]),
    };

    let first = structure(&ssa);
    let second = structure(&ssa);
    assert_eq!(first, second, "irreducible output must be deterministic");

    let mut transfers = Vec::new();
    collect_transfers(&first, &mut transfers);
    let labels: Vec<_> = transfers
        .iter()
        .filter_map(|statement| match statement {
            Stmt::Label(label) => Some(*label),
            _ => None,
        })
        .collect();
    assert!(
        labels.windows(2).all(|pair| pair[0] < pair[1]),
        "labels must follow deterministic block-id order: {labels:?}"
    );
    assert!(labels.contains(&crate::decompiler::ir::BlockLabel(1)));
    assert!(labels.contains(&crate::decompiler::ir::BlockLabel(2)));
    assert!(
        transfers
            .iter()
            .any(|statement| matches!(statement, Stmt::Goto(_))),
        "irreducible edges must remain explicit: {first:?}"
    );
    assert!(
            !first
                .stmts
                .iter()
                .any(|statement| matches!(statement, Stmt::Comment(comment) if comment.contains("incomplete"))),
            "valid irreducible control must not degrade to an incomplete comment: {first:?}"
        );
}

#[test]
fn infinite_loop_phi_copies_cover_both_arms_and_backedge() {
    const VIRTUAL_ENTRY: BlockId = BlockId(usize::MAX);

    let header = BlockId(0);
    let left_arm = BlockId(1);
    let right_arm = BlockId(2);
    let latch = BlockId(3);
    let mut cfg = Cfg::new();
    cfg.add_block(BasicBlock::new(
        header,
        0,
        1,
        0..1,
        Terminator::Branch {
            then_target: left_arm,
            else_target: right_arm,
        },
    ));
    for (id, offset) in [(left_arm, 1), (right_arm, 2)] {
        cfg.add_block(BasicBlock::new(
            id,
            offset,
            offset + 1,
            offset..offset + 1,
            Terminator::Jump { target: latch },
        ));
    }
    cfg.add_block(BasicBlock::new(
        latch,
        3,
        4,
        3..4,
        Terminator::Jump { target: header },
    ));
    cfg.add_edge(header, left_arm, EdgeKind::ConditionalTrue);
    cfg.add_edge(header, right_arm, EdgeKind::ConditionalFalse);
    cfg.add_edge(left_arm, latch, EdgeKind::Unconditional);
    cfg.add_edge(right_arm, latch, EdgeKind::Unconditional);
    cfg.add_edge(latch, header, EdgeKind::Unconditional);

    let state = v("state", 0);
    let condition = v("condition", 0);
    let left_entry = v("left_entry", 0);
    let left = v("left", 0);
    let right_entry = v("right_entry", 0);
    let right = v("right", 0);
    let merged = v("merged", 0);
    let next = v("next", 0);
    let mut header_block = block_with(vec![SsaStmt::assign(
        condition.clone(),
        SsaExpr::binary(
            BinOp::Lt,
            SsaExpr::var(state.clone()),
            SsaExpr::lit(Literal::Int(10)),
        ),
    )]);
    header_block.add_phi(phi(
        state.clone(),
        &[(VIRTUAL_ENTRY, v("initial", 0)), (latch, next.clone())],
    ));
    let mut left_block = block_with(vec![SsaStmt::assign(
        left.clone(),
        SsaExpr::binary(
            BinOp::Add,
            SsaExpr::var(left_entry.clone()),
            SsaExpr::lit(Literal::Int(1)),
        ),
    )]);
    left_block.add_phi(phi(left_entry.clone(), &[(header, state.clone())]));
    let mut right_block = block_with(vec![SsaStmt::assign(
        right.clone(),
        SsaExpr::binary(
            BinOp::Add,
            SsaExpr::var(right_entry.clone()),
            SsaExpr::lit(Literal::Int(2)),
        ),
    )]);
    right_block.add_phi(phi(right_entry.clone(), &[(header, state.clone())]));
    let mut latch_block = block_with(vec![SsaStmt::assign(
        next.clone(),
        SsaExpr::var(merged.clone()),
    )]);
    latch_block.add_phi(phi(
        merged.clone(),
        &[(left_arm, left.clone()), (right_arm, right.clone())],
    ));
    let ssa = SsaForm {
        dominance: crate::decompiler::cfg::ssa::compute(&cfg),
        cfg,
        blocks: BTreeMap::from([
            (header, header_block),
            (left_arm, left_block),
            (right_arm, right_block),
            (latch, latch_block),
        ]),
        definitions: BTreeMap::new(),
        uses: BTreeMap::from([
            (state, BTreeSet::from([UseSite::new(header, 0)])),
            (condition, BTreeSet::from([UseSite::terminator(header)])),
            (left_entry, BTreeSet::from([UseSite::new(left_arm, 0)])),
            (right_entry, BTreeSet::from([UseSite::new(right_arm, 0)])),
            (merged, BTreeSet::from([UseSite::new(latch, 0)])),
        ]),
    };

    let structured = structure(&ssa);
    let infinite = structured.stmts.iter().find_map(|stmt| match stmt {
        Stmt::ControlFlow(control_flow) => match control_flow.as_ref() {
            ControlFlow::While {
                condition: Expr::Literal(Literal::Bool(true)),
                body,
            } => Some(body),
            _ => None,
        },
        _ => None,
    });
    let body = infinite.expect("infinite loop");
    let branch = body.stmts.iter().find_map(|stmt| match stmt {
        Stmt::ControlFlow(control_flow) => match control_flow.as_ref() {
            ControlFlow::If {
                then_branch,
                else_branch: Some(else_branch),
                ..
            } => Some((then_branch, else_branch)),
            _ => None,
        },
        _ => None,
    });
    let (then_branch, else_branch) = branch.expect("infinite loop branch");
    assert!(then_branch.stmts.contains(&Stmt::Assign {
        target: "left_entry_0".to_string(),
        value: Expr::var("state_0"),
    }));
    assert!(else_branch.stmts.contains(&Stmt::Assign {
        target: "right_entry_0".to_string(),
        value: Expr::var("state_0"),
    }));
    assert!(matches!(
        body.stmts.last(),
        Some(Stmt::Assign {
            target,
            value: Expr::Variable(source),
        }) if target == "state_0" && source == "next_0"
    ));
    assert!(!block_contains_call(&structured, "phi"));
}

#[test]
fn while_phi_copies_run_in_preheader_and_latch() {
    let preheader = BlockId(0);
    let header = BlockId(1);
    let body = BlockId(2);
    let exit = BlockId(3);
    let mut cfg = Cfg::new();
    cfg.add_block(BasicBlock::new(
        preheader,
        0,
        1,
        0..1,
        Terminator::Jump { target: header },
    ));
    cfg.add_block(BasicBlock::new(
        header,
        1,
        2,
        1..2,
        Terminator::Branch {
            then_target: body,
            else_target: exit,
        },
    ));
    cfg.add_block(BasicBlock::new(
        body,
        2,
        3,
        2..3,
        Terminator::Jump { target: header },
    ));
    cfg.add_block(BasicBlock::new(exit, 3, 4, 3..4, Terminator::Return));
    cfg.add_edge(preheader, header, EdgeKind::Unconditional);
    cfg.add_edge(header, body, EdgeKind::ConditionalTrue);
    cfg.add_edge(header, exit, EdgeKind::ConditionalFalse);
    cfg.add_edge(body, header, EdgeKind::Unconditional);

    let seed = v("seed", 0);
    let state = v("state", 0);
    let condition = v("condition", 0);
    let body_value = v("body_value", 0);
    let next = v("next", 0);
    let exit_value = v("exit_value", 0);

    let preheader_block = block_with(vec![SsaStmt::assign(
        seed.clone(),
        SsaExpr::lit(Literal::Int(0)),
    )]);
    let mut header_block = block_with(vec![SsaStmt::assign(
        condition.clone(),
        SsaExpr::binary(
            BinOp::Lt,
            SsaExpr::var(state.clone()),
            SsaExpr::lit(Literal::Int(3)),
        ),
    )]);
    header_block.add_phi(phi(
        state.clone(),
        &[(preheader, seed.clone()), (body, next.clone())],
    ));
    let mut body_block = block_with(vec![SsaStmt::assign(
        next.clone(),
        SsaExpr::binary(
            BinOp::Add,
            SsaExpr::var(body_value.clone()),
            SsaExpr::lit(Literal::Int(1)),
        ),
    )]);
    body_block.add_phi(phi(body_value.clone(), &[(header, state.clone())]));
    let mut exit_block = block_with(vec![
        SsaStmt::expr(SsaExpr::unresolved_call(
            "consume".to_string(),
            vec![SsaExpr::var(exit_value.clone())],
        )),
        SsaStmt::ret(None),
    ]);
    exit_block.add_phi(phi(exit_value.clone(), &[(header, state.clone())]));
    let ssa = SsaForm {
        dominance: crate::decompiler::cfg::ssa::compute(&cfg),
        cfg,
        blocks: BTreeMap::from([
            (preheader, preheader_block),
            (header, header_block),
            (body, body_block),
            (exit, exit_block),
        ]),
        definitions: BTreeMap::new(),
        uses: BTreeMap::from([
            (state, BTreeSet::from([UseSite::new(header, 0)])),
            (condition, BTreeSet::from([UseSite::terminator(header)])),
            (body_value, BTreeSet::from([UseSite::new(body, 0)])),
            (exit_value, BTreeSet::from([UseSite::new(exit, 0)])),
        ]),
    };

    let structured = structure(&ssa);
    let while_index = structured
            .stmts
            .iter()
            .position(|stmt| {
                matches!(stmt, Stmt::ControlFlow(control_flow) if matches!(control_flow.as_ref(), ControlFlow::While { .. }))
            })
            .expect("while loop");
    let Stmt::ControlFlow(control_flow) = &structured.stmts[while_index] else {
        unreachable!();
    };
    let ControlFlow::While { body, .. } = control_flow.as_ref() else {
        unreachable!();
    };

    assert!(structured.stmts[..while_index].contains(&Stmt::Assign {
        target: "state_0".to_string(),
        value: Expr::var("seed_0"),
    }));
    assert!(matches!(
        body.stmts.first(),
        Some(Stmt::Assign {
            target,
            value: Expr::Variable(source),
        }) if target == "body_value_0" && source == "state_0"
    ));
    assert!(matches!(
        body.stmts.last(),
        Some(Stmt::Assign {
            target,
            value: Expr::Variable(source),
        }) if target == "state_0" && source == "next_0"
    ));
    assert!(matches!(
        structured.stmts.get(while_index + 1),
        Some(Stmt::Assign {
            target,
            value: Expr::Variable(source),
        }) if target == "exit_value_0" && source == "state_0"
    ));
    assert!(!block_contains_call(&structured, "phi"));
}

/// A try/catch: TryEntry{body, catch, finally=None}; body and catch both
/// reach an EndTry whose continuation is the post-try block.
#[test]
fn structures_a_try_entry_into_try_catch() {
    let mut cfg = Cfg::new();
    cfg.add_block(BasicBlock::new(
        BlockId(0),
        0,
        1,
        0..1,
        Terminator::TryEntry {
            body_target: BlockId(1),
            catch_target: Some(BlockId(2)),
            finally_target: None,
        },
    ));
    cfg.add_block(BasicBlock::new(
        BlockId(1),
        1,
        2,
        1..2,
        Terminator::Jump { target: BlockId(3) },
    ));
    cfg.add_block(BasicBlock::new(
        BlockId(2),
        2,
        3,
        2..3,
        Terminator::Jump { target: BlockId(3) },
    ));
    cfg.add_block(BasicBlock::new(
        BlockId(3),
        3,
        4,
        3..4,
        Terminator::EndTry {
            continuation: BlockId(4),
            nonlocal: false,
        },
    ));
    cfg.add_block(BasicBlock::new(BlockId(4), 4, 5, 4..5, Terminator::Return));
    cfg.add_edge(BlockId(0), BlockId(1), EdgeKind::Unconditional);
    cfg.add_edge(BlockId(0), BlockId(2), EdgeKind::Unconditional);
    cfg.add_edge(BlockId(1), BlockId(3), EdgeKind::Unconditional);
    cfg.add_edge(BlockId(2), BlockId(3), EdgeKind::Unconditional);
    let dominance = crate::decompiler::cfg::ssa::compute(&cfg);

    let mut blocks = std::collections::BTreeMap::new();
    blocks.insert(
        BlockId(1),
        block_with(vec![SsaStmt::assign(
            v("t", 0),
            SsaExpr::lit(Literal::Int(1)),
        )]),
    );
    blocks.insert(
        BlockId(2),
        block_with(vec![SsaStmt::assign(
            v("t", 1),
            SsaExpr::lit(Literal::Int(2)),
        )]),
    );
    blocks.insert(BlockId(0), SsaBlock::new());
    blocks.insert(BlockId(3), SsaBlock::new());
    blocks.insert(BlockId(4), SsaBlock::new());

    let ssa = SsaForm {
        cfg,
        dominance,
        blocks,
        definitions: std::collections::BTreeMap::new(),
        uses: std::collections::BTreeMap::new(),
    };

    let structured = structure(&ssa);
    let has_try = structured.stmts.iter().any(
        |s| matches!(s, Stmt::ControlFlow(cf) if matches!(**cf, ControlFlow::TryCatch { .. })),
    );
    assert!(
        has_try,
        "a TryEntry should structure as TryCatch; got {:?}",
        structured.stmts
    );
}

#[test]
fn direct_leave_successor_is_hoisted_as_the_branch_merge() {
    let branch = BlockId(0);
    let try_entry = BlockId(1);
    let try_body = BlockId(2);
    let catch = BlockId(3);
    let leave_target = BlockId(4);
    let return_block = BlockId(5);
    let mut cfg = Cfg::new();
    cfg.add_block(BasicBlock::new(
        branch,
        0,
        1,
        0..1,
        Terminator::Branch {
            then_target: try_entry,
            else_target: leave_target,
        },
    ));
    cfg.add_block(BasicBlock::new(
        try_entry,
        1,
        2,
        1..2,
        Terminator::TryEntry {
            body_target: try_body,
            catch_target: Some(catch),
            finally_target: None,
        },
    ));
    cfg.add_block(BasicBlock::new(try_body, 2, 3, 2..3, Terminator::Throw));
    cfg.add_block(BasicBlock::new(
        catch,
        3,
        4,
        3..4,
        Terminator::EndTry {
            continuation: leave_target,
            nonlocal: true,
        },
    ));
    cfg.add_block(BasicBlock::new(
        leave_target,
        4,
        5,
        4..5,
        Terminator::Jump {
            target: return_block,
        },
    ));
    cfg.add_block(BasicBlock::new(
        return_block,
        5,
        6,
        5..6,
        Terminator::Return,
    ));
    cfg.add_edge(branch, try_entry, EdgeKind::ConditionalTrue);
    cfg.add_edge(branch, leave_target, EdgeKind::ConditionalFalse);
    cfg.add_edge(try_entry, try_body, EdgeKind::Unconditional);
    cfg.add_edge(try_entry, catch, EdgeKind::Exception);
    cfg.add_edge(catch, leave_target, EdgeKind::Unconditional);
    cfg.add_edge(leave_target, return_block, EdgeKind::Unconditional);

    let condition = v("condition", 0);
    let ssa = SsaForm {
        dominance: crate::decompiler::cfg::ssa::compute(&cfg),
        cfg,
        blocks: BTreeMap::from([
            (
                branch,
                block_with(vec![SsaStmt::assign(
                    condition.clone(),
                    SsaExpr::var(v("arg0", 0)),
                )]),
            ),
            (try_entry, SsaBlock::new()),
            (
                try_body,
                block_with(vec![SsaStmt::throw(Some(SsaExpr::lit(Literal::Int(1))))]),
            ),
            (catch, SsaBlock::new()),
            (
                leave_target,
                block_with(vec![SsaStmt::expr(SsaExpr::unresolved_call(
                    "after".to_string(),
                    vec![],
                ))]),
            ),
            (return_block, block_with(vec![SsaStmt::ret(None)])),
        ]),
        definitions: BTreeMap::new(),
        uses: BTreeMap::from([(condition, BTreeSet::from([UseSite::terminator(branch)]))]),
    };

    let structured = structure(&ssa);
    let branch_index = structured
            .stmts
            .iter()
            .position(|statement| matches!(statement, Stmt::ControlFlow(control) if matches!(control.as_ref(), ControlFlow::If { else_branch: None, .. })))
            .expect("if without a sibling else");
    let label_index = structured
            .stmts
            .iter()
            .position(|statement| matches!(statement, Stmt::Label(label) if *label == crate::decompiler::ir::BlockLabel(leave_target.index())))
            .unwrap_or_else(|| panic!("leave label at parent scope: {structured:?}"));
    assert!(branch_index < label_index, "{structured:?}");
}

#[test]
fn try_phi_copies_stay_in_their_selected_region() {
    let entry = BlockId(0);
    let body = BlockId(1);
    let catch = BlockId(2);
    let end_try = BlockId(3);
    let continuation = BlockId(4);
    let mut cfg = Cfg::new();
    cfg.add_block(BasicBlock::new(
        entry,
        0,
        1,
        0..1,
        Terminator::TryEntry {
            body_target: body,
            catch_target: Some(catch),
            finally_target: None,
        },
    ));
    cfg.add_block(BasicBlock::new(
        body,
        1,
        2,
        1..2,
        Terminator::Jump { target: end_try },
    ));
    cfg.add_block(BasicBlock::new(
        catch,
        2,
        3,
        2..3,
        Terminator::Jump { target: end_try },
    ));
    cfg.add_block(BasicBlock::new(
        end_try,
        3,
        4,
        3..4,
        Terminator::EndTry {
            continuation,
            nonlocal: false,
        },
    ));
    cfg.add_block(BasicBlock::new(
        continuation,
        4,
        5,
        4..5,
        Terminator::Return,
    ));
    cfg.add_edge(entry, body, EdgeKind::Unconditional);
    cfg.add_edge(entry, catch, EdgeKind::Exception);
    cfg.add_edge(body, end_try, EdgeKind::Unconditional);
    cfg.add_edge(catch, end_try, EdgeKind::Unconditional);
    cfg.add_edge(end_try, continuation, EdgeKind::Unconditional);

    let body_value = v("body_value", 0);
    let catch_value = v("catch_value", 0);
    let mut body_block = block_with(vec![SsaStmt::expr(SsaExpr::unresolved_call(
        "consume_body".to_string(),
        vec![SsaExpr::var(body_value.clone())],
    ))]);
    body_block.add_phi(phi(body_value.clone(), &[(entry, v("arg_body", 0))]));
    let mut catch_block = block_with(vec![SsaStmt::expr(SsaExpr::unresolved_call(
        "consume_catch".to_string(),
        vec![SsaExpr::var(catch_value.clone())],
    ))]);
    catch_block.add_phi(phi(catch_value.clone(), &[(entry, v("arg_catch", 0))]));
    let ssa = SsaForm {
        dominance: crate::decompiler::cfg::ssa::compute(&cfg),
        cfg,
        blocks: BTreeMap::from([
            (entry, SsaBlock::new()),
            (body, body_block),
            (catch, catch_block),
            (end_try, SsaBlock::new()),
            (continuation, block_with(vec![SsaStmt::ret(None)])),
        ]),
        definitions: BTreeMap::new(),
        uses: BTreeMap::from([
            (body_value, BTreeSet::from([UseSite::new(body, 0)])),
            (catch_value, BTreeSet::from([UseSite::new(catch, 0)])),
        ]),
    };

    let structured = structure(&ssa);
    let try_catch = structured.stmts.iter().find_map(|stmt| match stmt {
        Stmt::ControlFlow(control_flow)
            if matches!(control_flow.as_ref(), ControlFlow::TryCatch { .. }) =>
        {
            Some(control_flow.as_ref())
        }
        _ => None,
    });
    let Some(ControlFlow::TryCatch {
        try_body,
        catch_body: Some(catch_body),
        ..
    }) = try_catch
    else {
        panic!("expected try/catch, got {:?}", structured.stmts);
    };
    let body_copy = Stmt::Assign {
        target: "body_value_0".to_string(),
        value: Expr::var("arg_body_0"),
    };
    let catch_copy = Stmt::Assign {
        target: "catch_value_0".to_string(),
        value: Expr::var("arg_catch_0"),
    };

    assert!(try_body.stmts.contains(&body_copy));
    assert!(!try_body.stmts.contains(&catch_copy));
    assert!(catch_body.stmts.contains(&catch_copy));
    assert!(!catch_body.stmts.contains(&body_copy));
    assert!(!block_contains_call(&structured, "phi"));
}

#[test]
fn endtry_continuation_copy_is_shared_after_all_regions() {
    let entry = BlockId(0);
    let body = BlockId(1);
    let catch = BlockId(2);
    let finally = BlockId(3);
    let end_try = BlockId(4);
    let continuation = BlockId(5);
    let mut cfg = Cfg::new();
    cfg.add_block(BasicBlock::new(
        entry,
        0,
        1,
        0..1,
        Terminator::TryEntry {
            body_target: body,
            catch_target: Some(catch),
            finally_target: Some(finally),
        },
    ));
    for (id, offset) in [(body, 1), (catch, 2), (finally, 3)] {
        cfg.add_block(BasicBlock::new(
            id,
            offset,
            offset + 1,
            offset..offset + 1,
            Terminator::Jump { target: end_try },
        ));
    }
    cfg.add_block(BasicBlock::new(
        end_try,
        4,
        5,
        4..5,
        Terminator::EndTry {
            continuation,
            nonlocal: false,
        },
    ));
    cfg.add_block(BasicBlock::new(
        continuation,
        5,
        6,
        5..6,
        Terminator::Return,
    ));
    cfg.add_edge(entry, body, EdgeKind::Unconditional);
    cfg.add_edge(entry, catch, EdgeKind::Exception);
    cfg.add_edge(entry, finally, EdgeKind::Finally);
    cfg.add_edge(body, end_try, EdgeKind::Unconditional);
    cfg.add_edge(catch, end_try, EdgeKind::Unconditional);
    cfg.add_edge(finally, end_try, EdgeKind::Unconditional);
    cfg.add_edge(end_try, continuation, EdgeKind::Unconditional);

    let shared = v("shared", 0);
    let continued = v("continued", 0);
    let finally_value = v("finally_value", 0);
    let mut finally_block = block_with(vec![SsaStmt::expr(SsaExpr::unresolved_call(
        "finally".to_string(),
        vec![SsaExpr::var(finally_value.clone())],
    ))]);
    finally_block.add_phi(phi(finally_value.clone(), &[(entry, v("arg_finally", 0))]));
    let mut continuation_block = block_with(vec![
        SsaStmt::expr(SsaExpr::unresolved_call(
            "consume".to_string(),
            vec![SsaExpr::var(continued.clone())],
        )),
        SsaStmt::ret(None),
    ]);
    continuation_block.add_phi(phi(continued.clone(), &[(end_try, shared.clone())]));
    let ssa = SsaForm {
        dominance: crate::decompiler::cfg::ssa::compute(&cfg),
        cfg,
        blocks: BTreeMap::from([
            (entry, SsaBlock::new()),
            (
                body,
                block_with(vec![SsaStmt::expr(SsaExpr::unresolved_call(
                    "body".to_string(),
                    vec![],
                ))]),
            ),
            (
                catch,
                block_with(vec![SsaStmt::expr(SsaExpr::unresolved_call(
                    "catch".to_string(),
                    vec![],
                ))]),
            ),
            (finally, finally_block),
            (
                end_try,
                block_with(vec![SsaStmt::assign(shared, SsaExpr::lit(Literal::Int(9)))]),
            ),
            (continuation, continuation_block),
        ]),
        definitions: BTreeMap::new(),
        uses: BTreeMap::from([
            (finally_value, BTreeSet::from([UseSite::new(finally, 0)])),
            (continued, BTreeSet::from([UseSite::new(continuation, 0)])),
        ]),
    };

    let structured = structure(&ssa);
    let try_index = structured
            .stmts
            .iter()
            .position(|stmt| {
                matches!(stmt, Stmt::ControlFlow(control_flow) if matches!(control_flow.as_ref(), ControlFlow::TryCatch { .. }))
            })
            .expect("try/catch/finally");
    let Stmt::ControlFlow(control_flow) = &structured.stmts[try_index] else {
        unreachable!();
    };
    let ControlFlow::TryCatch {
        try_body,
        catch_body: Some(catch_body),
        finally_body: Some(finally_body),
        ..
    } = control_flow.as_ref()
    else {
        unreachable!();
    };
    let shared_copy = Stmt::Assign {
        target: "continued_0".to_string(),
        value: Expr::var("shared_0"),
    };
    assert!(finally_body.stmts.contains(&Stmt::Assign {
        target: "finally_value_0".to_string(),
        value: Expr::var("arg_finally_0"),
    }));

    assert!(!try_body.stmts.contains(&shared_copy));
    assert!(!catch_body.stmts.contains(&shared_copy));
    assert!(!finally_body.stmts.contains(&shared_copy));
    assert!(matches!(
        structured.stmts.get(try_index + 1),
        Some(Stmt::Assign {
            target,
            value: Expr::Literal(Literal::Int(9)),
        }) if target == "shared_0"
    ));
    assert_eq!(structured.stmts.get(try_index + 2), Some(&shared_copy));
    assert_eq!(
        structured
            .stmts
            .iter()
            .filter(|stmt| *stmt == &shared_copy)
            .count(),
        1
    );
}

/// A do-while: BB0 (body entry, falls through to the latch) is the loop
/// header; BB1 (latch) tests the condition and branches back to BB0 or out
/// to BB2. BB0 dominates BB1, so BB0 is a loop header whose terminator is
/// not a Branch → do-while.
#[test]
fn structures_a_bottom_tested_loop_into_do_while() {
    let mut cfg = Cfg::new();
    cfg.add_block(BasicBlock::new(
        BlockId(0),
        0,
        1,
        0..1,
        Terminator::Fallthrough { target: BlockId(1) },
    ));
    cfg.add_block(BasicBlock::new(
        BlockId(1),
        1,
        2,
        1..2,
        Terminator::Branch {
            then_target: BlockId(0),
            else_target: BlockId(2),
        },
    ));
    cfg.add_block(BasicBlock::new(BlockId(2), 2, 3, 2..3, Terminator::Return));
    cfg.add_edge(BlockId(0), BlockId(1), EdgeKind::Unconditional);
    cfg.add_edge(BlockId(1), BlockId(0), EdgeKind::ConditionalTrue);
    cfg.add_edge(BlockId(1), BlockId(2), EdgeKind::ConditionalFalse);
    let dominance = crate::decompiler::cfg::ssa::compute(&cfg);

    let mut blocks = std::collections::BTreeMap::new();
    // body: b0_0 = step()
    blocks.insert(
        BlockId(0),
        block_with(vec![SsaStmt::assign(
            v("t", 0),
            SsaExpr::unresolved_call("step", vec![]),
        )]),
    );
    // latch condition: b1_0 = (loc0 < 3)
    blocks.insert(
        BlockId(1),
        block_with(vec![SsaStmt::assign(
            v("t", 1),
            SsaExpr::binary(
                BinOp::Lt,
                SsaExpr::var(v("loc0", 0)),
                SsaExpr::lit(Literal::Int(3)),
            ),
        )]),
    );
    blocks.insert(BlockId(2), SsaBlock::new());

    let ssa = SsaForm {
        cfg,
        dominance,
        blocks,
        definitions: std::collections::BTreeMap::new(),
        uses: std::collections::BTreeMap::new(),
    };

    let structured = structure(&ssa);
    let has_dowhile = structured
        .stmts
        .iter()
        .any(|s| matches!(s, Stmt::ControlFlow(cf) if matches!(**cf, ControlFlow::DoWhile { .. })));
    assert!(
        has_dowhile,
        "a bottom-tested loop should structure as DoWhile; got {:?}",
        structured.stmts
    );
}

#[test]
fn do_while_phi_backedge_copy_stays_in_body() {
    const VIRTUAL_ENTRY: BlockId = BlockId(usize::MAX);

    let header = BlockId(0);
    let latch = BlockId(1);
    let exit = BlockId(2);
    let mut cfg = Cfg::new();
    cfg.add_block(BasicBlock::new(
        header,
        0,
        1,
        0..1,
        Terminator::Fallthrough { target: latch },
    ));
    cfg.add_block(BasicBlock::new(
        latch,
        1,
        2,
        1..2,
        Terminator::Branch {
            then_target: header,
            else_target: exit,
        },
    ));
    cfg.add_block(BasicBlock::new(exit, 2, 3, 2..3, Terminator::Return));
    cfg.add_edge(header, latch, EdgeKind::Unconditional);
    cfg.add_edge(latch, header, EdgeKind::ConditionalTrue);
    cfg.add_edge(latch, exit, EdgeKind::ConditionalFalse);

    let state = v("state", 0);
    let initial = v("initial", 0);
    let next = v("next", 0);
    let condition = v("condition", 0);
    let exit_value = v("exit_value", 0);
    let mut header_block = block_with(vec![SsaStmt::assign(
        next.clone(),
        SsaExpr::binary(
            BinOp::Add,
            SsaExpr::var(state.clone()),
            SsaExpr::lit(Literal::Int(1)),
        ),
    )]);
    header_block.add_phi(phi(
        state.clone(),
        &[(VIRTUAL_ENTRY, initial), (latch, next.clone())],
    ));
    let latch_block = block_with(vec![SsaStmt::assign(
        condition.clone(),
        SsaExpr::binary(
            BinOp::Lt,
            SsaExpr::var(next.clone()),
            SsaExpr::lit(Literal::Int(3)),
        ),
    )]);
    let mut exit_block = block_with(vec![
        SsaStmt::expr(SsaExpr::unresolved_call(
            "consume".to_string(),
            vec![SsaExpr::var(exit_value.clone())],
        )),
        SsaStmt::ret(None),
    ]);
    exit_block.add_phi(phi(exit_value.clone(), &[(latch, state.clone())]));
    let ssa = SsaForm {
        dominance: crate::decompiler::cfg::ssa::compute(&cfg),
        cfg,
        blocks: BTreeMap::from([
            (header, header_block),
            (latch, latch_block),
            (exit, exit_block),
        ]),
        definitions: BTreeMap::new(),
        uses: BTreeMap::from([
            (state, BTreeSet::from([UseSite::new(header, 0)])),
            (condition, BTreeSet::from([UseSite::terminator(latch)])),
            (exit_value, BTreeSet::from([UseSite::new(exit, 0)])),
        ]),
    };

    let structured = structure(&ssa);
    let loop_index = structured
            .stmts
            .iter()
            .position(|stmt| {
                matches!(stmt, Stmt::ControlFlow(control_flow) if matches!(control_flow.as_ref(), ControlFlow::DoWhile { .. }))
            })
            .expect("do-while loop");
    let Stmt::ControlFlow(control_flow) = &structured.stmts[loop_index] else {
        unreachable!();
    };
    let ControlFlow::DoWhile { body, .. } = control_flow.as_ref() else {
        unreachable!();
    };

    assert!(matches!(
        structured.stmts.first(),
        Some(Stmt::Assign {
            target,
            value: Expr::Variable(source),
        }) if target == "state_0" && source == "initial_0"
    ));
    let guarded_backedge = body.stmts.iter().find_map(|stmt| match stmt {
        Stmt::ControlFlow(control_flow) => match control_flow.as_ref() {
            ControlFlow::If {
                then_branch,
                else_branch: None,
                ..
            } if then_branch.stmts.contains(&Stmt::Assign {
                target: "state_0".to_string(),
                value: Expr::var("next_0"),
            }) =>
            {
                Some(then_branch)
            }
            _ => None,
        },
        _ => None,
    });
    assert!(
        guarded_backedge.is_some(),
        "the backedge copy must be guarded inside the do-while body: {body:?}"
    );
    assert!(
        !body.stmts.iter().any(|stmt| matches!(
            stmt,
            Stmt::Assign {
                target,
                value: Expr::Variable(source),
            } if target == "state_0" && source == "next_0"
        )),
        "the false exit must not execute the backedge copy: {body:?}"
    );
    assert!(matches!(
        structured.stmts.get(loop_index + 1),
        Some(Stmt::Assign {
            target,
            value: Expr::Variable(source),
        }) if target == "exit_value_0" && source == "state_0"
    ));
    assert!(!block_contains_call(&structured, "phi"));
}

/// A switch: an equality cascade on one scrutinee. B0 compares `loc0 == 0`
/// → case0 body, else B1; B1 compares `loc0 == 1` → case1 body, else B2
/// (default); all bodies join at the merge B5.
#[test]
fn structures_an_equality_cascade_into_a_switch() {
    let mut cfg = Cfg::new();
    // B0: loc0 == 0 ? case0 : B1
    cfg.add_block(BasicBlock::new(
        BlockId(0),
        0,
        1,
        0..1,
        Terminator::Branch {
            then_target: BlockId(3),
            else_target: BlockId(1),
        },
    ));
    // B1: loc0 == 1 ? case1 : default(B2)
    cfg.add_block(BasicBlock::new(
        BlockId(1),
        1,
        2,
        1..2,
        Terminator::Branch {
            then_target: BlockId(4),
            else_target: BlockId(2),
        },
    ));
    // B2 (default) -> merge
    cfg.add_block(BasicBlock::new(
        BlockId(2),
        2,
        3,
        2..3,
        Terminator::Jump { target: BlockId(5) },
    ));
    // B3 (case0 body) -> merge
    cfg.add_block(BasicBlock::new(
        BlockId(3),
        3,
        4,
        3..4,
        Terminator::Jump { target: BlockId(5) },
    ));
    // B4 (case1 body) -> merge
    cfg.add_block(BasicBlock::new(
        BlockId(4),
        4,
        5,
        4..5,
        Terminator::Jump { target: BlockId(5) },
    ));
    // B5 (merge)
    cfg.add_block(BasicBlock::new(BlockId(5), 5, 6, 5..6, Terminator::Return));
    cfg.add_edge(BlockId(0), BlockId(3), EdgeKind::ConditionalTrue);
    cfg.add_edge(BlockId(0), BlockId(1), EdgeKind::ConditionalFalse);
    cfg.add_edge(BlockId(1), BlockId(4), EdgeKind::ConditionalTrue);
    cfg.add_edge(BlockId(1), BlockId(2), EdgeKind::ConditionalFalse);
    cfg.add_edge(BlockId(2), BlockId(5), EdgeKind::Unconditional);
    cfg.add_edge(BlockId(3), BlockId(5), EdgeKind::Unconditional);
    cfg.add_edge(BlockId(4), BlockId(5), EdgeKind::Unconditional);
    let dominance = crate::decompiler::cfg::ssa::compute(&cfg);

    let mut blocks = std::collections::BTreeMap::new();
    // B0: t0 = (loc0 == 0)
    blocks.insert(
        BlockId(0),
        block_with(vec![SsaStmt::assign(
            v("t", 0),
            SsaExpr::binary(
                BinOp::Eq,
                SsaExpr::var(v("loc0", 0)),
                SsaExpr::lit(Literal::Int(0)),
            ),
        )]),
    );
    // B1: t1 = (loc0 == 1)
    blocks.insert(
        BlockId(1),
        block_with(vec![SsaStmt::assign(
            v("t", 1),
            SsaExpr::binary(
                BinOp::Eq,
                SsaExpr::var(v("loc0", 1)),
                SsaExpr::lit(Literal::Int(1)),
            ),
        )]),
    );
    let default_value = v("default_value", 0);
    let case0_value = v("case0_value", 0);
    let case1_value = v("case1_value", 0);
    let mut default_block = block_with(vec![SsaStmt::expr(SsaExpr::unresolved_call(
        "consume_default".to_string(),
        vec![SsaExpr::var(default_value.clone())],
    ))]);
    default_block.add_phi(phi(
        default_value.clone(),
        &[(BlockId(1), v("arg_default", 0))],
    ));
    blocks.insert(BlockId(2), default_block);
    let mut case0_block = block_with(vec![SsaStmt::expr(SsaExpr::unresolved_call(
        "consume_case0".to_string(),
        vec![SsaExpr::var(case0_value.clone())],
    ))]);
    case0_block.add_phi(phi(case0_value.clone(), &[(BlockId(0), v("arg_case0", 0))]));
    blocks.insert(BlockId(3), case0_block);
    let mut case1_block = block_with(vec![SsaStmt::expr(SsaExpr::unresolved_call(
        "consume_case1".to_string(),
        vec![SsaExpr::var(case1_value.clone())],
    ))]);
    case1_block.add_phi(phi(case1_value.clone(), &[(BlockId(1), v("arg_case1", 0))]));
    blocks.insert(BlockId(4), case1_block);
    blocks.insert(BlockId(5), SsaBlock::new());

    let ssa = SsaForm {
        cfg,
        dominance,
        blocks,
        definitions: std::collections::BTreeMap::new(),
        uses: BTreeMap::from([
            (default_value, BTreeSet::from([UseSite::new(BlockId(2), 0)])),
            (case0_value, BTreeSet::from([UseSite::new(BlockId(3), 0)])),
            (case1_value, BTreeSet::from([UseSite::new(BlockId(4), 0)])),
        ]),
    };

    let structured = structure(&ssa);
    let switch = structured.stmts.iter().find_map(|stmt| match stmt {
        Stmt::ControlFlow(control_flow)
            if matches!(control_flow.as_ref(), ControlFlow::Switch { .. }) =>
        {
            Some(control_flow.as_ref())
        }
        _ => None,
    });
    let Some(ControlFlow::Switch { cases, default, .. }) = switch else {
        panic!(
            "an equality cascade on one scrutinee should structure as a Switch; got {:?}",
            structured.stmts
        );
    };
    assert!(cases[0].1.stmts.contains(&Stmt::Assign {
        target: "case0_value_0".to_string(),
        value: Expr::var("arg_case0_0"),
    }));
    assert!(cases[1].1.stmts.contains(&Stmt::Assign {
        target: "case1_value_0".to_string(),
        value: Expr::var("arg_case1_0"),
    }));
    assert!(default
        .as_ref()
        .expect("switch default")
        .stmts
        .contains(&Stmt::Assign {
            target: "default_value_0".to_string(),
            value: Expr::var("arg_default_0"),
        }));
}
