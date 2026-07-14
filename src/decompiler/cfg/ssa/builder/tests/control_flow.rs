use super::*;

#[test]
fn entry_loop_keeps_manifest_arguments_as_incoming_slots() {
    let instructions = vec![instr(0, OpCode::Ldarg0), instr(1, OpCode::Drop)];
    let mut cfg = Cfg::new();
    cfg.add_block(BasicBlock::new(
        BlockId(0),
        0,
        2,
        0..2,
        Terminator::Jump { target: BlockId(0) },
    ));
    cfg.add_edge(BlockId(0), BlockId(0), EdgeKind::Unconditional);
    let context = MethodContext {
        argument_names: vec!["value".to_string()],
        ..MethodContext::default()
    };

    let ssa = SsaBuilder::new(&cfg, &instructions)
        .with_method_context(&context)
        .build();
    let block = ssa.block(BlockId(0)).expect("entry loop block");

    assert!(
        block.stmts.iter().any(|stmt| matches!(
            stmt,
            SsaStmt::Assign {
                value: SsaExpr::Variable(source),
                ..
            } if source == &SsaVariable::initial("arg0".to_string())
        )),
        "entry-loop LDARG0 must read the incoming manifest argument: {block:?}"
    );
}

#[test]
fn inferred_entry_stack_arguments_follow_vm_order() {
    let instructions = vec![instr(0, OpCode::Sub), instr(1, OpCode::Ret)];
    let cfg = CfgBuilder::new(&instructions).build();
    let context = MethodContext {
        argument_names: vec!["left".to_string(), "right".to_string()],
        arguments_on_entry_stack: true,
        returns_value: Some(true),
        ..MethodContext::default()
    };

    let ssa = SsaBuilder::new(&cfg, &instructions)
        .with_method_context(&context)
        .build();
    let block = ssa.block(BlockId(0)).expect("entry block");

    assert!(matches!(
        block.stmts.first(),
        Some(SsaStmt::Assign {
            value: SsaExpr::Binary { left, right, .. },
            ..
        }) if matches!(left.as_ref(), SsaExpr::Variable(value) if value.base == "arg1")
            && matches!(right.as_ref(), SsaExpr::Variable(value) if value.base == "arg0")
    ));
}

#[test]
fn entry_loop_keeps_inferred_arguments_as_incoming_stack_values() {
    let instructions = vec![instr(0, OpCode::Drop)];
    let mut cfg = Cfg::new();
    cfg.add_block(BasicBlock::new(
        BlockId(0),
        0,
        1,
        0..1,
        Terminator::Jump { target: BlockId(0) },
    ));
    cfg.add_edge(BlockId(0), BlockId(0), EdgeKind::Unconditional);
    let context = MethodContext {
        argument_names: vec!["value".to_string()],
        arguments_on_entry_stack: true,
        ..MethodContext::default()
    };

    let ssa = SsaBuilder::new(&cfg, &instructions)
        .with_method_context(&context)
        .build();
    let block = ssa.block(BlockId(0)).expect("entry loop block");

    assert!(block.phi_nodes.iter().any(|phi| {
        phi.operands
            .values()
            .any(|value| value == &SsaVariable::initial("arg0".to_string()))
    }));
}

#[test]
fn store_local_emits_a_slot_assignment() {
    // PUSH10 ; STLOC0 ; RET  →  the store must define a loc0 SSA var.
    let ins = vec![
        Instruction::new(0, OpCode::Push10, None),
        Instruction::new(1, OpCode::Stloc0, None),
        Instruction::new(2, OpCode::Ret, None),
    ];
    let (ins, cfg) = linear(ins);
    let ssa = SsaBuilder::new(&cfg, &ins).build();
    let block = ssa.blocks_iter().next().expect("a block exists").1;

    let has_loc0_assign = block.stmts.iter().any(|s| match s {
        SsaStmt::Assign { target, .. } => target.base == "loc0",
        _ => false,
    });
    assert!(
        has_loc0_assign,
        "STLOC0 should define a loc0 SSA variable; got {:?}",
        block.stmts
    );
}

#[test]
fn store_then_load_connects_within_a_block() {
    // PUSH10 ; STLOC0 ; LDLOC0 ; RET
    //   store defines a loc0 var; the load must read that var, not stay opaque.
    let ins = vec![
        Instruction::new(0, OpCode::Push10, None),
        Instruction::new(1, OpCode::Stloc0, None),
        Instruction::new(2, OpCode::Ldloc0, None),
        Instruction::new(3, OpCode::Ret, None),
    ];
    let (ins, cfg) = linear(ins);
    let ssa = SsaBuilder::new(&cfg, &ins).build();
    let block = ssa.blocks_iter().next().expect("a block exists").1;

    // loc0 defs in order: [store, load].
    let loc0_defs: Vec<&SsaStmt> = block
        .stmts
        .iter()
        .filter(|s| matches!(s, SsaStmt::Assign { target, .. } if target.base == "loc0"))
        .collect();
    assert!(
        loc0_defs.len() >= 2,
        "expected a store def and a load def for loc0; got {:?}",
        block.stmts
    );
    // The last loc0 def is the load: it must reference the stored var, NOT
    // be an opaque ldloc0() Call.
    let load_def = loc0_defs.last().copied().unwrap();
    let SsaStmt::Assign { value, .. } = load_def else {
        panic!("load def should be an Assign: {load_def:?}");
    };
    assert!(
        matches!(value, SsaExpr::Variable(_)),
        "LDLOC0 after STLOC0 should read the stored var; got {value:?}"
    );
    assert!(
        !matches!(value, SsaExpr::Call { .. }),
        "LDLOC0 should not stay an opaque ldloc0() call once a store exists; got {value:?}"
    );
}

#[test]
fn diamond_places_a_phi_at_the_merge() {
    // Build a diamond by hand so we control predecessor exit stacks:
    //   BB0 (entry) pushes 1, branches to BB1 / BB2
    //   BB1 pushes 10  -> jmp BB3
    //   BB2 pushes 20  -> jmp BB3
    //   BB3 (merge): the incoming slot should be a φ(BB1: 10, BB2: 20).
    let ins = vec![
        Instruction::new(0, OpCode::Push1, None), // BB0: push 1 (condition-ish)
        Instruction::new(0, OpCode::Pushint8, Some(Operand::I8(10))), // BB1
        Instruction::new(0, OpCode::Pushint8, Some(Operand::I8(20))), // BB2
        Instruction::new(0, OpCode::Ret, None),   // BB3
    ];

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

    let ssa = SsaBuilder::new(&cfg, &ins).build();
    let merge = ssa.block(BlockId(3)).expect("merge block exists");
    assert!(
        merge.phi_count() >= 1,
        "merge block should have a phi node for the incoming value slot"
    );
    let phi = &merge.phi_nodes[0];
    assert_eq!(
        phi.operands.len(),
        2,
        "phi should have one operand per predecessor"
    );
}

#[test]
fn diamond_places_a_phi_for_a_slot() {
    // Two arms store different values to the same local; the merge loads it
    // and so needs a slot φ(loc0) over BB1 / BB2.
    //   BB0: Push1, STLOC0, Branch -> BB1 / BB2
    //   BB1: PUSH11, STLOC0 -> jmp BB3
    //   BB2: PUSH12, STLOC0 -> jmp BB3
    //   BB3: LDLOC0, RET
    let ins = vec![
        Instruction::new(0, OpCode::Push1, None),
        Instruction::new(1, OpCode::Stloc0, None),
        Instruction::new(0, OpCode::Push11, None),
        Instruction::new(1, OpCode::Stloc0, None),
        Instruction::new(0, OpCode::Push12, None),
        Instruction::new(1, OpCode::Stloc0, None),
        Instruction::new(0, OpCode::Ldloc0, None),
        Instruction::new(0, OpCode::Ret, None),
    ];

    let mut cfg = Cfg::new();
    cfg.add_block(BasicBlock::new(
        BlockId(0),
        0,
        1,
        0..2,
        Terminator::Branch {
            then_target: BlockId(1),
            else_target: BlockId(2),
        },
    ));
    cfg.add_block(BasicBlock::new(
        BlockId(1),
        1,
        2,
        2..4,
        Terminator::Jump { target: BlockId(3) },
    ));
    cfg.add_block(BasicBlock::new(
        BlockId(2),
        2,
        3,
        4..6,
        Terminator::Jump { target: BlockId(3) },
    ));
    cfg.add_block(BasicBlock::new(BlockId(3), 3, 4, 6..8, Terminator::Return));
    cfg.add_edge(BlockId(0), BlockId(1), EdgeKind::ConditionalTrue);
    cfg.add_edge(BlockId(0), BlockId(2), EdgeKind::ConditionalFalse);
    cfg.add_edge(BlockId(1), BlockId(3), EdgeKind::Unconditional);
    cfg.add_edge(BlockId(2), BlockId(3), EdgeKind::Unconditional);

    let ssa = SsaBuilder::new(&cfg, &ins).build();
    let merge = ssa.block(BlockId(3)).expect("merge block exists");

    let has_slot_phi = merge.phi_nodes.iter().any(|phi| phi.target.base == "loc0");
    assert!(
        has_slot_phi,
        "merge of two STLOC0 arms should place a loc0 φ; got {:?}",
        merge.phi_nodes
    );
}

#[test]
fn partially_initialized_slot_merge_is_incomplete() {
    let instructions = vec![
        instr(0, OpCode::Push1),
        instr(1, OpCode::Push1),
        instr(2, OpCode::Stloc0),
        instr(3, OpCode::Nop),
        instr(4, OpCode::Ldloc0),
        instr(5, OpCode::Ret),
    ];
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
        3,
        1..3,
        Terminator::Jump { target: BlockId(3) },
    ));
    cfg.add_block(BasicBlock::new(
        BlockId(2),
        3,
        4,
        3..4,
        Terminator::Jump { target: BlockId(3) },
    ));
    cfg.add_block(BasicBlock::new(BlockId(3), 4, 6, 4..6, Terminator::Return));
    cfg.add_edge(BlockId(0), BlockId(1), EdgeKind::ConditionalTrue);
    cfg.add_edge(BlockId(0), BlockId(2), EdgeKind::ConditionalFalse);
    cfg.add_edge(BlockId(1), BlockId(3), EdgeKind::Unconditional);
    cfg.add_edge(BlockId(2), BlockId(3), EdgeKind::Unconditional);
    let context = MethodContext {
        returns_value: Some(true),
        ..MethodContext::default()
    };

    let built = SsaBuilder::new(&cfg, &instructions)
        .with_method_context(&context)
        .build_with_report();

    assert!(built.fidelity.issues.iter().any(|issue| {
        issue.offset == 4
            && issue.opcode == OpCode::Ldloc0
            && issue.kind == LoweringIssueKind::LostStackValue
            && issue.fidelity == Fidelity::Incomplete
    }));
    let merge = built.ssa.block(BlockId(3)).expect("merge block");
    assert!(merge
        .phi_nodes
        .iter()
        .any(|phi| { phi.target.base == "loc0" && phi.operands.values().any(is_unknown) }));
}

#[test]
fn initslot_seeds_locals_with_null_before_partial_assignment() {
    let instructions = vec![
        Instruction::new(0, OpCode::Initslot, Some(Operand::Bytes(vec![1, 0]))),
        instr(3, OpCode::PushT),
        instr(4, OpCode::Push1),
        instr(5, OpCode::Stloc0),
        instr(6, OpCode::Nop),
        instr(7, OpCode::Ldloc0),
        instr(8, OpCode::Ret),
    ];
    let mut cfg = Cfg::new();
    cfg.add_block(BasicBlock::new(
        BlockId(0),
        0,
        4,
        0..2,
        Terminator::Branch {
            then_target: BlockId(1),
            else_target: BlockId(2),
        },
    ));
    cfg.add_block(BasicBlock::new(
        BlockId(1),
        4,
        6,
        2..4,
        Terminator::Jump { target: BlockId(3) },
    ));
    cfg.add_block(BasicBlock::new(
        BlockId(2),
        6,
        7,
        4..5,
        Terminator::Jump { target: BlockId(3) },
    ));
    cfg.add_block(BasicBlock::new(BlockId(3), 7, 9, 5..7, Terminator::Return));
    cfg.add_edge(BlockId(0), BlockId(1), EdgeKind::ConditionalTrue);
    cfg.add_edge(BlockId(0), BlockId(2), EdgeKind::ConditionalFalse);
    cfg.add_edge(BlockId(1), BlockId(3), EdgeKind::Unconditional);
    cfg.add_edge(BlockId(2), BlockId(3), EdgeKind::Unconditional);
    let context = MethodContext {
        returns_value: Some(true),
        ..MethodContext::default()
    };

    let built = SsaBuilder::new(&cfg, &instructions)
        .with_method_context(&context)
        .build_with_report();

    assert!(!built.fidelity.issues.iter().any(|issue| {
        issue.offset == 7
            && issue.opcode == OpCode::Ldloc0
            && issue.kind == LoweringIssueKind::LostStackValue
    }));
    assert!(built.ssa.block(BlockId(0)).is_some_and(|block| {
        block.stmts.iter().all(|statement| {
            !matches!(
                statement,
                SsaStmt::Assign {
                    target,
                    value: SsaExpr::Literal(Literal::Null),
                } if target.base == "loc0"
            )
        })
    }));
    let merge = built.ssa.block(BlockId(3)).expect("merge block");
    assert!(merge.phi_nodes.iter().any(|phi| {
        phi.target.base == "loc0"
            && phi.operands.len() == 2
            && phi.operands.values().all(|operand| !is_unknown(operand))
            && phi.operands.values().any(SsaVariable::is_vm_null)
    }));
}

#[test]
fn first_static_load_establishes_snapshot_for_non_writing_branch() {
    let instructions = vec![
        instr(0, OpCode::Ldsfld0),
        instr(1, OpCode::PushNull),
        instr(2, OpCode::Stsfld0),
        instr(3, OpCode::Nop),
        instr(4, OpCode::Ldsfld0),
        instr(5, OpCode::Ret),
    ];
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
        3,
        1..3,
        Terminator::Jump { target: BlockId(3) },
    ));
    cfg.add_block(BasicBlock::new(
        BlockId(2),
        3,
        4,
        3..4,
        Terminator::Jump { target: BlockId(3) },
    ));
    cfg.add_block(BasicBlock::new(BlockId(3), 4, 6, 4..6, Terminator::Return));
    cfg.add_edge(BlockId(0), BlockId(1), EdgeKind::ConditionalTrue);
    cfg.add_edge(BlockId(0), BlockId(2), EdgeKind::ConditionalFalse);
    cfg.add_edge(BlockId(1), BlockId(3), EdgeKind::Unconditional);
    cfg.add_edge(BlockId(2), BlockId(3), EdgeKind::Unconditional);
    let context = MethodContext {
        returns_value: Some(true),
        ..MethodContext::default()
    };

    let built = SsaBuilder::new(&cfg, &instructions)
        .with_method_context(&context)
        .build_with_report();
    let merge = built.ssa.block(BlockId(3)).expect("merge block");

    assert!(merge.phi_nodes.iter().any(|phi| {
        phi.target.base == "static0"
            && phi.operands.len() == 2
            && phi.operands.values().all(|operand| !is_unknown(operand))
    }));
    assert!(!built.fidelity.issues.iter().any(|issue| {
        issue.offset == 4
            && issue.opcode == OpCode::Ldsfld0
            && issue.kind == LoweringIssueKind::LostStackValue
    }));
}

#[test]
fn loop_phi_uses_ambient_static_value_on_preheader() {
    let instructions = vec![
        instr(0, OpCode::Nop),
        instr(1, OpCode::Nop),
        instr(2, OpCode::Push1),
        instr(3, OpCode::Stsfld0),
        instr(4, OpCode::Ldsfld0),
        instr(5, OpCode::Ret),
    ];
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
        4,
        2..4,
        Terminator::Jump { target: header },
    ));
    cfg.add_block(BasicBlock::new(exit, 4, 6, 4..6, Terminator::Return));
    cfg.add_edge(preheader, header, EdgeKind::Unconditional);
    cfg.add_edge(header, body, EdgeKind::ConditionalTrue);
    cfg.add_edge(header, exit, EdgeKind::ConditionalFalse);
    cfg.add_edge(body, header, EdgeKind::Unconditional);
    let context = MethodContext {
        returns_value: Some(true),
        ..MethodContext::default()
    };

    let built = SsaBuilder::new(&cfg, &instructions)
        .with_method_context(&context)
        .build_with_report();
    let header = built.ssa.block(header).expect("loop header");
    let phi = header
        .phi_nodes
        .iter()
        .find(|phi| phi.target.base == "static0")
        .expect("loop-carried static phi");

    assert_eq!(
        phi.operands.get(&preheader),
        Some(&SsaVariable::initial("static0".to_string()))
    );
    assert!(phi.operands.values().all(|operand| !is_unknown(operand)));
}

#[test]
fn exception_edges_supply_their_payload_at_mixed_joins() {
    let instructions = vec![
        instr(0, OpCode::Push9),
        instr(1, OpCode::Throw),
        instr(2, OpCode::Stloc0),
        instr(3, OpCode::Ret),
    ];
    let normal = BlockId(0);
    let exceptional = BlockId(1);
    let handler = BlockId(2);
    let mut cfg = Cfg::new();
    cfg.add_block(BasicBlock::new(
        normal,
        0,
        1,
        0..1,
        Terminator::Jump { target: handler },
    ));
    cfg.add_block(BasicBlock::new(exceptional, 1, 2, 1..2, Terminator::Throw));
    cfg.add_block(BasicBlock::new(handler, 2, 4, 2..4, Terminator::Return));
    cfg.add_edge(normal, handler, EdgeKind::Unconditional);
    cfg.add_edge(exceptional, handler, EdgeKind::Exception);

    let ssa = SsaBuilder::new(&cfg, &instructions).build();
    let handler_block = ssa.block(handler).expect("handler block");

    assert!(handler_block.phi_nodes.iter().any(|phi| {
        phi.operands
            .values()
            .any(|operand| operand == &SsaVariable::exception_payload(handler))
    }));
}

#[test]
fn exceptional_finally_entry_does_not_taint_normal_return_stack() {
    let instructions = vec![
        Instruction::new(0, OpCode::Try, Some(Operand::Bytes(vec![0, 5]))),
        instr(3, OpCode::Push1),
        Instruction::new(4, OpCode::Endtry, Some(Operand::Jump(3))),
        instr(5, OpCode::Endfinally),
        instr(7, OpCode::Ret),
    ];
    let cfg = CfgBuilder::new(&instructions).build();
    let context = MethodContext {
        returns_value: Some(true),
        ..MethodContext::default()
    };

    let built = SsaBuilder::new(&cfg, &instructions)
        .with_method_context(&context)
        .build_with_report();

    assert!(!built.fidelity.issues.iter().any(|issue| {
        issue.offset == 7
            && issue.opcode == OpCode::Ret
            && issue.kind == LoweringIssueKind::LostStackValue
    }));
    let return_block = cfg.block_at_offset(7).expect("return block").id;
    assert!(matches!(
        built
            .ssa
            .block(return_block)
            .and_then(|block| block.stmts.last()),
        Some(SsaStmt::Return(Some(SsaExpr::Variable(value)))) if value.base != "?"
    ));
}

#[test]
fn known_non_returning_call_does_not_produce_a_stack_value() {
    let instructions = vec![
        Instruction::new(0, OpCode::Call, Some(Operand::Jump(3))),
        instr(2, OpCode::Ret),
        instr(3, OpCode::Abort),
    ];
    let cfg = CfgBuilder::new(&instructions)
        .with_non_returning_calls([0])
        .build();
    let context = MethodContext {
        calls_by_offset: BTreeMap::from([(
            0,
            CallContract::new(
                SemanticCallTarget::Internal {
                    offset: 3,
                    name: "abort_leaf".to_string(),
                },
                0,
                true,
            )
            .with_may_return(false),
        )]),
        ..MethodContext::default()
    };

    let ssa = SsaBuilder::new(&cfg, &instructions)
        .with_method_context(&context)
        .build();
    let entry = ssa.block(BlockId::ENTRY).expect("entry block");

    assert!(entry.stmts.iter().any(|stmt| matches!(
        stmt,
        SsaStmt::Expr(SsaExpr::Call { target, .. })
            if matches!(target, SemanticCallTarget::Internal { offset: 3, .. })
    )));
    assert!(!entry.stmts.iter().any(|stmt| matches!(
        stmt,
        SsaStmt::Assign {
            value: SsaExpr::Call { .. },
            ..
        }
    )));
}

#[test]
fn dup_conditional_join_reuses_a_shorter_prefix_top_value() {
    let instructions = vec![
        instr(0, OpCode::Nop),
        instr(1, OpCode::Nop),
        instr(2, OpCode::Nop),
        instr(3, OpCode::Dup),
        instr(4, OpCode::Ldloc0),
        Instruction::new(5, OpCode::Jmpifnot, Some(Operand::Jump(0))),
    ];
    let preheader = BlockId(0);
    let short_path = BlockId(1);
    let long_path = BlockId(2);
    let merge = BlockId(3);
    let mut cfg = Cfg::new();
    cfg.add_block(BasicBlock::new(
        preheader,
        0,
        1,
        0..1,
        Terminator::Branch {
            then_target: short_path,
            else_target: long_path,
        },
    ));
    cfg.add_block(BasicBlock::new(
        short_path,
        1,
        2,
        1..2,
        Terminator::Jump { target: merge },
    ));
    cfg.add_block(BasicBlock::new(
        long_path,
        2,
        3,
        2..3,
        Terminator::Jump { target: merge },
    ));
    cfg.add_block(BasicBlock::new(merge, 3, 6, 3..6, Terminator::Return));
    cfg.add_edge(preheader, short_path, EdgeKind::ConditionalTrue);
    cfg.add_edge(preheader, long_path, EdgeKind::ConditionalFalse);
    cfg.add_edge(short_path, merge, EdgeKind::Unconditional);
    cfg.add_edge(long_path, merge, EdgeKind::Unconditional);

    let ambient = SsaVariable::initial("ambient".to_string());
    let argument = SsaVariable::initial("argument".to_string());
    let transformed = SsaVariable::initial("transformed".to_string());
    let exits = BTreeMap::from([
        (short_path, vec![ambient.clone(), argument.clone()]),
        (
            long_path,
            vec![ambient.clone(), argument.clone(), transformed.clone()],
        ),
    ]);
    let builder = SsaBuilder::new(&cfg, &instructions);
    let (entry, phis) = builder.compute_join_entry(merge, &exits);

    assert_eq!(entry.len(), 3);
    assert_eq!(entry[0], ambient);
    assert_eq!(entry[1], argument.clone());
    let phi = phis
        .iter()
        .find(|phi| phi.target == entry[2])
        .expect("conditional merge should place a top-value phi");
    assert_eq!(phi.operands.get(&short_path), Some(&argument));
    assert_eq!(phi.operands.get(&long_path), Some(&transformed));
}

#[test]
fn entry_loop_slot_without_virtual_initial_value_is_incomplete() {
    let instructions = vec![
        instr(0, OpCode::Ldloc0),
        instr(1, OpCode::Drop),
        instr(2, OpCode::Push1),
        instr(3, OpCode::Stloc0),
    ];
    let mut cfg = Cfg::new();
    cfg.add_block(BasicBlock::new(
        BlockId(0),
        0,
        4,
        0..4,
        Terminator::Jump { target: BlockId(0) },
    ));
    cfg.add_edge(BlockId(0), BlockId(0), EdgeKind::Unconditional);

    let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

    assert!(built.fidelity.issues.iter().any(|issue| {
        issue.offset == 0
            && issue.opcode == OpCode::Ldloc0
            && issue.kind == LoweringIssueKind::LostStackValue
            && issue.fidelity == Fidelity::Incomplete
    }));
    let entry = built.ssa.block(BlockId(0)).expect("entry loop block");
    assert!(entry
        .phi_nodes
        .iter()
        .any(|phi| { phi.target.base == "loc0" && phi.operands.values().any(is_unknown) }));
}
