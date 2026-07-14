use super::*;

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
fn promotes_explicit_induction_loop_to_for() {
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

    let induction = v("index", 0);
    let next_induction = v("index", 1);
    let condition = v("condition", 0);
    let ssa = SsaForm {
        dominance: crate::decompiler::cfg::ssa::compute(&cfg),
        cfg,
        blocks: BTreeMap::from([
            (
                preheader,
                block_with(vec![SsaStmt::assign(
                    induction.clone(),
                    SsaExpr::lit(Literal::Int(0)),
                )]),
            ),
            (
                header,
                block_with(vec![SsaStmt::assign(
                    condition.clone(),
                    SsaExpr::binary(
                        BinOp::Lt,
                        SsaExpr::var(induction.clone()),
                        SsaExpr::lit(Literal::Int(3)),
                    ),
                )]),
            ),
            (
                body,
                block_with(vec![SsaStmt::assign(
                    next_induction,
                    SsaExpr::unary(UnaryOp::Inc, SsaExpr::var(induction.clone())),
                )]),
            ),
            (exit, SsaBlock::new()),
        ]),
        definitions: BTreeMap::new(),
        uses: BTreeMap::from([(condition, BTreeSet::from([UseSite::terminator(header)]))]),
    };

    let structured = structure(&ssa);
    let Some(Stmt::ControlFlow(control)) = structured
        .stmts
        .iter()
        .find(|statement| matches!(statement, Stmt::ControlFlow(control) if matches!(control.as_ref(), ControlFlow::For { .. })))
    else {
        panic!("expected promoted for loop: {structured:?}");
    };
    let ControlFlow::For {
        init,
        condition,
        update,
        ..
    } = control.as_ref()
    else {
        panic!("expected promoted for loop: {structured:?}");
    };
    assert!(matches!(init.as_deref(), Some(Stmt::Assign { target, .. }) if target == "index_0"));
    assert!(matches!(condition, Some(Expr::Binary { .. })));
    assert!(matches!(
        update,
        Some(Expr::Unary {
            op: UnaryOp::Inc,
            ..
        })
    ));
}

#[test]
fn promotes_compiler_copy_chain_induction_loop_to_for() {
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

    let induction = v("index", 0);
    let temporary = v("temp", 0);
    let condition = v("condition", 0);
    let ssa = SsaForm {
        dominance: crate::decompiler::cfg::ssa::compute(&cfg),
        cfg,
        blocks: BTreeMap::from([
            (
                preheader,
                block_with(vec![
                    SsaStmt::assign(induction.clone(), SsaExpr::lit(Literal::Int(0))),
                    SsaStmt::assign(v("collection", 0), SsaExpr::lit(Literal::Int(7))),
                ]),
            ),
            (
                header,
                block_with(vec![SsaStmt::assign(
                    condition.clone(),
                    SsaExpr::binary(
                        BinOp::Lt,
                        SsaExpr::var(induction.clone()),
                        SsaExpr::lit(Literal::Int(3)),
                    ),
                )]),
            ),
            (
                body,
                block_with(vec![
                    SsaStmt::assign(
                        temporary.clone(),
                        SsaExpr::unary(UnaryOp::Inc, SsaExpr::var(induction.clone())),
                    ),
                    SsaStmt::assign(induction.clone(), SsaExpr::var(temporary)),
                ]),
            ),
            (exit, SsaBlock::new()),
        ]),
        definitions: BTreeMap::new(),
        uses: BTreeMap::from([(condition, BTreeSet::from([UseSite::terminator(header)]))]),
    };

    let structured = structure(&ssa);
    assert!(
        structured.stmts.iter().any(|statement| matches!(
            statement,
            Stmt::ControlFlow(control) if matches!(control.as_ref(), ControlFlow::For { .. })
        )),
        "expected compiler copy-chain induction loop to promote: {structured:?}"
    );
}
