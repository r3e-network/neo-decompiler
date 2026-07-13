use super::*;

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
