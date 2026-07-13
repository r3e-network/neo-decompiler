use super::*;

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
