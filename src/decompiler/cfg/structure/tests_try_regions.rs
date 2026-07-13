use super::*;

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
