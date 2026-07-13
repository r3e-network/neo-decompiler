use super::*;

/// A do-while: BB0 (body entry, falls through to the latch) is the loop
/// header; BB1 (latch) tests the condition and branches back to BB0 or out
/// to BB2. BB0 dominates BB1, so BB0 is a loop header whose terminator is
/// not a Branch -> do-while.

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
