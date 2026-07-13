use super::*;

#[test]
fn reported_build_records_unresolved_call_at_the_call_site() {
    let instructions = vec![
        Instruction::new(0, OpCode::Call, Some(Operand::Jump(2))),
        instr(2, OpCode::Ret),
    ];
    let cfg = CfgBuilder::new(&instructions).build();
    let context = MethodContext::default();

    let built = SsaBuilder::new(&cfg, &instructions)
        .with_method_context(&context)
        .build_with_report();

    assert_eq!(built.fidelity.status, Fidelity::Incomplete);
    assert!(built.fidelity.issues.iter().any(|issue| {
        issue.offset == 0
            && issue.opcode == OpCode::Call
            && issue.kind == LoweringIssueKind::UnresolvedCall
            && issue.fidelity == Fidelity::Incomplete
    }));
}

#[test]
fn reported_build_records_explicitly_unresolved_call_target() {
    let instructions = vec![
        Instruction::new(0, OpCode::CallT, Some(Operand::U16(3))),
        instr(3, OpCode::Ret),
    ];
    let cfg = CfgBuilder::new(&instructions).build();
    let mut context = MethodContext::default();
    context.calls_by_offset.insert(
        0,
        CallContract::new(
            SemanticCallTarget::Unresolved {
                display_name: "ambiguous_call".to_string(),
            },
            0,
            true,
        ),
    );

    let built = SsaBuilder::new(&cfg, &instructions)
        .with_method_context(&context)
        .build_with_report();

    assert!(built.fidelity.issues.iter().any(|issue| {
        issue.offset == 0
            && issue.opcode == OpCode::CallT
            && issue.kind == LoweringIssueKind::UnresolvedCall
            && issue.fidelity == Fidelity::Incomplete
    }));
}

#[test]
fn reported_build_records_missing_operand_metadata_at_the_instruction() {
    let instructions = vec![instr(4, OpCode::Pushint8), instr(5, OpCode::Ret)];
    let cfg = CfgBuilder::new(&instructions).build();

    let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

    assert!(built.fidelity.issues.iter().any(|issue| {
        issue.offset == 4
            && issue.opcode == OpCode::Pushint8
            && issue.kind == LoweringIssueKind::MissingOperandMetadata
            && issue.fidelity == Fidelity::Incomplete
    }));
}

#[test]
fn unreachable_underflow_is_covered_without_reducing_semantic_fidelity() {
    let instructions = vec![instr(0, OpCode::Nop), instr(1, OpCode::Drop)];
    let mut cfg = Cfg::new();
    cfg.add_block(BasicBlock::new(BlockId(0), 0, 1, 0..1, Terminator::Return));
    cfg.add_block(BasicBlock::new(BlockId(1), 1, 2, 1..2, Terminator::Return));

    let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

    assert_eq!(built.fidelity.status, Fidelity::Exact);
    assert_eq!(
        built.fidelity.covered_offsets,
        BTreeSet::from([0usize, 1usize])
    );
    assert!(built.fidelity.issues.is_empty());
}

#[test]
fn reported_build_records_unknown_value_reaching_return() {
    let instructions = vec![
        instr(10, OpCode::Push1),
        instr(11, OpCode::Pack),
        instr(12, OpCode::Ret),
    ];
    let cfg = CfgBuilder::new(&instructions).build();

    let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

    assert!(built.fidelity.issues.iter().any(|issue| {
        issue.offset == 11
            && issue.opcode == OpCode::Pack
            && issue.kind == LoweringIssueKind::MissingProvenance
            && issue.fidelity == Fidelity::Incomplete
    }));
}

#[test]
fn reported_build_records_unknown_phi_reaching_return() {
    let (instructions, cfg) = uneven_stack_merge(vec![instr(3, OpCode::Ret)]);
    let context = MethodContext {
        returns_value: Some(true),
        ..MethodContext::default()
    };

    let built = SsaBuilder::new(&cfg, &instructions)
        .with_method_context(&context)
        .build_with_report();

    assert!(built.fidelity.issues.iter().any(|issue| {
        issue.offset == 3
            && issue.opcode == OpCode::Ret
            && issue.kind == LoweringIssueKind::LostStackValue
            && issue.fidelity == Fidelity::Incomplete
    }));
}

#[test]
fn reported_build_records_unknown_phi_consumed_by_resolved_call() {
    let (instructions, cfg) = uneven_stack_merge(vec![
        Instruction::new(3, OpCode::CallT, Some(Operand::U16(0))),
        instr(6, OpCode::Ret),
    ]);
    let mut context = MethodContext {
        returns_value: Some(false),
        ..MethodContext::default()
    };
    context.calls_by_offset.insert(
        3,
        CallContract::new(
            SemanticCallTarget::MethodToken {
                index: 0,
                name: "helper".to_string(),
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
        issue.offset == 3
            && issue.opcode == OpCode::CallT
            && issue.kind == LoweringIssueKind::LostStackValue
            && issue.fidelity == Fidelity::Incomplete
    }));
}

#[test]
fn reported_build_records_unknown_phi_consumed_by_drop() {
    let (instructions, cfg) =
        uneven_stack_merge(vec![instr(3, OpCode::Drop), instr(4, OpCode::Ret)]);
    let context = MethodContext {
        returns_value: Some(false),
        ..MethodContext::default()
    };

    let built = SsaBuilder::new(&cfg, &instructions)
        .with_method_context(&context)
        .build_with_report();

    assert!(built.fidelity.issues.iter().any(|issue| {
        issue.offset == 3
            && issue.opcode == OpCode::Drop
            && issue.kind == LoweringIssueKind::LostStackValue
            && issue.fidelity == Fidelity::Incomplete
    }));
}

#[test]
fn reported_build_records_fixed_reorder_underflow_at_the_instruction() {
    let instructions = vec![instr(20, OpCode::Dup), instr(21, OpCode::Ret)];
    let cfg = CfgBuilder::new(&instructions).build();

    let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

    assert!(built.fidelity.issues.iter().any(|issue| {
        issue.offset == 20
            && issue.opcode == OpCode::Dup
            && issue.kind == LoweringIssueKind::LostStackValue
            && issue.fidelity == Fidelity::Incomplete
    }));
}

#[test]
fn reported_build_marks_known_syscall_conservative() {
    let instructions = vec![
        instr(30, OpCode::Push1),
        Instruction::new(31, OpCode::Syscall, Some(Operand::Syscall(0x8CEC_27F8))),
        instr(36, OpCode::Ret),
    ];
    let cfg = CfgBuilder::new(&instructions).build();

    let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

    assert_eq!(built.fidelity.status, Fidelity::Conservative);
    assert!(built.fidelity.issues.iter().any(|issue| {
        issue.offset == 31
            && issue.opcode == OpCode::Syscall
            && issue.fidelity == Fidelity::Conservative
    }));
}

#[test]
fn reported_build_records_unsupported_control_at_the_instruction() {
    let instructions = vec![instr(40, OpCode::Endfinally), instr(41, OpCode::Ret)];
    let cfg = CfgBuilder::new(&instructions).build();

    let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

    assert!(built.fidelity.issues.iter().any(|issue| {
        issue.offset == 40
            && issue.opcode == OpCode::Endfinally
            && issue.kind == LoweringIssueKind::UnsupportedControl
            && issue.fidelity == Fidelity::Incomplete
    }));
}

#[test]
fn reported_build_records_stack_underflow_at_the_instruction() {
    let instructions = vec![instr(0, OpCode::Add), instr(1, OpCode::Ret)];
    let cfg = CfgBuilder::new(&instructions).build();

    let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

    assert!(built.fidelity.issues.iter().any(|issue| {
        issue.offset == 0
            && issue.opcode == OpCode::Add
            && issue.kind == LoweringIssueKind::LostStackValue
            && issue.fidelity == Fidelity::Incomplete
    }));
}

#[test]
fn reported_build_records_slot_load_without_reaching_definition() {
    let instructions = vec![instr(0, OpCode::Ldloc0), instr(1, OpCode::Ret)];
    let cfg = CfgBuilder::new(&instructions).build();

    let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

    assert!(built.fidelity.issues.iter().any(|issue| {
        issue.offset == 0
            && issue.opcode == OpCode::Ldloc0
            && issue.kind == LoweringIssueKind::LostStackValue
            && issue.fidelity == Fidelity::Incomplete
    }));
}
