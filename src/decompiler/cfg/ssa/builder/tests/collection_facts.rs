use super::*;

#[test]
fn aliased_unpack_preserves_unmodified_collection_provenance() {
    let instructions = vec![
        instr(0, OpCode::Push1),
        instr(1, OpCode::Push1),
        instr(2, OpCode::Pack),
        instr(3, OpCode::Dup),
        instr(4, OpCode::Unpack),
        instr(5, OpCode::Drop),
        instr(6, OpCode::Ret),
    ];
    let cfg = CfgBuilder::new(&instructions).build();
    let context = MethodContext {
        returns_value: Some(false),
        ..MethodContext::default()
    };

    let built = SsaBuilder::new(&cfg, &instructions)
        .with_method_context(&context)
        .build_with_report();

    assert_eq!(built.fidelity.status, Fidelity::Exact);
}

#[test]
fn slot_round_trip_preserves_unmodified_collection_provenance() {
    let instructions = vec![
        instr(0, OpCode::Push1),
        instr(1, OpCode::Push1),
        instr(2, OpCode::Pack),
        instr(3, OpCode::Stloc0),
        instr(4, OpCode::Ldloc0),
        instr(5, OpCode::Unpack),
        instr(6, OpCode::Drop),
        instr(7, OpCode::Ret),
    ];
    let cfg = CfgBuilder::new(&instructions).build();
    let context = MethodContext {
        returns_value: Some(false),
        ..MethodContext::default()
    };

    let built = SsaBuilder::new(&cfg, &instructions)
        .with_method_context(&context)
        .build_with_report();

    assert_eq!(built.fidelity.status, Fidelity::Exact);
}

#[test]
fn collection_mutation_invalidates_all_alias_provenance() {
    let instructions = vec![
        instr(0, OpCode::Push1),
        instr(1, OpCode::Push1),
        instr(2, OpCode::Pack),
        instr(3, OpCode::Dup),
        instr(4, OpCode::Push2),
        instr(5, OpCode::Append),
        instr(6, OpCode::Unpack),
        instr(7, OpCode::Drop),
        instr(8, OpCode::Ret),
    ];
    let cfg = CfgBuilder::new(&instructions).build();
    let context = MethodContext {
        returns_value: Some(false),
        ..MethodContext::default()
    };

    let built = SsaBuilder::new(&cfg, &instructions)
        .with_method_context(&context)
        .build_with_report();

    assert_eq!(built.fidelity.status, Fidelity::Incomplete);
    assert!(built.fidelity.issues.iter().any(|issue| {
        issue.offset == 6
            && issue.opcode == OpCode::Unpack
            && issue.kind == LoweringIssueKind::MissingProvenance
            && issue.fidelity == Fidelity::Incomplete
    }));
}

#[test]
fn setitem_invalidates_contents_but_preserves_collection_shape() {
    let instructions = vec![
        instr(0, OpCode::Push1),
        instr(1, OpCode::Push1),
        instr(2, OpCode::Packstruct),
        instr(3, OpCode::Dup),
        instr(4, OpCode::Push0),
        instr(5, OpCode::Push2),
        instr(6, OpCode::Setitem),
        instr(7, OpCode::Unpack),
        instr(8, OpCode::Drop),
        instr(9, OpCode::Drop),
        instr(10, OpCode::Ret),
    ];
    let cfg = CfgBuilder::new(&instructions).build();
    let context = MethodContext {
        returns_value: Some(false),
        ..MethodContext::default()
    };

    let built = SsaBuilder::new(&cfg, &instructions)
        .with_method_context(&context)
        .build_with_report();

    assert_eq!(
        built.fidelity.status,
        Fidelity::Exact,
        "{:#?}",
        built.fidelity
    );
    assert!(built
        .ssa
        .blocks_iter()
        .flat_map(|(_, block)| &block.stmts)
        .any(|statement| matches!(
            statement,
            SsaStmt::Assign {
                value: SsaExpr::Index { .. },
                ..
            }
        )));
}

#[test]
fn dynamic_pickitem_uses_a_uniform_nested_collection_shape() {
    let instructions = vec![
        Instruction::new(0, OpCode::Initslot, Some(Operand::Bytes(vec![0, 1]))),
        instr(3, OpCode::Push1),
        instr(4, OpCode::Push1),
        instr(5, OpCode::Push2),
        instr(6, OpCode::Packstruct),
        instr(7, OpCode::Push1),
        instr(8, OpCode::Push1),
        instr(9, OpCode::Push2),
        instr(10, OpCode::Packstruct),
        instr(11, OpCode::Push2),
        instr(12, OpCode::Pack),
        instr(13, OpCode::Stloc0),
        instr(14, OpCode::Ldloc0),
        instr(15, OpCode::Ldarg0),
        instr(16, OpCode::Pickitem),
        instr(17, OpCode::Unpack),
        instr(18, OpCode::Drop),
        instr(19, OpCode::Drop),
        instr(20, OpCode::Drop),
        instr(21, OpCode::Ret),
    ];
    let cfg = CfgBuilder::new(&instructions).build();
    let context = MethodContext {
        argument_names: vec!["arg0".to_string()],
        returns_value: Some(false),
        ..MethodContext::default()
    };

    let built = SsaBuilder::new(&cfg, &instructions)
        .with_method_context(&context)
        .build_with_report();

    assert_eq!(
        built.fidelity.status,
        Fidelity::Exact,
        "{:#?}",
        built.fidelity
    );
}

#[test]
fn internal_call_return_facts_reach_dynamic_pickitem_unpack() {
    let instructions = vec![
        Instruction::new(0, OpCode::Initslot, Some(Operand::Bytes(vec![0, 1]))),
        Instruction::new(3, OpCode::Call, Some(Operand::Jump(97))),
        instr(5, OpCode::Ldarg0),
        instr(6, OpCode::Pickitem),
        instr(7, OpCode::Unpack),
        instr(8, OpCode::Drop),
        instr(9, OpCode::Drop),
        instr(10, OpCode::Drop),
        instr(11, OpCode::Ret),
    ];
    let cfg = CfgBuilder::new(&instructions).build();
    let return_facts = CollectionShapeFacts {
        shape: Some(CollectionShape::Array(2)),
        indexed: BTreeMap::from([
            (0, CollectionShape::Struct(2)),
            (1, CollectionShape::Struct(2)),
        ]),
    };
    let mut context = MethodContext {
        argument_names: vec!["arg0".to_string()],
        returns_value: Some(false),
        ..MethodContext::default()
    };
    context.calls_by_offset.insert(
        3,
        CallContract::new(
            SemanticCallTarget::Internal {
                offset: 100,
                name: "tupleFactory".to_string(),
            },
            0,
            true,
        )
        .with_return_facts(Some(return_facts)),
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
}

fn argument_field_writes(
    instructions: Vec<Instruction>,
    argument_count: usize,
) -> Vec<BTreeMap<usize, CollectionShape>> {
    let cfg = CfgBuilder::new(&instructions).build();
    let context = MethodContext {
        argument_names: (0..argument_count)
            .map(|index| format!("arg{index}"))
            .collect(),
        returns_value: Some(false),
        ..MethodContext::default()
    };
    SsaBuilder::new(&cfg, &instructions)
        .with_method_context(&context)
        .build_with_report()
        .collection_analysis
        .argument_field_writes
}

#[test]
fn argument_field_writes_reject_dynamic_conflicting_partial_and_overwritten_facts() {
    let dynamic_index = vec![
        Instruction::new(0, OpCode::Initslot, Some(Operand::Bytes(vec![0, 2]))),
        instr(3, OpCode::Ldarg0),
        instr(4, OpCode::Ldarg1),
        instr(5, OpCode::Push1),
        instr(6, OpCode::Push1),
        instr(7, OpCode::Push2),
        instr(8, OpCode::Pack),
        instr(9, OpCode::Setitem),
        instr(10, OpCode::Ret),
    ];
    assert_eq!(
        argument_field_writes(dynamic_index, 2),
        vec![BTreeMap::new(), BTreeMap::new()]
    );

    let conflicting_paths = vec![
        Instruction::new(0, OpCode::Initslot, Some(Operand::Bytes(vec![0, 1]))),
        instr(3, OpCode::Push1),
        Instruction::new(4, OpCode::Jmpif, Some(Operand::Jump(10))),
        instr(6, OpCode::Ldarg0),
        instr(7, OpCode::Push0),
        instr(8, OpCode::Push1),
        instr(9, OpCode::Push1),
        instr(10, OpCode::Push2),
        instr(11, OpCode::Pack),
        instr(12, OpCode::Setitem),
        instr(13, OpCode::Ret),
        instr(14, OpCode::Ldarg0),
        instr(15, OpCode::Push0),
        instr(16, OpCode::Push1),
        instr(17, OpCode::Push1),
        instr(18, OpCode::Push1),
        instr(19, OpCode::Push3),
        instr(20, OpCode::Pack),
        instr(21, OpCode::Setitem),
        instr(22, OpCode::Ret),
    ];
    assert_eq!(
        argument_field_writes(conflicting_paths, 1),
        vec![BTreeMap::new()]
    );

    let partial_paths = vec![
        Instruction::new(0, OpCode::Initslot, Some(Operand::Bytes(vec![0, 1]))),
        instr(3, OpCode::Push1),
        Instruction::new(4, OpCode::Jmpif, Some(Operand::Jump(3))),
        instr(6, OpCode::Ret),
        instr(7, OpCode::Ldarg0),
        instr(8, OpCode::Push0),
        instr(9, OpCode::Push1),
        instr(10, OpCode::Push1),
        instr(11, OpCode::Push2),
        instr(12, OpCode::Pack),
        instr(13, OpCode::Setitem),
        instr(14, OpCode::Ret),
    ];
    assert_eq!(
        argument_field_writes(partial_paths, 1),
        vec![BTreeMap::new()]
    );

    let overwritten = vec![
        Instruction::new(0, OpCode::Initslot, Some(Operand::Bytes(vec![0, 1]))),
        instr(3, OpCode::Ldarg0),
        instr(4, OpCode::Push0),
        instr(5, OpCode::Push1),
        instr(6, OpCode::Push1),
        instr(7, OpCode::Push2),
        instr(8, OpCode::Pack),
        instr(9, OpCode::Setitem),
        instr(10, OpCode::Ldarg0),
        instr(11, OpCode::Push0),
        instr(12, OpCode::Push1),
        instr(13, OpCode::Setitem),
        instr(14, OpCode::Ret),
    ];
    assert_eq!(argument_field_writes(overwritten, 1), vec![BTreeMap::new()]);
}

#[test]
fn static_alias_resize_and_unknown_call_prevent_reusing_seeded_shape() {
    let seeded = CollectionShapeFacts {
        shape: Some(CollectionShape::Array(2)),
        indexed: BTreeMap::from([(0, CollectionShape::Array(2))]),
    };
    let resize_instructions = vec![
        instr(0, OpCode::Ldsfld0),
        instr(1, OpCode::Push1),
        instr(2, OpCode::Append),
        instr(3, OpCode::Ret),
    ];
    let resize_cfg = CfgBuilder::new(&resize_instructions).build();
    let resize_context = MethodContext {
        returns_value: Some(false),
        static_collection_facts: BTreeMap::from([(0, seeded.clone())]),
        ..MethodContext::default()
    };
    let resized = SsaBuilder::new(&resize_cfg, &resize_instructions)
        .with_method_context(&resize_context)
        .build_with_report();
    assert!(resized
        .collection_analysis
        .static_writes
        .iter()
        .any(|write| { write.index == 0 && write.facts.is_none() && !write.provisional }));

    let call_instructions = vec![
        instr(0, OpCode::Ldsfld0),
        Instruction::new(1, OpCode::Call, Some(Operand::Jump(8))),
        instr(3, OpCode::Ldsfld0),
        instr(4, OpCode::Unpack),
        instr(5, OpCode::Drop),
        instr(6, OpCode::Drop),
        instr(7, OpCode::Drop),
        instr(8, OpCode::Ret),
    ];
    let call_cfg = CfgBuilder::new(&call_instructions).build();
    let mut call_context = MethodContext {
        returns_value: Some(false),
        static_collection_facts: BTreeMap::from([(0, seeded)]),
        ..MethodContext::default()
    };
    call_context.calls_by_offset.insert(
        1,
        CallContract::new(
            SemanticCallTarget::Internal {
                offset: 9,
                name: "unknown_mutator".to_string(),
            },
            1,
            false,
        ),
    );
    let called = SsaBuilder::new(&call_cfg, &call_instructions)
        .with_method_context(&call_context)
        .build_with_report();
    assert!(called.fidelity.issues.iter().any(|issue| {
        issue.offset == 4
            && issue.opcode == OpCode::Unpack
            && issue.kind == LoweringIssueKind::MissingProvenance
    }));
    assert!(called
        .collection_analysis
        .static_writes
        .iter()
        .any(|write| { write.index == 0 && write.facts.is_none() && write.provisional }));
}

#[test]
fn value_returning_collection_mutation_invalidates_all_alias_provenance() {
    let instructions = vec![
        instr(0, OpCode::Push1),
        instr(1, OpCode::Push1),
        instr(2, OpCode::Pack),
        instr(3, OpCode::Dup),
        instr(4, OpCode::Popitem),
        instr(5, OpCode::Drop),
        instr(6, OpCode::Unpack),
        instr(7, OpCode::Drop),
        instr(8, OpCode::Ret),
    ];
    let cfg = CfgBuilder::new(&instructions).build();
    let context = MethodContext {
        returns_value: Some(false),
        ..MethodContext::default()
    };

    let built = SsaBuilder::new(&cfg, &instructions)
        .with_method_context(&context)
        .build_with_report();

    assert!(built.fidelity.issues.iter().any(|issue| {
        issue.offset == 6
            && issue.opcode == OpCode::Unpack
            && issue.kind == LoweringIssueKind::MissingProvenance
            && issue.fidelity == Fidelity::Incomplete
    }));
}

#[test]
fn known_call_invalidates_collection_argument_provenance() {
    let instructions = vec![
        instr(0, OpCode::Push1),
        instr(1, OpCode::Push1),
        instr(2, OpCode::Pack),
        instr(3, OpCode::Dup),
        Instruction::new(4, OpCode::CallT, Some(Operand::U16(0))),
        instr(7, OpCode::Unpack),
        instr(8, OpCode::Drop),
        instr(9, OpCode::Ret),
    ];
    let cfg = CfgBuilder::new(&instructions).build();
    let mut context = MethodContext {
        returns_value: Some(false),
        ..MethodContext::default()
    };
    context.calls_by_offset.insert(
        4,
        CallContract::new(
            SemanticCallTarget::MethodToken {
                index: 0,
                name: "mutate".to_string(),
                hash_le: None,
                call_flags: None,
            },
            1,
            false,
        ),
    );

    let built = SsaBuilder::new(&cfg, &instructions)
        .with_method_context(&context)
        .build_with_report();

    assert!(built.fidelity.issues.iter().any(|issue| {
        issue.offset == 7
            && issue.opcode == OpCode::Unpack
            && issue.kind == LoweringIssueKind::MissingProvenance
    }));
}

#[test]
fn collection_returning_call_does_not_preserve_argument_shape_across_alias_mutation() {
    let instructions = vec![
        instr(0, OpCode::Push1),
        instr(1, OpCode::Push1),
        instr(2, OpCode::Pack),
        instr(3, OpCode::Dup),
        Instruction::new(4, OpCode::Call, Some(Operand::Jump(6))),
        instr(7, OpCode::Push2),
        instr(8, OpCode::Append),
        instr(9, OpCode::Unpack),
        instr(10, OpCode::Drop),
        instr(11, OpCode::Ret),
    ];
    let cfg = CfgBuilder::new(&instructions).build();
    let mut context = MethodContext {
        returns_value: Some(false),
        ..MethodContext::default()
    };
    context.calls_by_offset.insert(
        4,
        CallContract::new(
            SemanticCallTarget::Internal {
                offset: 10,
                name: "identity_collection".to_string(),
            },
            1,
            true,
        )
        .with_return_shape(Some(CollectionShape::Array(2)))
        .with_argument_effects(vec![CollectionArgumentEffect::ReadOnly]),
    );

    let built = SsaBuilder::new(&cfg, &instructions)
        .with_method_context(&context)
        .build_with_report();

    assert!(built.fidelity.issues.iter().any(|issue| {
        issue.offset == 9
            && issue.opcode == OpCode::Unpack
            && issue.kind == LoweringIssueKind::MissingProvenance
    }));
}

#[test]
fn shape_preserving_internal_call_discards_contents_but_retains_arity() {
    let instructions = vec![
        instr(0, OpCode::Push1),
        instr(1, OpCode::Push1),
        instr(2, OpCode::Packstruct),
        instr(3, OpCode::Dup),
        Instruction::new(4, OpCode::Call, Some(Operand::Jump(6))),
        instr(6, OpCode::Unpack),
        instr(7, OpCode::Drop),
        instr(8, OpCode::Drop),
        instr(9, OpCode::Ret),
    ];
    let cfg = CfgBuilder::new(&instructions).build();
    let mut context = MethodContext {
        returns_value: Some(false),
        ..MethodContext::default()
    };
    context.calls_by_offset.insert(
        4,
        CallContract::new(
            SemanticCallTarget::Internal {
                offset: 10,
                name: "set_fields".to_string(),
            },
            1,
            false,
        )
        .with_argument_effects(vec![CollectionArgumentEffect::PreservesShape]),
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
    assert!(built
        .ssa
        .blocks_iter()
        .flat_map(|(_, block)| &block.stmts)
        .any(|statement| matches!(
            statement,
            SsaStmt::Assign {
                value: SsaExpr::Index { .. },
                ..
            }
        )));
}

#[test]
fn syscall_invalidates_collection_argument_provenance() {
    let instructions = vec![
        instr(0, OpCode::Push1),
        instr(1, OpCode::Push1),
        instr(2, OpCode::Pack),
        instr(3, OpCode::Dup),
        Instruction::new(4, OpCode::Syscall, Some(Operand::Syscall(0x9647_E7CF))),
        instr(9, OpCode::Unpack),
        instr(10, OpCode::Drop),
        instr(11, OpCode::Ret),
    ];
    let cfg = CfgBuilder::new(&instructions).build();
    let context = MethodContext {
        returns_value: Some(false),
        ..MethodContext::default()
    };

    let built = SsaBuilder::new(&cfg, &instructions)
        .with_method_context(&context)
        .build_with_report();

    assert!(built.fidelity.issues.iter().any(|issue| {
        issue.offset == 9
            && issue.opcode == OpCode::Unpack
            && issue.kind == LoweringIssueKind::MissingProvenance
    }));
}

#[test]
fn opaque_call_invalidates_all_collection_provenance() {
    let instructions = vec![
        instr(0, OpCode::Push1),
        instr(1, OpCode::Push1),
        instr(2, OpCode::Pack),
        instr(3, OpCode::Stloc0),
        Instruction::new(4, OpCode::Call, Some(Operand::Jump(2))),
        instr(6, OpCode::Drop),
        instr(7, OpCode::Ldloc0),
        instr(8, OpCode::Unpack),
        instr(9, OpCode::Drop),
        instr(10, OpCode::Ret),
    ];
    let cfg = CfgBuilder::new(&instructions).build();
    let context = MethodContext {
        returns_value: Some(false),
        ..MethodContext::default()
    };

    let built = SsaBuilder::new(&cfg, &instructions)
        .with_method_context(&context)
        .build_with_report();

    assert!(built.fidelity.issues.iter().any(|issue| {
        issue.offset == 8
            && issue.opcode == OpCode::Unpack
            && issue.kind == LoweringIssueKind::MissingProvenance
    }));
}

#[test]
fn internal_call_invalidates_static_collection_provenance() {
    let instructions = vec![
        instr(0, OpCode::Push1),
        instr(1, OpCode::Push1),
        instr(2, OpCode::Pack),
        instr(3, OpCode::Stsfld0),
        Instruction::new(4, OpCode::Call, Some(Operand::Jump(3))),
        instr(6, OpCode::Ldsfld0),
        instr(7, OpCode::Unpack),
        instr(8, OpCode::Drop),
        instr(9, OpCode::Ret),
    ];
    let cfg = CfgBuilder::new(&instructions).build();
    let mut context = MethodContext {
        returns_value: Some(false),
        ..MethodContext::default()
    };
    context.calls_by_offset.insert(
        4,
        CallContract::new(
            SemanticCallTarget::Internal {
                offset: 7,
                name: "mutate_static".to_string(),
            },
            0,
            false,
        ),
    );

    let built = SsaBuilder::new(&cfg, &instructions)
        .with_method_context(&context)
        .build_with_report();

    assert!(built.fidelity.issues.iter().any(|issue| {
        issue.offset == 7
            && issue.opcode == OpCode::Unpack
            && issue.kind == LoweringIssueKind::MissingProvenance
    }));
}

#[test]
fn later_internal_call_does_not_retroactively_invalidate_collection_provenance() {
    let instructions = vec![
        instr(0, OpCode::Push1),
        instr(1, OpCode::Push1),
        instr(2, OpCode::Pack),
        instr(3, OpCode::Stloc0),
        instr(4, OpCode::Ldloc0),
        instr(5, OpCode::Unpack),
        instr(6, OpCode::Drop),
        instr(7, OpCode::Drop),
        Instruction::new(8, OpCode::Call, Some(Operand::Jump(2))),
        instr(10, OpCode::Ret),
    ];
    let cfg = CfgBuilder::new(&instructions).build();
    let mut context = MethodContext {
        returns_value: Some(false),
        ..MethodContext::default()
    };
    context.calls_by_offset.insert(
        8,
        CallContract::new(
            SemanticCallTarget::Internal {
                offset: 10,
                name: "later_call".to_string(),
            },
            0,
            false,
        ),
    );

    let built = SsaBuilder::new(&cfg, &instructions)
        .with_method_context(&context)
        .build_with_report();

    assert!(!built.fidelity.issues.iter().any(|issue| {
        issue.offset == 5
            && issue.opcode == OpCode::Unpack
            && issue.kind == LoweringIssueKind::MissingProvenance
    }));
}

#[test]
fn loop_backedge_mutation_invalidates_header_collection_provenance() {
    let instructions = vec![
        instr(0, OpCode::Push1),
        instr(1, OpCode::Push1),
        instr(2, OpCode::Pack),
        instr(3, OpCode::Stloc0),
        instr(4, OpCode::Ldloc0),
        instr(5, OpCode::Unpack),
        instr(6, OpCode::Drop),
        instr(7, OpCode::Drop),
        instr(8, OpCode::Ldloc0),
        instr(9, OpCode::Push2),
        instr(10, OpCode::Append),
        Instruction::new(11, OpCode::Jmp, Some(Operand::Jump(-7))),
    ];
    let cfg = CfgBuilder::new(&instructions).build();
    let context = MethodContext {
        returns_value: Some(false),
        ..MethodContext::default()
    };

    let built = SsaBuilder::new(&cfg, &instructions)
        .with_method_context(&context)
        .build_with_report();

    assert!(built.fidelity.issues.iter().any(|issue| {
        issue.offset == 5
            && issue.opcode == OpCode::Unpack
            && issue.kind == LoweringIssueKind::MissingProvenance
    }));
}
