use super::*;

#[test]
fn reported_build_marks_clean_method_exact() {
    let instructions = vec![instr(0, OpCode::Push1), instr(1, OpCode::Ret)];
    let cfg = CfgBuilder::new(&instructions).build();

    let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

    assert_eq!(built.fidelity.status, Fidelity::Exact);
    assert!(built.fidelity.issues.is_empty());
    assert_eq!(built.fidelity.covered_offsets, BTreeSet::from([0, 1]));
    assert_eq!(built.fidelity.instruction_count, 2);
}

#[test]
fn reported_build_marks_literal_pack_exact() {
    let instructions = vec![
        instr(0, OpCode::PushF),
        instr(1, OpCode::Assert),
        instr(2, OpCode::Push1),
        instr(3, OpCode::Push1),
        instr(4, OpCode::Push2),
        instr(5, OpCode::Pack),
        instr(6, OpCode::Drop),
        instr(7, OpCode::Push1),
        instr(8, OpCode::Ret),
    ];
    let cfg = CfgBuilder::new(&instructions).build();

    let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

    assert_eq!(built.fidelity.status, Fidelity::Exact);
    assert_eq!(built.fidelity.instruction_count, instructions.len());
    assert_eq!(
        built.fidelity.covered_offsets,
        BTreeSet::from([0, 1, 2, 3, 4, 5, 6, 7, 8])
    );
    assert!(built.fidelity.issues.is_empty());
}

#[test]
fn literal_dynamic_stack_operations_apply_exact_vm_order() {
    let cases = [
        (
            OpCode::Pick,
            vec![
                instr(0, OpCode::Push1),
                instr(1, OpCode::Push2),
                instr(2, OpCode::Push3),
                instr(3, OpCode::Push1),
                instr(4, OpCode::Pick),
                instr(5, OpCode::Ret),
            ],
            2,
        ),
        (
            OpCode::Roll,
            vec![
                instr(0, OpCode::Push1),
                instr(1, OpCode::Push2),
                instr(2, OpCode::Push3),
                instr(3, OpCode::Push2),
                instr(4, OpCode::Roll),
                instr(5, OpCode::Ret),
            ],
            1,
        ),
        (
            OpCode::Xdrop,
            vec![
                instr(0, OpCode::Push1),
                instr(1, OpCode::Push2),
                instr(2, OpCode::Push3),
                instr(3, OpCode::Push1),
                instr(4, OpCode::Xdrop),
                instr(5, OpCode::Drop),
                instr(6, OpCode::Ret),
            ],
            1,
        ),
        (
            OpCode::Reversen,
            vec![
                instr(0, OpCode::Push1),
                instr(1, OpCode::Push2),
                instr(2, OpCode::Push3),
                instr(3, OpCode::Push3),
                instr(4, OpCode::Reversen),
                instr(5, OpCode::Ret),
            ],
            1,
        ),
    ];

    for (opcode, instructions, expected) in cases {
        let cfg = CfgBuilder::new(&instructions).build();
        let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

        assert_eq!(
            built.fidelity.status,
            Fidelity::Exact,
            "{opcode:?}: {:#?}",
            built.fidelity.issues
        );
        assert_eq!(
            optimized_return_expression(&instructions),
            SsaExpr::lit(Literal::Int(expected)),
            "{opcode:?} must preserve Neo's deep-to-top stack order"
        );
    }
}

#[test]
fn literal_pick_creates_a_fresh_ssa_copy() {
    let instructions = vec![
        instr(0, OpCode::Push1),
        instr(1, OpCode::Push2),
        instr(2, OpCode::Push1),
        instr(3, OpCode::Pick),
        instr(4, OpCode::Ret),
    ];
    let cfg = CfgBuilder::new(&instructions).build();
    let ssa = SsaBuilder::new(&cfg, &instructions).build();
    let block = ssa.blocks_iter().next().expect("entry block").1;
    let (copy_target, copy_source) = block
        .stmts
        .iter()
        .find_map(|statement| match statement {
            SsaStmt::Assign {
                target,
                value: SsaExpr::Variable(source),
            } => Some((target, source)),
            _ => None,
        })
        .expect("PICK must emit an SSA copy assignment");

    assert_ne!(copy_target, copy_source);
    assert!(matches!(
        block.stmts.last(),
        Some(SsaStmt::Return(Some(SsaExpr::Variable(returned))))
            if returned == copy_target
    ));
}

#[test]
fn literal_dynamic_stack_operand_resolves_through_an_ssa_copy() {
    let instructions = vec![
        instr(0, OpCode::Push1),
        instr(1, OpCode::Push2),
        instr(2, OpCode::Push1),
        instr(3, OpCode::Dup),
        instr(4, OpCode::Pick),
        instr(5, OpCode::Ret),
    ];
    let cfg = CfgBuilder::new(&instructions).build();

    let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

    assert_eq!(
        built.fidelity.status,
        Fidelity::Exact,
        "{:#?}",
        built.fidelity.issues
    );
    assert_eq!(
        optimized_return_expression(&instructions),
        SsaExpr::lit(Literal::Int(2))
    );
}

#[test]
fn literal_dynamic_stack_operations_accept_zero_and_depth_boundary() {
    let cases = [
        (
            OpCode::Pick,
            0,
            vec![
                instr(0, OpCode::Push1),
                instr(1, OpCode::Push2),
                instr(2, OpCode::Push0),
                instr(3, OpCode::Pick),
                instr(4, OpCode::Ret),
            ],
            2,
        ),
        (
            OpCode::Roll,
            0,
            vec![
                instr(0, OpCode::Push1),
                instr(1, OpCode::Push2),
                instr(2, OpCode::Push0),
                instr(3, OpCode::Roll),
                instr(4, OpCode::Ret),
            ],
            2,
        ),
        (
            OpCode::Xdrop,
            0,
            vec![
                instr(0, OpCode::Push1),
                instr(1, OpCode::Push2),
                instr(2, OpCode::Push0),
                instr(3, OpCode::Xdrop),
                instr(4, OpCode::Ret),
            ],
            1,
        ),
        (
            OpCode::Reversen,
            0,
            vec![
                instr(0, OpCode::Push1),
                instr(1, OpCode::Push2),
                instr(2, OpCode::Push0),
                instr(3, OpCode::Reversen),
                instr(4, OpCode::Ret),
            ],
            2,
        ),
        (
            OpCode::Pick,
            1,
            vec![
                instr(0, OpCode::Push1),
                instr(1, OpCode::Push2),
                instr(2, OpCode::Push1),
                instr(3, OpCode::Pick),
                instr(4, OpCode::Ret),
            ],
            1,
        ),
        (
            OpCode::Roll,
            1,
            vec![
                instr(0, OpCode::Push1),
                instr(1, OpCode::Push2),
                instr(2, OpCode::Push1),
                instr(3, OpCode::Roll),
                instr(4, OpCode::Ret),
            ],
            1,
        ),
        (
            OpCode::Xdrop,
            1,
            vec![
                instr(0, OpCode::Push1),
                instr(1, OpCode::Push2),
                instr(2, OpCode::Push1),
                instr(3, OpCode::Xdrop),
                instr(4, OpCode::Ret),
            ],
            2,
        ),
        (
            OpCode::Reversen,
            2,
            vec![
                instr(0, OpCode::Push1),
                instr(1, OpCode::Push2),
                instr(2, OpCode::Push2),
                instr(3, OpCode::Reversen),
                instr(4, OpCode::Ret),
            ],
            1,
        ),
    ];

    for (opcode, operand, instructions, expected) in cases {
        let cfg = CfgBuilder::new(&instructions).build();
        let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

        assert_eq!(
            built.fidelity.status,
            Fidelity::Exact,
            "{opcode:?}({operand}): {:#?}",
            built.fidelity.issues
        );
        assert_eq!(
            optimized_return_expression(&instructions),
            SsaExpr::lit(Literal::Int(expected)),
            "{opcode:?}({operand})"
        );
    }
}

#[test]
fn literal_dynamic_stack_operations_reject_positions_beyond_depth() {
    for opcode in [OpCode::Pick, OpCode::Roll, OpCode::Xdrop, OpCode::Reversen] {
        let count = if opcode == OpCode::Reversen {
            OpCode::Push2
        } else {
            OpCode::Push1
        };
        let instructions = vec![
            instr(0, OpCode::Push1),
            instr(1, count),
            instr(2, opcode),
            instr(3, OpCode::Ret),
        ];
        let cfg = CfgBuilder::new(&instructions).build();

        let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

        assert_eq!(built.fidelity.status, Fidelity::Incomplete, "{opcode:?}");
        assert!(
            built.fidelity.issues.iter().any(|issue| {
                issue.offset == 2
                    && issue.opcode == opcode
                    && issue.kind == LoweringIssueKind::LostStackValue
                    && issue.detail.contains("requires 2 stack values")
            }),
            "{opcode:?}: {:#?}",
            built.fidelity.issues
        );
    }
}

#[test]
fn dynamic_stack_literals_must_be_nonnegative_i32_integers() {
    for opcode in [OpCode::Pick, OpCode::Roll, OpCode::Xdrop, OpCode::Reversen] {
        let invalid_cases = [
            (
                "negative",
                vec![
                    instr(0, OpCode::Push1),
                    instr(1, OpCode::PushM1),
                    instr(2, opcode),
                    instr(3, OpCode::Ret),
                ],
                2,
            ),
            (
                "non-integer",
                vec![
                    instr(0, OpCode::Push1),
                    instr(1, OpCode::PushT),
                    instr(2, opcode),
                    instr(3, OpCode::Ret),
                ],
                2,
            ),
            (
                "pointer",
                vec![
                    instr(0, OpCode::Push1),
                    Instruction::new(1, OpCode::PushA, Some(Operand::I32(0))),
                    instr(6, opcode),
                    instr(7, OpCode::Ret),
                ],
                6,
            ),
            (
                "larger than i32::MAX",
                vec![
                    instr(0, OpCode::Push1),
                    Instruction::new(
                        1,
                        OpCode::Pushint64,
                        Some(Operand::I64(i64::from(i32::MAX) + 1)),
                    ),
                    instr(10, opcode),
                    instr(11, OpCode::Ret),
                ],
                10,
            ),
        ];

        for (label, instructions, operation_offset) in invalid_cases {
            let cfg = CfgBuilder::new(&instructions).build();
            let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

            assert_eq!(
                built.fidelity.status,
                Fidelity::Incomplete,
                "{opcode:?} {label}"
            );
            assert!(
                built.fidelity.issues.iter().any(|issue| {
                    issue.offset == operation_offset
                        && issue.opcode == opcode
                        && issue.kind == LoweringIssueKind::MissingProvenance
                        && issue.detail.contains("32-bit integer literal")
                }),
                "{opcode:?} {label}: {:#?}",
                built.fidelity.issues
            );
        }
    }
}

#[test]
fn dynamic_stack_i32_max_resolves_before_stack_bounds_check() {
    for opcode in [OpCode::Pick, OpCode::Roll, OpCode::Xdrop, OpCode::Reversen] {
        let instructions = vec![
            instr(0, OpCode::Push1),
            Instruction::new(
                1,
                OpCode::Pushint64,
                Some(Operand::I64(i64::from(i32::MAX))),
            ),
            instr(10, opcode),
            instr(11, OpCode::Ret),
        ];
        let cfg = CfgBuilder::new(&instructions).build();

        let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

        assert!(
            built.fidelity.issues.iter().any(|issue| {
                issue.offset == 10
                    && issue.opcode == opcode
                    && issue.kind == LoweringIssueKind::LostStackValue
            }),
            "{opcode:?}: {:#?}",
            built.fidelity.issues
        );
        assert!(
            !built.fidelity.issues.iter().any(|issue| {
                issue.offset == 10
                    && issue.opcode == opcode
                    && issue.kind == LoweringIssueKind::MissingProvenance
            }),
            "{opcode:?}: {:#?}",
            built.fidelity.issues
        );
    }
}

#[test]
fn runtime_variable_dynamic_stack_operands_remain_incomplete() {
    for opcode in [OpCode::Pick, OpCode::Roll, OpCode::Xdrop, OpCode::Reversen] {
        let instructions = vec![
            instr(0, OpCode::Push1),
            instr(1, OpCode::Ldarg0),
            instr(2, opcode),
            instr(3, OpCode::Ret),
        ];
        let cfg = CfgBuilder::new(&instructions).build();
        let context = MethodContext {
            argument_names: vec!["index".to_string()],
            ..MethodContext::default()
        };

        let built = SsaBuilder::new(&cfg, &instructions)
            .with_method_context(&context)
            .build_with_report();

        assert_eq!(built.fidelity.status, Fidelity::Incomplete, "{opcode:?}");
        assert!(
            built.fidelity.issues.iter().any(|issue| {
                issue.offset == 2
                    && issue.opcode == opcode
                    && issue.kind == LoweringIssueKind::MissingProvenance
                    && issue.detail.contains("32-bit integer literal")
            }),
            "{opcode:?}: {:#?}",
            built.fidelity.issues
        );
    }
}

#[test]
fn literal_dynamic_stack_operations_report_unknown_selected_values() {
    for opcode in [OpCode::Pick, OpCode::Roll, OpCode::Xdrop, OpCode::Reversen] {
        let operand = if opcode == OpCode::Reversen {
            OpCode::Push1
        } else {
            OpCode::Push0
        };
        let (instructions, cfg) = uneven_stack_merge(vec![
            instr(3, operand),
            instr(4, opcode),
            instr(5, OpCode::Clear),
            instr(6, OpCode::Ret),
        ]);
        let context = MethodContext {
            returns_value: Some(false),
            ..MethodContext::default()
        };

        let built = SsaBuilder::new(&cfg, &instructions)
            .with_method_context(&context)
            .build_with_report();

        assert!(
            built.fidelity.issues.iter().any(|issue| {
                issue.offset == 4
                    && issue.opcode == opcode
                    && issue.kind == LoweringIssueKind::LostStackValue
                    && issue.detail.contains("unknown stack value")
            }),
            "{opcode:?}: {:#?}",
            built.fidelity.issues
        );
    }
}

#[test]
fn reported_build_keeps_dynamic_pack_incomplete() {
    let instructions = vec![
        instr(0, OpCode::Ldarg0),
        instr(1, OpCode::Pack),
        instr(2, OpCode::Drop),
        instr(3, OpCode::Ret),
    ];
    let cfg = CfgBuilder::new(&instructions).build();
    let context = MethodContext {
        argument_names: vec!["count".to_string()],
        returns_value: Some(false),
        ..MethodContext::default()
    };

    let built = SsaBuilder::new(&cfg, &instructions)
        .with_method_context(&context)
        .build_with_report();

    assert_eq!(built.fidelity.status, Fidelity::Incomplete);
    assert!(built.fidelity.issues.iter().any(|issue| {
        issue.offset == 1
            && issue.opcode == OpCode::Pack
            && issue.kind == LoweringIssueKind::MissingProvenance
            && issue.fidelity == Fidelity::Incomplete
    }));
}
