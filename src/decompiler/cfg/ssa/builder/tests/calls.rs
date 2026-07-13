use super::*;

#[test]
fn linear_compute_produces_real_binary_expr() {
    // PUSH1, PUSH2, ADD, RET
    let ins = vec![
        instr(0, OpCode::Push1),
        instr(1, OpCode::Push2),
        instr(2, OpCode::Add),
        Instruction::new(3, OpCode::Ret, None),
    ];
    let (ins, cfg) = linear(ins);
    let ssa = SsaBuilder::new(&cfg, &ins).build();

    // At least one block exists; find a block with assignments.
    let (_id, block) = ssa
        .blocks_iter()
        .find(|(_, b)| b.stmt_count() >= 3)
        .expect("a block with >= 3 assignments should exist");

    // v0 = 1, v1 = 2, v2 = (v0 + v2)
    let assignment_count = block
        .stmts
        .iter()
        .filter(|stmt| matches!(stmt, SsaStmt::Assign { .. }))
        .count();
    assert_eq!(assignment_count, 3, "expected 3 push/compute assignments");
    let add = &block.stmts[2];
    let SsaStmt::Assign { value, .. } = add else {
        panic!("third stmt should be the ADD assignment: {add:?}");
    };
    let SsaExpr::Binary { op, left, right } = value else {
        panic!("ADD should lower to a binary expr, got {value:?}");
    };
    assert_eq!(*op, BinOp::Add);
    // Operands reference the two push defs (deep on the left, top right).
    assert!(
        matches!(left.as_ref(), SsaExpr::Variable(_)),
        "left operand"
    );
    assert!(
        matches!(right.as_ref(), SsaExpr::Variable(_)),
        "right operand"
    );
    assert!(matches!(block.stmts.last(), Some(SsaStmt::Return(Some(_)))));
}

#[test]
fn dup_creates_a_copy_definition() {
    // PUSH1, DUP, RET
    let ins = vec![
        instr(0, OpCode::Push1),
        instr(1, OpCode::Dup),
        Instruction::new(2, OpCode::Ret, None),
    ];
    let (ins, cfg) = linear(ins);
    let ssa = SsaBuilder::new(&cfg, &ins).build();
    let block = ssa.blocks_iter().next().expect("a block exists").1;

    // Two assignments: v0 = 1, v1 = v0 (the DUP copy).
    let assignment_count = block
        .stmts
        .iter()
        .filter(|stmt| matches!(stmt, SsaStmt::Assign { .. }))
        .count();
    assert_eq!(assignment_count, 2);
    let copy = &block.stmts[1];
    let SsaStmt::Assign { value, .. } = copy else {
        panic!("DUP should produce an assignment: {copy:?}");
    };
    assert!(
        matches!(value, SsaExpr::Variable(_)),
        "DUP copy should reference its source var, got {value:?}"
    );
    assert!(matches!(block.stmts.last(), Some(SsaStmt::Return(Some(_)))));
}

#[test]
fn call_results_replace_pre_call_stack_values_at_ret() {
    let cases = [
        (
            "call_0x0005",
            vec![
                instr(0, OpCode::Push1),
                Instruction::new(1, OpCode::Call, Some(Operand::Jump(4))),
                instr(3, OpCode::Ret),
            ],
        ),
        (
            "callt_0x0002",
            vec![
                instr(0, OpCode::Push1),
                Instruction::new(1, OpCode::CallT, Some(Operand::U16(2))),
                instr(4, OpCode::Ret),
            ],
        ),
        (
            "calla",
            vec![
                Instruction::new(0, OpCode::PushA, Some(Operand::I32(6))),
                instr(5, OpCode::CallA),
                instr(6, OpCode::Ret),
            ],
        ),
    ];

    for (expected_name, instructions) in cases {
        let cfg = CfgBuilder::new(&instructions).build();
        let ssa = SsaBuilder::new(&cfg, &instructions).build();
        let block = ssa.blocks_iter().next().expect("a block exists").1;
        let Some(SsaStmt::Return(Some(SsaExpr::Variable(returned)))) = block.stmts.last() else {
            panic!("{expected_name} must produce the value consumed by RET: {block:?}");
        };
        assert!(
            block.stmts.iter().any(|stmt| matches!(
                stmt,
                SsaStmt::Assign {
                    target,
                    value: SsaExpr::Call { target: call_target, .. }
                } if target == returned && call_target.display_name() == expected_name
            )),
            "{expected_name} must define RET's value: {block:?}"
        );
    }
}

#[test]
fn dropping_opaque_call_result_does_not_expose_pre_call_values() {
    let cases = [
        vec![
            instr(0, OpCode::Push1),
            Instruction::new(1, OpCode::Call, Some(Operand::Jump(5))),
            instr(3, OpCode::Drop),
            instr(4, OpCode::Ret),
        ],
        vec![
            instr(0, OpCode::Push1),
            Instruction::new(1, OpCode::CallT, Some(Operand::U16(2))),
            instr(4, OpCode::Drop),
            instr(5, OpCode::Ret),
        ],
        vec![
            instr(0, OpCode::Push1),
            Instruction::new(1, OpCode::PushA, Some(Operand::I32(7))),
            instr(6, OpCode::CallA),
            instr(7, OpCode::Drop),
            instr(8, OpCode::Ret),
        ],
    ];

    for instructions in cases {
        let cfg = CfgBuilder::new(&instructions).build();
        let ssa = SsaBuilder::new(&cfg, &instructions).build();
        let block = ssa.blocks_iter().next().expect("a block exists").1;
        assert!(
            matches!(block.stmts.last(), Some(SsaStmt::Return(None))),
            "opaque call arguments must not survive a dropped result: {block:?}"
        );
    }
}

#[test]
fn known_call_contract_preserves_stack_and_uses_source_argument_order() {
    // Ambient value 9 stays below two call arguments. Neo pushes call
    // arguments right-to-left, so popping top-first must render (1, 2).
    let instructions = vec![
        instr(0, OpCode::Push9),
        instr(1, OpCode::Push2),
        instr(2, OpCode::Push1),
        Instruction::new(3, OpCode::Call, Some(Operand::Jump(4))),
        instr(5, OpCode::Drop),
        instr(6, OpCode::Ret),
    ];
    let cfg = CfgBuilder::new(&instructions).build();
    let mut context = MethodContext::default();
    context.calls_by_offset.insert(
        3,
        CallContract::new(
            SemanticCallTarget::Internal {
                offset: 7,
                name: "helper".to_string(),
            },
            2,
            true,
        ),
    );

    let mut ssa = SsaBuilder::new(&cfg, &instructions)
        .with_method_context(&context)
        .build();
    crate::decompiler::cfg::ssa::optimize_ssa(&mut ssa);
    let block = ssa.blocks_iter().next().expect("a block exists").1;

    assert!(
        block.stmts.iter().any(|stmt| matches!(
            stmt,
            SsaStmt::Assign {
                value: SsaExpr::Call { target, args },
                ..
            } if target.display_name() == "helper"
                && args.as_slice() == [
                    SsaExpr::lit(Literal::Int(1)),
                    SsaExpr::lit(Literal::Int(2)),
                ]
        )),
        "known value call must retain its ordered arguments: {block:?}"
    );
    assert!(
        matches!(
            block.stmts.last(),
            Some(SsaStmt::Return(Some(SsaExpr::Literal(Literal::Int(9)))))
        ),
        "dropping a known call result must reveal the preserved ambient value: {block:?}"
    );
}

#[test]
fn known_tail_jump_returns_resolved_call_with_source_argument_order() {
    let instructions = vec![
        instr(0, OpCode::Push2),
        instr(1, OpCode::Push1),
        Instruction::new(2, OpCode::Jmp, Some(Operand::Jump(18))),
    ];
    let mut cfg = Cfg::new();
    cfg.add_block(BasicBlock::new(BlockId(0), 0, 3, 0..3, Terminator::Return));
    let mut context = MethodContext {
        returns_value: Some(true),
        ..MethodContext::default()
    };
    context.calls_by_offset.insert(
        2,
        CallContract::new(
            SemanticCallTarget::Internal {
                offset: 20,
                name: "helper".to_string(),
            },
            2,
            true,
        ),
    );

    let output = SsaBuilder::new(&cfg, &instructions)
        .with_method_context(&context)
        .build_with_report();
    assert_eq!(output.fidelity.status, Fidelity::Exact);
    let mut ssa = output.ssa;
    crate::decompiler::cfg::ssa::optimize_ssa(&mut ssa);
    let block = ssa.blocks_iter().next().expect("a block exists").1;

    assert!(matches!(
        block.stmts.last(),
        Some(SsaStmt::Return(Some(SsaExpr::Call { target, args })))
            if target.display_name() == "helper"
                && args.as_slice() == [
                    SsaExpr::lit(Literal::Int(1)),
                    SsaExpr::lit(Literal::Int(2)),
                ]
    ));
}

#[test]
fn known_call_contract_emits_void_call_without_phantom_result() {
    let instructions = vec![
        instr(0, OpCode::Push9),
        instr(1, OpCode::Push1),
        Instruction::new(2, OpCode::CallT, Some(Operand::U16(0))),
        instr(5, OpCode::Ret),
    ];
    let cfg = CfgBuilder::new(&instructions).build();
    let mut context = MethodContext::default();
    context.calls_by_offset.insert(
        2,
        CallContract::new(
            SemanticCallTarget::MethodToken {
                index: 0,
                name: "notify".to_string(),
                hash_le: None,
                call_flags: None,
            },
            1,
            false,
        ),
    );

    let mut ssa = SsaBuilder::new(&cfg, &instructions)
        .with_method_context(&context)
        .build();
    crate::decompiler::cfg::ssa::optimize_ssa(&mut ssa);
    let block = ssa.blocks_iter().next().expect("a block exists").1;

    assert!(
        block.stmts.iter().any(|stmt| matches!(
            stmt,
            SsaStmt::Expr(SsaExpr::Call { target, args })
                if target.display_name() == "notify"
                    && args.as_slice() == [SsaExpr::lit(Literal::Int(1))]
        )),
        "known void call must survive as a side-effect statement: {block:?}"
    );
    assert!(
        matches!(
            block.stmts.last(),
            Some(SsaStmt::Return(Some(SsaExpr::Literal(Literal::Int(9)))))
        ),
        "known void call must not replace the ambient return value: {block:?}"
    );
}

#[test]
fn known_calla_contract_consumes_pointer_without_rendering_it_as_an_argument() {
    let instructions = vec![
        instr(0, OpCode::Push9),
        instr(1, OpCode::Push1),
        Instruction::new(2, OpCode::PushA, Some(Operand::I32(8))),
        instr(7, OpCode::CallA),
        instr(8, OpCode::Ret),
    ];
    let cfg = CfgBuilder::new(&instructions).build();
    let mut context = MethodContext::default();
    context.calls_by_offset.insert(
        7,
        CallContract::new(
            SemanticCallTarget::Internal {
                offset: 10,
                name: "delegate".to_string(),
            },
            1,
            false,
        ),
    );

    let output = SsaBuilder::new(&cfg, &instructions)
        .with_method_context(&context)
        .build_with_report();
    assert_eq!(output.fidelity.status, Fidelity::Exact);
    let mut ssa = output.ssa;
    crate::decompiler::cfg::ssa::optimize_ssa(&mut ssa);
    let block = ssa.blocks_iter().next().expect("a block exists").1;

    assert!(
        block.stmts.iter().any(|stmt| matches!(
            stmt,
            SsaStmt::Expr(SsaExpr::Call { target, args })
                if target.display_name() == "delegate"
                    && args.as_slice() == [SsaExpr::lit(Literal::Int(1))]
        )),
        "CALLA pointer must be consumed separately from source arguments: {block:?}"
    );
    assert!(
        matches!(
            block.stmts.last(),
            Some(SsaStmt::Return(Some(SsaExpr::Literal(Literal::Int(9)))))
        ),
        "resolved void CALLA must preserve deeper caller stack state: {block:?}"
    );
}

#[test]
fn collection_mutations_emit_ordered_effect_calls() {
    let cases = [
        (
            OpCode::Setitem,
            "set_item",
            vec![OpCode::Push1, OpCode::Push2, OpCode::Push3],
            vec![1, 2, 3],
        ),
        (
            OpCode::Append,
            "append",
            vec![OpCode::Push1, OpCode::Push2],
            vec![1, 2],
        ),
        (
            OpCode::Remove,
            "remove_item",
            vec![OpCode::Push1, OpCode::Push2],
            vec![1, 2],
        ),
        (
            OpCode::Clearitems,
            "clear_items",
            vec![OpCode::Push1],
            vec![1],
        ),
        (
            OpCode::Reverseitems,
            "reverse_items",
            vec![OpCode::Push1],
            vec![1],
        ),
        (
            OpCode::Memcpy,
            "memcpy",
            vec![
                OpCode::Push1,
                OpCode::Push2,
                OpCode::Push3,
                OpCode::Push4,
                OpCode::Push5,
            ],
            vec![1, 2, 3, 4, 5],
        ),
    ];

    for (opcode, expected_name, pushes, expected_values) in cases {
        let mut instructions = vec![instr(0, OpCode::Push9)];
        instructions.extend(
            pushes
                .into_iter()
                .enumerate()
                .map(|(offset, push)| instr(offset + 1, push)),
        );
        instructions.push(instr(instructions.len(), opcode));
        instructions.push(instr(instructions.len(), OpCode::Ret));

        let cfg = CfgBuilder::new(&instructions).build();
        let mut ssa = SsaBuilder::new(&cfg, &instructions).build();
        crate::decompiler::cfg::ssa::optimize_ssa(&mut ssa);
        let block = ssa.blocks_iter().next().expect("a block exists").1;
        let matching_calls: Vec<_> = block
            .stmts
            .iter()
            .filter_map(|stmt| match stmt {
                SsaStmt::Expr(SsaExpr::Call { target, args })
                    if target.display_name() == expected_name =>
                {
                    Some(args)
                }
                _ => None,
            })
            .collect();
        let expected_args: Vec<_> = expected_values
            .into_iter()
            .map(|value| SsaExpr::lit(Literal::Int(value)))
            .collect();

        assert_eq!(
            matching_calls.len(),
            1,
            "{opcode:?} must emit one {expected_name} effect call: {block:?}"
        );
        assert_eq!(
            matching_calls[0], &expected_args,
            "{opcode:?} must preserve deep-to-top operand order"
        );
        assert!(
            matches!(
                block.stmts.last(),
                Some(SsaStmt::Return(Some(SsaExpr::Literal(Literal::Int(9)))))
            ),
            "{opcode:?} must consume exactly its operands and preserve ambient 9: {block:?}"
        );
    }
}

#[test]
fn collection_mutation_underflow_preserves_declared_arity() {
    let cases = [
        (OpCode::Setitem, "set_item", 3, true),
        (OpCode::Append, "append", 2, true),
        (OpCode::Remove, "remove_item", 2, true),
        (OpCode::Clearitems, "clear_items", 1, false),
        (OpCode::Reverseitems, "reverse_items", 1, false),
        (OpCode::Memcpy, "memcpy", 5, true),
    ];

    for (opcode, expected_name, arity, has_available_top) in cases {
        let mut instructions = Vec::new();
        if has_available_top {
            instructions.push(instr(0, OpCode::Push1));
        }
        instructions.push(instr(instructions.len(), opcode));
        instructions.push(instr(instructions.len(), OpCode::Ret));

        let cfg = CfgBuilder::new(&instructions).build();
        let mut ssa = SsaBuilder::new(&cfg, &instructions).build();
        crate::decompiler::cfg::ssa::optimize_ssa(&mut ssa);
        let block = ssa.blocks_iter().next().expect("a block exists").1;
        let args = block.stmts.iter().find_map(|stmt| match stmt {
            SsaStmt::Expr(SsaExpr::Call { target, args })
                if target.display_name() == expected_name =>
            {
                Some(args)
            }
            _ => None,
        });
        let args =
            args.unwrap_or_else(|| panic!("{opcode:?} underflow must remain visible: {block:?}"));

        assert_eq!(args.len(), arity, "{opcode:?} must retain declared arity");
        let unknown_count = args
            .iter()
            .filter(|arg| **arg == SsaExpr::var(unknown_var()))
            .count();
        assert_eq!(
            unknown_count,
            arity - usize::from(has_available_top),
            "{opcode:?} must preserve each missing operand position"
        );
        if has_available_top {
            assert_eq!(
                args.last(),
                Some(&SsaExpr::lit(Literal::Int(1))),
                "{opcode:?} must keep the available top value in the final operand position"
            );
        }
    }
}

#[test]
fn structured_known_syscall_value_uses_catalog_contract() {
    let instructions = vec![
        instr(0, OpCode::Push9),
        instr(1, OpCode::Push1),
        Instruction::new(2, OpCode::Syscall, Some(Operand::Syscall(0x8CEC_27F8))),
        instr(7, OpCode::Drop),
        instr(8, OpCode::Ret),
    ];
    let cfg = CfgBuilder::new(&instructions).build();

    let mut ssa = SsaBuilder::new(&cfg, &instructions).build();
    crate::decompiler::cfg::ssa::optimize_ssa(&mut ssa);
    let block = ssa.blocks_iter().next().expect("a block exists").1;

    assert!(block.stmts.iter().any(|stmt| matches!(
        stmt,
        SsaStmt::Assign {
            value: SsaExpr::Call { target, args },
            ..
        } if matches!(target, SemanticCallTarget::Syscall {
                hash: 0x8CEC_27F8,
                name: Some(name),
            } if name == "System.Runtime.CheckWitness")
            && args.as_slice() == [
                SsaExpr::lit(Literal::String(
                    "System.Runtime.CheckWitness".to_string()
                )),
                SsaExpr::lit(Literal::Int(1)),
            ]
    )));
    assert!(matches!(
        block.stmts.last(),
        Some(SsaStmt::Return(Some(SsaExpr::Literal(Literal::Int(9)))))
    ));
}

#[test]
fn structured_known_syscall_void_preserves_ambient_stack_value() {
    let instructions = vec![
        instr(0, OpCode::Push9),
        instr(1, OpCode::Push1),
        Instruction::new(2, OpCode::Syscall, Some(Operand::Syscall(0x9647_E7CF))),
        instr(7, OpCode::Ret),
    ];
    let cfg = CfgBuilder::new(&instructions).build();

    let mut ssa = SsaBuilder::new(&cfg, &instructions).build();
    crate::decompiler::cfg::ssa::optimize_ssa(&mut ssa);
    let block = ssa.blocks_iter().next().expect("a block exists").1;

    assert!(block.stmts.iter().any(|stmt| matches!(
        stmt,
        SsaStmt::Expr(SsaExpr::Call { target, args })
            if matches!(target, SemanticCallTarget::Syscall {
                    hash: 0x9647_E7CF,
                    name: Some(name),
                } if name == "System.Runtime.Log")
                && args.as_slice() == [
                    SsaExpr::lit(Literal::String("System.Runtime.Log".to_string())),
                    SsaExpr::lit(Literal::Int(1)),
                ]
    )));
    assert!(matches!(
        block.stmts.last(),
        Some(SsaStmt::Return(Some(SsaExpr::Literal(Literal::Int(9)))))
    ));
}

#[test]
fn structured_known_syscall_preserves_declaration_order() {
    let instructions = vec![
        instr(0, OpCode::Push9),
        instr(1, OpCode::Push1),
        instr(2, OpCode::Push2),
        instr(3, OpCode::Push3),
        Instruction::new(4, OpCode::Syscall, Some(Operand::Syscall(0x8418_3FE6))),
        instr(9, OpCode::Ret),
    ];
    let cfg = CfgBuilder::new(&instructions).build();

    let mut ssa = SsaBuilder::new(&cfg, &instructions).build();
    crate::decompiler::cfg::ssa::optimize_ssa(&mut ssa);
    let block = ssa.blocks_iter().next().expect("a block exists").1;

    assert!(block.stmts.iter().any(|stmt| matches!(
        stmt,
        SsaStmt::Expr(SsaExpr::Call { target, args })
            if matches!(target, SemanticCallTarget::Syscall {
                    hash: 0x8418_3FE6,
                    name: Some(name),
                } if name == "System.Storage.Put")
                && args.as_slice() == [
                    SsaExpr::lit(Literal::String("System.Storage.Put".to_string())),
                    SsaExpr::lit(Literal::Int(3)),
                    SsaExpr::lit(Literal::Int(2)),
                    SsaExpr::lit(Literal::Int(1)),
                ]
    )));
    assert!(matches!(
        block.stmts.last(),
        Some(SsaStmt::Return(Some(SsaExpr::Literal(Literal::Int(9)))))
    ));
}

#[test]
fn structured_syscall_fallback_keeps_missing_known_argument_visible() {
    let instructions = vec![
        Instruction::new(0, OpCode::Syscall, Some(Operand::Syscall(0x9647_E7CF))),
        instr(5, OpCode::Ret),
    ];
    let cfg = CfgBuilder::new(&instructions).build();
    let ssa = SsaBuilder::new(&cfg, &instructions).build();
    let block = ssa.blocks_iter().next().expect("a block exists").1;

    assert!(block.stmts.iter().any(|stmt| matches!(
        stmt,
        SsaStmt::Expr(SsaExpr::Call { target, args })
            if matches!(target, SemanticCallTarget::Syscall {
                    hash: 0x9647_E7CF,
                    name: Some(name),
                } if name == "System.Runtime.Log")
                && args.as_slice() == [
                    SsaExpr::lit(Literal::String("System.Runtime.Log".to_string())),
                    SsaExpr::var(unknown_var()),
                ]
    )));
}

#[test]
fn structured_syscall_fallback_unknown_hash_uses_opaque_barrier() {
    let instructions = vec![
        instr(0, OpCode::Push9),
        Instruction::new(1, OpCode::Syscall, Some(Operand::Syscall(0xDEAD_BEEF))),
        instr(6, OpCode::Drop),
        instr(7, OpCode::Ret),
    ];
    let cfg = CfgBuilder::new(&instructions).build();
    let ssa = SsaBuilder::new(&cfg, &instructions).build();
    let block = ssa.blocks_iter().next().expect("a block exists").1;

    assert!(block.stmts.iter().any(|stmt| matches!(
        stmt,
        SsaStmt::Assign {
            value: SsaExpr::Call { target, args },
            ..
        } if matches!(target, SemanticCallTarget::Syscall {
                hash: 0xDEAD_BEEF,
                name: None,
            })
            && args.as_slice() == [SsaExpr::lit(Literal::String(
                "0xDEADBEEF".to_string()
            ))]
    )));
    assert!(matches!(block.stmts.last(), Some(SsaStmt::Return(None))));
}
