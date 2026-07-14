use super::*;

fn terminal_return_scan_loop(extra_update_effect: bool) -> SsaForm {
    let preheader = BlockId(0);
    let header = BlockId(1);
    let body = BlockId(2);
    let exit = BlockId(3);
    let terminal = BlockId(4);
    let update = BlockId(5);
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
        Terminator::Branch {
            then_target: terminal,
            else_target: update,
        },
    ));
    cfg.add_block(BasicBlock::new(exit, 3, 4, 3..4, Terminator::Return));
    cfg.add_block(BasicBlock::new(terminal, 4, 5, 4..5, Terminator::Return));
    cfg.add_block(BasicBlock::new(
        update,
        5,
        6,
        5..6,
        Terminator::Jump { target: header },
    ));
    cfg.add_edge(preheader, header, EdgeKind::Unconditional);
    cfg.add_edge(header, body, EdgeKind::ConditionalTrue);
    cfg.add_edge(header, exit, EdgeKind::ConditionalFalse);
    cfg.add_edge(body, terminal, EdgeKind::ConditionalTrue);
    cfg.add_edge(body, update, EdgeKind::ConditionalFalse);
    cfg.add_edge(update, header, EdgeKind::Unconditional);

    let induction = v("index", 0);
    let next_induction = v("index", 1);
    let header_condition = v("header_condition", 0);
    let body_condition = v("body_condition", 0);
    let mut update_stmts = vec![SsaStmt::assign(
        next_induction,
        SsaExpr::binary(
            BinOp::Add,
            SsaExpr::var(induction.clone()),
            SsaExpr::lit(Literal::Int(1)),
        ),
    )];
    if extra_update_effect {
        update_stmts.push(SsaStmt::assign(
            v("observed", 0),
            SsaExpr::lit(Literal::Int(1)),
        ));
    }
    SsaForm {
        dominance: crate::decompiler::cfg::ssa::compute(&cfg),
        cfg,
        blocks: BTreeMap::from([
            (
                preheader,
                block_with(vec![SsaStmt::assign(
                    induction,
                    SsaExpr::lit(Literal::Int(0)),
                )]),
            ),
            (
                header,
                block_with(vec![SsaStmt::assign(
                    header_condition.clone(),
                    SsaExpr::binary(
                        BinOp::Lt,
                        SsaExpr::var(v("index", 0)),
                        SsaExpr::lit(Literal::Int(3)),
                    ),
                )]),
            ),
            (
                body,
                block_with(vec![SsaStmt::assign(
                    body_condition.clone(),
                    SsaExpr::lit(Literal::Bool(true)),
                )]),
            ),
            (
                exit,
                block_with(vec![SsaStmt::ret(Some(SsaExpr::lit(Literal::Bool(false))))]),
            ),
            (
                terminal,
                block_with(vec![SsaStmt::ret(Some(SsaExpr::lit(Literal::Bool(true))))]),
            ),
            (update, block_with(update_stmts)),
        ]),
        definitions: BTreeMap::new(),
        uses: BTreeMap::from([
            (
                header_condition,
                BTreeSet::from([UseSite::terminator(header)]),
            ),
            (body_condition, BTreeSet::from([UseSite::terminator(body)])),
        ]),
    }
}

#[test]
fn promotes_terminal_return_scan_loop_to_for() {
    let structured = structure(&terminal_return_scan_loop(false));
    let Some(Stmt::ControlFlow(control)) = structured.stmts.iter().find(|statement| {
        matches!(
            statement,
            Stmt::ControlFlow(control) if matches!(control.as_ref(), ControlFlow::For { .. })
        )
    }) else {
        panic!("expected terminal-return scan loop to promote: {structured:?}");
    };
    let ControlFlow::For { update, body, .. } = control.as_ref() else {
        unreachable!();
    };
    assert!(matches!(
        update,
        Some(Expr::Unary {
            op: UnaryOp::Inc,
            operand,
        }) if **operand == Expr::var("index_0")
    ));
    assert!(body.stmts.iter().any(|statement| {
        matches!(
            statement,
            Stmt::ControlFlow(control) if matches!(control.as_ref(), ControlFlow::If {
                else_branch: None,
                then_branch,
                ..
            } if then_branch.stmts.iter().any(|statement| matches!(
                statement,
                Stmt::Return(Some(Expr::Literal(Literal::Bool(true))))
            )))
        )
    }));
}

#[test]
fn terminal_return_scan_loop_keeps_while_with_extra_update_effect() {
    let structured = structure(&terminal_return_scan_loop(true));
    assert!(
        structured.stmts.iter().any(|statement| matches!(
            statement,
            Stmt::ControlFlow(control) if matches!(control.as_ref(), ControlFlow::While { .. })
        )),
        "an update branch with extra effects must remain a while loop: {structured:?}"
    );
    assert!(
        !structured.stmts.iter().any(|statement| matches!(
            statement,
            Stmt::ControlFlow(control) if matches!(control.as_ref(), ControlFlow::For { .. })
        )),
        "an update branch with extra effects must not promote: {structured:?}"
    );
}
