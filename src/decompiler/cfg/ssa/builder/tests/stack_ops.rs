use super::*;

#[test]
fn convert_consumes_one_value() {
    let instructions = vec![
        instr(0, OpCode::Push1),
        Instruction::new(1, OpCode::Convert, Some(Operand::U8(0x21))),
        instr(3, OpCode::Ret),
    ];

    let converted = first_nonliteral_assignment(&instructions);

    assert_eq!(collect_expr_uses(&converted).len(), 1, "{converted:?}");
    assert!(
        format!("{converted:?}").contains("Integer"),
        "CONVERT must retain its integer target tag: {converted:?}"
    );
}

#[test]
fn istype_preserves_target_tag() {
    let instructions = vec![
        instr(0, OpCode::Push1),
        Instruction::new(1, OpCode::Istype, Some(Operand::U8(0x21))),
        instr(3, OpCode::Ret),
    ];

    let checked = first_nonliteral_assignment(&instructions);

    assert_eq!(collect_expr_uses(&checked).len(), 1, "{checked:?}");
    assert!(
        format!("{checked:?}").contains("Integer"),
        "ISTYPE must retain its integer target tag: {checked:?}"
    );
}

#[test]
fn convert_and_istype_reject_any_target_tag() {
    for opcode in [OpCode::Convert, OpCode::Istype] {
        let instructions = vec![
            instr(0, OpCode::Push1),
            Instruction::new(1, opcode, Some(Operand::U8(0x00))),
            instr(3, OpCode::Ret),
        ];
        let cfg = CfgBuilder::new(&instructions).build();

        let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

        assert_eq!(built.fidelity.status, Fidelity::Incomplete, "{opcode:?}");
        assert!(
            built.fidelity.issues.iter().any(|issue| {
                issue.offset == 1
                    && issue.opcode == opcode
                    && issue.kind == LoweringIssueKind::MissingOperandMetadata
                    && issue.detail.contains("Any")
            }),
            "{opcode:?}: {:#?}",
            built.fidelity
        );
    }
}

#[test]
fn newarray_t_accepts_any_target_tag() {
    let instructions = vec![
        instr(0, OpCode::Push1),
        Instruction::new(1, OpCode::NewarrayT, Some(Operand::U8(0x00))),
        instr(3, OpCode::Ret),
    ];
    let cfg = CfgBuilder::new(&instructions).build();

    let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

    assert_eq!(
        built.fidelity.status,
        Fidelity::Exact,
        "{:#?}",
        built.fidelity
    );
}

#[test]
fn newarray_t_preserves_element_type() {
    let instructions = vec![
        instr(0, OpCode::Push2),
        Instruction::new(1, OpCode::NewarrayT, Some(Operand::U8(0x21))),
        instr(3, OpCode::Ret),
    ];

    let array = first_nonliteral_assignment(&instructions);

    assert_eq!(collect_expr_uses(&array).len(), 1, "{array:?}");
    assert!(
        format!("{array:?}").contains("Integer"),
        "NEWARRAY_T must retain its integer element tag: {array:?}"
    );
}

#[test]
fn pack_preserves_elements() {
    let instructions = vec![
        instr(0, OpCode::Push2),
        instr(1, OpCode::Push1),
        instr(2, OpCode::Push2),
        instr(3, OpCode::Pack),
        instr(4, OpCode::Ret),
    ];

    assert_eq!(
        optimized_collection_expression(&instructions),
        SsaExpr::Array(vec![
            SsaExpr::lit(Literal::Int(1)),
            SsaExpr::lit(Literal::Int(2)),
        ])
    );
}

#[test]
fn pack_accepts_nonnegative_wide_literal_count() {
    let mut count = vec![0; 16];
    count[0] = 2;
    let instructions = vec![
        instr(0, OpCode::Push2),
        instr(1, OpCode::Push1),
        Instruction::new(2, OpCode::Pushint128, Some(Operand::Bytes(count))),
        instr(19, OpCode::Pack),
        instr(20, OpCode::Ret),
    ];

    assert_eq!(
        optimized_collection_expression(&instructions),
        SsaExpr::Array(vec![
            SsaExpr::lit(Literal::Int(1)),
            SsaExpr::lit(Literal::Int(2)),
        ])
    );
}

#[test]
fn packstruct_preserves_elements() {
    let instructions = vec![
        instr(0, OpCode::Push2),
        instr(1, OpCode::Push1),
        instr(2, OpCode::Push2),
        instr(3, OpCode::Packstruct),
        instr(4, OpCode::Ret),
    ];

    let packed = optimized_collection_expression(&instructions);

    assert_eq!(
        format!("{packed:?}"),
        "Struct([Literal(Int(1)), Literal(Int(2))])"
    );
}

#[test]
fn reports_only_unanimous_unmodified_collection_return_shapes() {
    let fixed = vec![
        instr(0, OpCode::Push2),
        instr(1, OpCode::Push1),
        instr(2, OpCode::Push2),
        instr(3, OpCode::Packstruct),
        instr(4, OpCode::Ret),
    ];
    let fixed_cfg = CfgBuilder::new(&fixed).build();
    assert_eq!(
        SsaBuilder::new(&fixed_cfg, &fixed)
            .build_with_report()
            .return_shape,
        Some(CollectionShape::Struct(2))
    );

    let mixed = vec![
        instr(0, OpCode::Push1),
        Instruction::new(1, OpCode::Jmpif, Some(Operand::Jump(6))),
        instr(3, OpCode::Push1),
        instr(4, OpCode::Push1),
        instr(5, OpCode::Pack),
        instr(6, OpCode::Ret),
        instr(7, OpCode::Push2),
        instr(8, OpCode::Push1),
        instr(9, OpCode::Packstruct),
        instr(10, OpCode::Ret),
    ];
    let mixed_cfg = CfgBuilder::new(&mixed).build();
    assert_eq!(
        SsaBuilder::new(&mixed_cfg, &mixed)
            .build_with_report()
            .return_shape,
        None
    );

    let mutated = vec![
        instr(0, OpCode::Push1),
        instr(1, OpCode::Push1),
        instr(2, OpCode::Pack),
        instr(3, OpCode::Dup),
        instr(4, OpCode::Push2),
        instr(5, OpCode::Append),
        instr(6, OpCode::Ret),
    ];
    let mutated_cfg = CfgBuilder::new(&mutated).build();
    assert_eq!(
        SsaBuilder::new(&mutated_cfg, &mutated)
            .build_with_report()
            .return_shape,
        None
    );
}

#[test]
fn packmap_preserves_pairs_in_source_order() {
    let instructions = vec![
        instr(0, OpCode::Push4),
        instr(1, OpCode::Push3),
        instr(2, OpCode::Push2),
        instr(3, OpCode::Push1),
        instr(4, OpCode::Push2),
        instr(5, OpCode::Packmap),
        instr(6, OpCode::Ret),
    ];

    assert_eq!(
        optimized_collection_expression(&instructions),
        SsaExpr::Map(vec![
            (SsaExpr::lit(Literal::Int(1)), SsaExpr::lit(Literal::Int(2)),),
            (SsaExpr::lit(Literal::Int(3)), SsaExpr::lit(Literal::Int(4)),),
        ])
    );
}

#[test]
fn unpack_constant_pack_pushes_literal_count() {
    let instructions = vec![
        instr(0, OpCode::Push2),
        instr(1, OpCode::Push1),
        instr(2, OpCode::Push2),
        instr(3, OpCode::Pack),
        instr(4, OpCode::Unpack),
        instr(5, OpCode::Ret),
    ];

    assert_eq!(
        optimized_return_expression(&instructions),
        SsaExpr::lit(Literal::Int(2))
    );
}

#[test]
fn unpack_constant_pack_replays_vm_element_order() {
    let instructions = vec![
        instr(0, OpCode::Push2),
        instr(1, OpCode::Push1),
        instr(2, OpCode::Push2),
        instr(3, OpCode::Pack),
        instr(4, OpCode::Unpack),
        instr(5, OpCode::Drop),
        instr(6, OpCode::Sub),
        instr(7, OpCode::Ret),
    ];

    assert_eq!(
        optimized_return_expression(&instructions),
        SsaExpr::lit(Literal::Int(1))
    );
}

#[test]
fn unpack_shaped_call_result_uses_runtime_indexes_once_in_vm_order() {
    let instructions = vec![
        Instruction::new(0, OpCode::Call, Some(Operand::Jump(8))),
        instr(2, OpCode::Unpack),
        instr(3, OpCode::Drop),
        instr(4, OpCode::Stloc0),
        instr(5, OpCode::Stloc1),
        instr(6, OpCode::Ret),
    ];
    let cfg = CfgBuilder::new(&instructions).build();
    let mut context = MethodContext {
        returns_value: Some(false),
        ..MethodContext::default()
    };
    context.calls_by_offset.insert(
        0,
        CallContract::new(
            SemanticCallTarget::Internal {
                offset: 8,
                name: "pair".to_string(),
            },
            0,
            true,
        )
        .with_return_shape(Some(CollectionShape::Struct(2))),
    );

    let built = SsaBuilder::new(&cfg, &instructions)
        .with_method_context(&context)
        .build_with_report();
    assert_eq!(
        built.fidelity.status,
        Fidelity::Exact,
        "{:#?}",
        built.fidelity
    );

    let mut indexes = BTreeMap::new();
    let mut slots = BTreeMap::new();
    let mut calls = 0usize;
    for statement in built.ssa.blocks_iter().flat_map(|(_, block)| &block.stmts) {
        let SsaStmt::Assign { target, value } = statement else {
            continue;
        };
        match value {
            SsaExpr::Call {
                target: SemanticCallTarget::Internal { .. },
                ..
            } => calls += 1,
            SsaExpr::Index { index, .. } => {
                let SsaExpr::Literal(Literal::Int(index)) = index.as_ref() else {
                    panic!("runtime collection index must be an integer literal");
                };
                indexes.insert(target.clone(), *index);
            }
            SsaExpr::Variable(source) if matches!(target.base.as_str(), "loc0" | "loc1") => {
                slots.insert(target.base.clone(), source.clone());
            }
            _ => {}
        }
    }

    assert_eq!(calls, 1, "the shaped producer call must be evaluated once");
    assert_eq!(
        slots
            .iter()
            .map(|(slot, source)| (slot.as_str(), indexes.get(source).copied()))
            .collect::<Vec<_>>(),
        vec![("loc0", Some(0)), ("loc1", Some(1))]
    );
}

#[test]
fn mutation_invalidates_shaped_call_result_before_unpack() {
    let instructions = vec![
        Instruction::new(0, OpCode::Call, Some(Operand::Jump(8))),
        instr(2, OpCode::Dup),
        instr(3, OpCode::Push1),
        instr(4, OpCode::Append),
        instr(5, OpCode::Unpack),
        instr(6, OpCode::Drop),
        instr(7, OpCode::Ret),
    ];
    let cfg = CfgBuilder::new(&instructions).build();
    let mut context = MethodContext {
        returns_value: Some(false),
        ..MethodContext::default()
    };
    context.calls_by_offset.insert(
        0,
        CallContract::new(
            SemanticCallTarget::Internal {
                offset: 8,
                name: "pair".to_string(),
            },
            0,
            true,
        )
        .with_return_shape(Some(CollectionShape::Struct(2))),
    );

    let built = SsaBuilder::new(&cfg, &instructions)
        .with_method_context(&context)
        .build_with_report();

    assert!(built.fidelity.issues.iter().any(|issue| {
        issue.offset == 5
            && issue.opcode == OpCode::Unpack
            && issue.kind == LoweringIssueKind::MissingProvenance
    }));
}

#[test]
fn adjacent_drop_bare_throw_preserves_the_empty_stack_fault_exactly() {
    let instructions = vec![
        instr(0, OpCode::Push1),
        instr(1, OpCode::Drop),
        instr(2, OpCode::Throw),
    ];
    let cfg = CfgBuilder::new(&instructions).build();

    let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

    assert_eq!(
        built.fidelity.status,
        Fidelity::Exact,
        "{:#?}",
        built.fidelity
    );
    assert!(has_payloadless_throw(&built.ssa));
}

#[test]
fn drop_throw_with_an_ambient_value_keeps_the_throw_payload() {
    let instructions = vec![
        instr(0, OpCode::Push1),
        instr(1, OpCode::Push2),
        instr(2, OpCode::Drop),
        instr(3, OpCode::Throw),
    ];
    let cfg = CfgBuilder::new(&instructions).build();

    let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

    assert_eq!(built.fidelity.status, Fidelity::Exact);
    assert!(!has_payloadless_throw(&built.ssa));
    assert!(built
        .ssa
        .blocks_iter()
        .flat_map(|(_, block)| &block.stmts)
        .any(|statement| matches!(statement, SsaStmt::Throw(Some(_)))));
}

#[test]
fn drop_bare_throw_is_not_fused_across_a_basic_block_boundary() {
    let instructions = vec![
        instr(0, OpCode::Push1),
        instr(1, OpCode::Drop),
        instr(2, OpCode::Throw),
    ];
    let mut cfg = Cfg::new();
    cfg.add_block(BasicBlock::new(
        BlockId(0),
        0,
        2,
        0..2,
        Terminator::Jump { target: BlockId(1) },
    ));
    cfg.add_block(BasicBlock::new(BlockId(1), 2, 3, 2..3, Terminator::Throw));
    cfg.add_edge(BlockId(0), BlockId(1), EdgeKind::Unconditional);

    let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

    assert!(!has_payloadless_throw(&built.ssa));
    assert!(built.fidelity.issues.iter().any(|issue| {
        issue.offset == 2
            && issue.opcode == OpCode::Throw
            && issue.kind == LoweringIssueKind::LostStackValue
    }));
}

#[test]
fn adjacent_unpack_packstruct_becomes_exact_clone_intrinsic() {
    let instructions = vec![
        instr(0, OpCode::Push1),
        instr(1, OpCode::Unpack),
        instr(2, OpCode::Packstruct),
        instr(3, OpCode::Ret),
    ];
    let cfg = CfgBuilder::new(&instructions).build();

    let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

    assert_eq!(
        built.fidelity.status,
        Fidelity::Exact,
        "{:#?}",
        built.fidelity
    );
    assert!(has_unpack_packstruct_intrinsic(&built.ssa));
    let call = built
        .ssa
        .blocks_iter()
        .flat_map(|(_, block)| &block.stmts)
        .find_map(|statement| match statement {
            SsaStmt::Assign {
                value:
                    SsaExpr::Call {
                        target: SemanticCallTarget::Intrinsic(Intrinsic::UnpackPackStruct),
                        args,
                    },
                ..
            } => Some(args),
            _ => None,
        })
        .expect("adjacent pair must emit the clone intrinsic");
    assert_eq!(
        call.len(),
        1,
        "the clone must consume only its source value"
    );
}

#[test]
fn unpack_packstruct_fusion_preserves_ambient_stack_values() {
    let instructions = vec![
        instr(0, OpCode::Push1),
        instr(1, OpCode::Push2),
        instr(2, OpCode::Unpack),
        instr(3, OpCode::Packstruct),
        instr(4, OpCode::Drop),
        instr(5, OpCode::Ret),
    ];

    assert_eq!(
        optimized_return_expression(&instructions),
        SsaExpr::lit(Literal::Int(1))
    );
}

#[test]
fn non_adjacent_unpack_packstruct_is_not_fused() {
    let instructions = vec![
        instr(0, OpCode::Push1),
        instr(1, OpCode::Unpack),
        instr(2, OpCode::Nop),
        instr(3, OpCode::Packstruct),
        instr(4, OpCode::Ret),
    ];
    let cfg = CfgBuilder::new(&instructions).build();

    let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

    assert!(!has_unpack_packstruct_intrinsic(&built.ssa));
}

#[test]
fn unpack_packstruct_is_not_fused_across_basic_block_boundary() {
    let instructions = vec![
        instr(0, OpCode::Push1),
        instr(1, OpCode::Unpack),
        instr(2, OpCode::Packstruct),
        instr(3, OpCode::Ret),
    ];
    let mut cfg = Cfg::new();
    cfg.add_block(BasicBlock::new(
        BlockId(0),
        0,
        2,
        0..2,
        Terminator::Jump { target: BlockId(1) },
    ));
    cfg.add_block(BasicBlock::new(BlockId(1), 2, 4, 2..4, Terminator::Return));
    cfg.add_edge(BlockId(0), BlockId(1), EdgeKind::Unconditional);

    let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

    assert!(!has_unpack_packstruct_intrinsic(&built.ssa));
}

#[test]
fn unpack_packstruct_fusion_preserves_source_underflow_diagnostic() {
    let instructions = vec![
        instr(0, OpCode::Unpack),
        instr(1, OpCode::Packstruct),
        instr(2, OpCode::Ret),
    ];
    let cfg = CfgBuilder::new(&instructions).build();

    let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

    assert!(built.fidelity.issues.iter().any(|issue| {
        issue.offset == 0
            && issue.opcode == OpCode::Unpack
            && issue.kind == LoweringIssueKind::LostStackValue
            && issue.detail.contains("requires 1 stack values")
    }));
}

#[test]
fn unpack_packstruct_fusion_preserves_unknown_source_diagnostic() {
    let (instructions, cfg) = uneven_stack_merge(vec![
        instr(3, OpCode::Unpack),
        instr(4, OpCode::Packstruct),
        instr(5, OpCode::Ret),
    ]);

    let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

    assert!(built.fidelity.issues.iter().any(|issue| {
        issue.offset == 3
            && issue.opcode == OpCode::Unpack
            && issue.kind == LoweringIssueKind::LostStackValue
            && issue
                .detail
                .contains("clone consumes an unknown stack value")
    }));
}

#[test]
fn signed_wide_pushes_decode_to_decimal() {
    for (opcode, bytes) in [
        (OpCode::Pushint128, vec![0xFF; 16]),
        (OpCode::Pushint256, vec![0xFF; 32]),
    ] {
        let instruction = Instruction::new(0, opcode, Some(Operand::Bytes(bytes)));
        assert_eq!(
            literal_for_push(opcode, &instruction),
            Some(Literal::BigInt("-1".to_string())),
            "{opcode:?}"
        );
    }
}

#[test]
fn printable_pushdata_becomes_string_literal() {
    let instruction = Instruction::new(
        0,
        OpCode::Pushdata1,
        Some(Operand::Bytes(b"hello".to_vec())),
    );

    assert_eq!(
        literal_for_push(OpCode::Pushdata1, &instruction),
        Some(Literal::String("hello".to_string()))
    );
}

#[test]
fn nonprintable_pushdata_remains_bytes() {
    let bytes = vec![0x00, 0xFF];
    let instruction = Instruction::new(0, OpCode::Pushdata1, Some(Operand::Bytes(bytes.clone())));

    assert_eq!(
        literal_for_push(OpCode::Pushdata1, &instruction),
        Some(Literal::Bytes(bytes))
    );
}

#[test]
fn user_append_call_remains_internal_while_vm_append_is_intrinsic() {
    let internal_instructions = vec![
        instr(0, OpCode::Push2),
        instr(1, OpCode::Push1),
        Instruction::new(2, OpCode::Call, Some(Operand::Jump(6))),
        instr(4, OpCode::Ret),
    ];
    let internal_cfg = CfgBuilder::new(&internal_instructions).build();
    let mut context = MethodContext::default();
    context.calls_by_offset.insert(
        2,
        CallContract::new(
            SemanticCallTarget::Internal {
                offset: 8,
                name: "append".to_string(),
            },
            2,
            false,
        ),
    );
    let internal_ssa = SsaBuilder::new(&internal_cfg, &internal_instructions)
        .with_method_context(&context)
        .build();
    let internal_target = internal_ssa
        .blocks_iter()
        .flat_map(|(_, block)| &block.stmts)
        .find_map(|stmt| match stmt {
            SsaStmt::Expr(SsaExpr::Call { target, .. }) => Some(target),
            _ => None,
        })
        .expect("resolved internal append call");

    let intrinsic_instructions = vec![
        instr(0, OpCode::Push2),
        instr(1, OpCode::Push1),
        instr(2, OpCode::Append),
        instr(3, OpCode::Ret),
    ];
    let intrinsic_cfg = CfgBuilder::new(&intrinsic_instructions).build();
    let intrinsic_ssa = SsaBuilder::new(&intrinsic_cfg, &intrinsic_instructions).build();
    let intrinsic_target = intrinsic_ssa
        .blocks_iter()
        .flat_map(|(_, block)| &block.stmts)
        .find_map(|stmt| match stmt {
            SsaStmt::Expr(SsaExpr::Call { target, .. }) => Some(target),
            _ => None,
        })
        .expect("VM APPEND intrinsic call");

    assert!(matches!(
        internal_target,
        SemanticCallTarget::Internal { offset: 8, name } if name == "append"
    ));
    assert!(matches!(
        intrinsic_target,
        SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Append))
    ));
    assert_eq!(internal_target.display_name(), "append");
    assert_eq!(intrinsic_target.display_name(), "append");
    assert_ne!(internal_target, intrinsic_target);
}

#[test]
fn context_free_calls_preserve_encoded_identity() {
    let cases = [
        (
            Instruction::new(0, OpCode::Call, Some(Operand::Jump(8))),
            SemanticCallTarget::Internal {
                offset: 8,
                name: "call_0x0008".to_string(),
            },
        ),
        (
            Instruction::new(0, OpCode::Call_L, Some(Operand::Jump32(12))),
            SemanticCallTarget::Internal {
                offset: 12,
                name: "call_0x000C".to_string(),
            },
        ),
        (
            Instruction::new(0, OpCode::CallT, Some(Operand::U16(7))),
            SemanticCallTarget::MethodToken {
                index: 7,
                name: "callt_0x0007".to_string(),
                hash_le: None,
                call_flags: None,
            },
        ),
    ];

    for (call, expected) in cases {
        let instructions = vec![call, instr(2, OpCode::Ret)];
        let cfg = CfgBuilder::new(&instructions).build();
        let ssa = SsaBuilder::new(&cfg, &instructions).build();
        let target = ssa
            .blocks_iter()
            .flat_map(|(_, block)| &block.stmts)
            .find_map(|stmt| match stmt {
                SsaStmt::Assign {
                    value: SsaExpr::Call { target, .. },
                    ..
                } => Some(target),
                _ => None,
            })
            .expect("context-free call target");

        assert_eq!(target, &expected);
    }
}
