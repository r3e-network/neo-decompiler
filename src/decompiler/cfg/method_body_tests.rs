use super::{
    classify_opcode, lower_method_body, register_structured_temporaries, Fidelity, FidelityReport,
    LoweringIssue, LoweringIssueKind, MethodIrRequest, MethodSymbolTypes, OpcodeFidelity,
    StatementId, SymbolInfo, SymbolOrigin,
};
use crate::decompiler::analysis::method_contracts::ReturnBehavior;
use crate::decompiler::analysis::types::ValueType;
use crate::decompiler::cfg::ssa::MethodContext;
use crate::decompiler::high_level::MAX_HIGH_LEVEL_METHOD_INSTRUCTIONS;
use crate::decompiler::ir::{
    render_block, Block, ControlFlow, Expr, Intrinsic, Literal, SemanticCallTarget, Stmt,
};
use crate::instruction::{Instruction, OpCode, Operand};
use std::collections::{BTreeMap, BTreeSet};

use super::types::register_structured_temporaries_with_call_types;

fn instruction(offset: usize, opcode: OpCode) -> Instruction {
    Instruction::new(offset, opcode, None)
}

#[test]
fn all_known_opcodes_have_an_explicit_classification() {
    let known = OpCode::all_known();
    assert!(!known.is_empty());
    for opcode in known {
        assert!(!matches!(opcode, OpCode::Unknown(_)));
        let _classification = classify_opcode(opcode);
    }
    assert!(matches!(
        classify_opcode(OpCode::Unknown(0xFF)),
        OpcodeFidelity::Incomplete(_)
    ));
}

#[test]
fn type_operand_opcodes_are_exact_once_tags_are_preserved() {
    for opcode in [OpCode::Convert, OpCode::Istype, OpCode::NewarrayT] {
        assert_eq!(
            classify_opcode(opcode),
            OpcodeFidelity::Exact,
            "{opcode:?} carries its operand type tag in structured IR"
        );
    }
}

#[test]
fn cat_temporaries_preserve_known_byte_container_types() {
    let cat = |left, right| {
        Expr::call(
            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Cat)),
            vec![left, right],
        )
    };
    let text = || Expr::Literal(Literal::String("text".to_string()));
    let body = Block::with_stmts(vec![
        Stmt::assign("text0", cat(text(), text())),
        Stmt::assign("text1", cat(Expr::var("text0"), text())),
        Stmt::assign("buffer0", cat(Expr::var("buffer"), text())),
        Stmt::assign("unknown0", cat(Expr::var("unknown"), text())),
    ]);
    let mut symbols = BTreeMap::from([
        (
            "buffer".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Local(0),
                value_type: ValueType::Buffer,
            },
        ),
        (
            "unknown".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Local(1),
                value_type: ValueType::Unknown,
            },
        ),
    ]);

    register_structured_temporaries(&body, &mut symbols);

    assert_eq!(symbols["text0"].value_type, ValueType::ByteString);
    assert_eq!(symbols["text1"].value_type, ValueType::ByteString);
    assert_eq!(symbols["buffer0"].value_type, ValueType::Buffer);
    assert_eq!(symbols["unknown0"].value_type, ValueType::Unknown);
}

#[test]
fn typed_array_index_temporaries_keep_their_element_type() {
    let body = Block::with_stmts(vec![Stmt::assign(
        "element",
        Expr::index(
            Expr::NewArray {
                length: Box::new(Expr::int(2)),
                element_type: Some(ValueType::Integer),
            },
            Expr::int(0),
        ),
    )]);
    let mut symbols = BTreeMap::new();

    register_structured_temporaries(&body, &mut symbols);

    assert_eq!(symbols["element"].value_type, ValueType::Integer);
}

#[test]
fn dynamic_stack_opcodes_defer_fidelity_to_literal_resolution() {
    for opcode in [OpCode::Pick, OpCode::Roll, OpCode::Xdrop, OpCode::Reversen] {
        assert_eq!(
            classify_opcode(opcode),
            OpcodeFidelity::Exact,
            "{opcode:?} is validated against its resolved operand by the SSA builder"
        );
    }
}

#[test]
fn report_finish_sorts_and_deduplicates_by_diagnostic_identity() {
    let duplicate = |fidelity| LoweringIssue {
        offset: 7,
        opcode: OpCode::Syscall,
        kind: LoweringIssueKind::MissingProvenance,
        fidelity,
        detail: "low-level syscall wrapper".to_string(),
    };
    let mut report = FidelityReport::exact(3);
    report.issues = vec![
        duplicate(Fidelity::Conservative),
        LoweringIssue {
            offset: 2,
            opcode: OpCode::Assert,
            kind: LoweringIssueKind::UnsupportedOpcode,
            fidelity: Fidelity::Incomplete,
            detail: "assertion effect is not represented".to_string(),
        },
        duplicate(Fidelity::Incomplete),
    ];

    report.finish();

    assert_eq!(report.status, Fidelity::Incomplete);
    assert_eq!(report.issues.len(), 2);
    assert_eq!(report.issues[0].offset, 2);
    assert_eq!(report.issues[1].offset, 7);
    assert_eq!(report.issues[1].fidelity, Fidelity::Incomplete);
}

#[test]
fn lowers_only_the_exact_slice_with_neutral_source_symbols() {
    let instructions = vec![
        instruction(0, OpCode::Assert),
        Instruction::new(10, OpCode::Initslot, Some(Operand::Bytes(vec![1, 1]))),
        instruction(11, OpCode::Ldarg0),
        instruction(12, OpCode::Stloc0),
        instruction(13, OpCode::Ldloc0),
        instruction(14, OpCode::Stsfld1),
        instruction(15, OpCode::Ldsfld1),
        instruction(16, OpCode::Ret),
        instruction(20, OpCode::Pack),
    ];
    let request = MethodIrRequest {
        start: 10,
        end: 17,
        instructions: &instructions,
        context: MethodContext {
            argument_names: vec!["amount".to_string()],
            returns_value: Some(true),
            ..MethodContext::default()
        },
        symbol_types: MethodSymbolTypes {
            parameters: vec![ValueType::Integer],
            locals: vec![ValueType::Boolean],
            statics: vec![ValueType::Unknown, ValueType::ByteString],
        },
        reduce_temps: false,
    };

    let lowered = lower_method_body(request);

    assert_eq!(
        lowered.fidelity.status,
        Fidelity::Exact,
        "{:#?}",
        lowered.fidelity.issues
    );
    assert_eq!(lowered.fidelity.instruction_count, 7);
    assert_eq!(
        lowered.fidelity.covered_offsets,
        std::collections::BTreeSet::from([10, 11, 12, 13, 14, 15, 16])
    );
    assert_eq!(lowered.symbols["amount"].origin, SymbolOrigin::Parameter(0));
    assert_eq!(lowered.symbols["amount"].value_type, ValueType::Integer);
    assert_eq!(lowered.symbols["loc0"].origin, SymbolOrigin::Local(0));
    assert_eq!(lowered.symbols["loc0"].value_type, ValueType::Boolean);
    assert_eq!(lowered.symbols["static1"].origin, SymbolOrigin::Static(1));
    assert_eq!(lowered.symbols["static1"].value_type, ValueType::ByteString);
    assert_eq!(lowered.return_behavior, ReturnBehavior::Value);
    assert!(!lowered.source_map.statement_origins.is_empty());
    assert!(lowered
        .source_map
        .statement_origins
        .values()
        .all(|origins| origins.iter().all(|offset| (10..17).contains(offset))));

    let rendered = render_block(&lowered.body, 0);
    assert!(!rendered.contains("arg0"), "{rendered}");
    assert!(!rendered.contains("loc0_"), "{rendered}");
    assert!(!rendered.contains("static1_"), "{rendered}");
}

#[test]
fn catch_exception_symbol_is_a_dynamic_vm_payload() {
    let instructions = vec![
        Instruction::new(0, OpCode::Try, Some(Operand::Bytes(vec![6, 0]))),
        instruction(3, OpCode::Nop),
        Instruction::new(4, OpCode::Endtry, Some(Operand::Jump(5))),
        instruction(6, OpCode::Drop),
        Instruction::new(7, OpCode::Endtry, Some(Operand::Jump(2))),
        instruction(9, OpCode::Ret),
    ];
    let lowered = lower_method_body(MethodIrRequest {
        start: 0,
        end: 10,
        instructions: &instructions,
        context: MethodContext {
            returns_value: Some(false),
            ..MethodContext::default()
        },
        symbol_types: MethodSymbolTypes::default(),
        reduce_temps: false,
    });

    assert_eq!(
        lowered.fidelity.status,
        Fidelity::Exact,
        "{:#?}",
        lowered.fidelity.issues
    );
    let (payload_name, payload) = lowered
        .symbols
        .iter()
        .find(|(_, symbol)| symbol.origin == SymbolOrigin::ExceptionPayload)
        .expect("handler payload symbol");
    assert_eq!(payload.value_type, ValueType::Any);
    let rendered = render_block(&lowered.body, 0);
    assert!(
        rendered.contains(&format!("catch({payload_name})")),
        "{rendered}"
    );
    assert!(!rendered.contains('?'), "{rendered}");
}

#[test]
fn phi_assignments_refine_common_value_types() {
    let body = Block::from(vec![Stmt::ControlFlow(Box::new(ControlFlow::if_else(
        Expr::var("condition"),
        Block::from(vec![Stmt::assign("p3_0", Expr::int(1))]),
        Block::from(vec![Stmt::assign("p3_0", Expr::int(2))]),
    )))]);
    let mut symbols = BTreeMap::from([
        (
            "condition".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Parameter(0),
                value_type: ValueType::Boolean,
            },
        ),
        (
            "p3_0".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Phi,
                value_type: ValueType::Unknown,
            },
        ),
    ]);

    register_structured_temporaries(&body, &mut symbols);

    assert_eq!(symbols["p3_0"].value_type, ValueType::Integer);
}

#[test]
fn phi_assignments_keep_conflicting_value_types_dynamic() {
    let body = Block::from(vec![Stmt::ControlFlow(Box::new(ControlFlow::if_else(
        Expr::var("condition"),
        Block::from(vec![Stmt::assign("p3_0", Expr::int(1))]),
        Block::from(vec![Stmt::assign(
            "p3_0",
            Expr::Literal(Literal::String("bytes".to_string())),
        )]),
    )))]);
    let mut symbols = BTreeMap::from([
        (
            "condition".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Parameter(0),
                value_type: ValueType::Boolean,
            },
        ),
        (
            "p3_0".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Phi,
                value_type: ValueType::Unknown,
            },
        ),
    ]);

    register_structured_temporaries(&body, &mut symbols);

    assert_eq!(symbols["p3_0"].value_type, ValueType::Any);
}

#[test]
fn local_and_static_assignments_refine_only_unanimous_types() {
    let body = Block::from(vec![
        Stmt::assign("loc0", Expr::int(1)),
        Stmt::assign("loc0", Expr::int(2)),
        Stmt::assign(
            "static0",
            Expr::Literal(Literal::String("value".to_string())),
        ),
        Stmt::assign(
            "static0",
            Expr::Literal(Literal::String("other".to_string())),
        ),
    ]);
    let mut symbols = BTreeMap::from([
        (
            "loc0".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Local(0),
                value_type: ValueType::Unknown,
            },
        ),
        (
            "static0".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Static(0),
                value_type: ValueType::Unknown,
            },
        ),
    ]);

    register_structured_temporaries(&body, &mut symbols);

    assert_eq!(symbols["loc0"].value_type, ValueType::Integer);
    assert_eq!(symbols["static0"].value_type, ValueType::ByteString);
}

#[test]
fn local_assignments_with_conflicting_or_unknown_paths_stay_dynamic() {
    let body = Block::from(vec![
        Stmt::assign("conflicting", Expr::int(1)),
        Stmt::assign("conflicting", Expr::Literal(Literal::Bool(true))),
        Stmt::assign("unknown_path", Expr::int(1)),
        Stmt::assign("unknown_path", Expr::Unknown),
        Stmt::assign("nullable", Expr::Literal(Literal::Null)),
        Stmt::assign("nullable", Expr::int(1)),
    ]);
    let mut symbols = BTreeMap::from([
        (
            "conflicting".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Local(0),
                value_type: ValueType::Unknown,
            },
        ),
        (
            "unknown_path".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Local(1),
                value_type: ValueType::Unknown,
            },
        ),
        (
            "nullable".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Local(2),
                value_type: ValueType::Unknown,
            },
        ),
    ]);

    register_structured_temporaries(&body, &mut symbols);

    assert_eq!(symbols["conflicting"].value_type, ValueType::Any);
    assert_eq!(symbols["unknown_path"].value_type, ValueType::Any);
    assert_eq!(symbols["nullable"].value_type, ValueType::Any);
}

#[test]
fn pusha_literal_values_remain_pointer_typed_for_csharp_refinement() {
    let body = Block::from(vec![Stmt::assign("loc0", Expr::int(52))]);
    let mut symbols = BTreeMap::from([(
        "loc0".to_string(),
        SymbolInfo {
            origin: SymbolOrigin::Local(0),
            value_type: ValueType::Unknown,
        },
    )]);

    register_structured_temporaries_with_call_types(
        &body,
        &mut symbols,
        &BTreeMap::new(),
        &BTreeSet::from([52]),
    );

    assert_eq!(symbols["loc0"].value_type, ValueType::Pointer);
}

#[test]
fn source_map_unions_offsets_for_folded_return() {
    let instructions = vec![
        instruction(0, OpCode::Push1),
        instruction(1, OpCode::Push1),
        instruction(2, OpCode::Add),
        instruction(3, OpCode::Ret),
    ];
    let lowered = lower_method_body(MethodIrRequest {
        start: 0,
        end: 4,
        instructions: &instructions,
        context: MethodContext {
            returns_value: Some(true),
            ..MethodContext::default()
        },
        symbol_types: MethodSymbolTypes::default(),
        reduce_temps: false,
    });
    assert_eq!(lowered.fidelity.status, Fidelity::Exact);
    assert_eq!(
        lowered.source_map.statement_origins.get(&StatementId(0)),
        Some(&BTreeSet::from([0, 1, 2, 3]))
    );
}

#[test]
fn rejects_an_oversized_slice_before_cfg_construction() {
    let instructions: Vec<_> = (0..=MAX_HIGH_LEVEL_METHOD_INSTRUCTIONS)
        .map(|offset| instruction(offset, OpCode::Nop))
        .collect();
    let request = MethodIrRequest {
        start: 0,
        end: instructions.len(),
        instructions: &instructions,
        context: MethodContext::default(),
        symbol_types: MethodSymbolTypes::default(),
        reduce_temps: false,
    };

    let lowered = lower_method_body(request);

    assert!(lowered.body.is_empty());
    assert_eq!(lowered.fidelity.status, Fidelity::Incomplete);
    assert_eq!(lowered.fidelity.instruction_count, instructions.len());
    assert!(lowered.fidelity.covered_offsets.is_empty());
    assert!(lowered.fidelity.issues.iter().any(|issue| {
        issue.offset == 0
            && issue.opcode == OpCode::Nop
            && issue.kind == LoweringIssueKind::BudgetExceeded
            && issue.fidelity == Fidelity::Incomplete
    }));
}

#[test]
fn unknown_merge_value_keeps_the_method_incomplete() {
    let instructions = vec![
        instruction(0, OpCode::Push1),
        Instruction::new(1, OpCode::Jmpif, Some(Operand::Jump(4))),
        instruction(3, OpCode::Push1),
        Instruction::new(4, OpCode::Jmp, Some(Operand::Jump(2))),
        instruction(5, OpCode::Nop),
        instruction(6, OpCode::Ret),
    ];
    let request = MethodIrRequest {
        start: 0,
        end: 7,
        instructions: &instructions,
        context: MethodContext {
            returns_value: Some(true),
            ..MethodContext::default()
        },
        symbol_types: MethodSymbolTypes::default(),
        reduce_temps: false,
    };

    let lowered = lower_method_body(request);

    assert_eq!(lowered.fidelity.status, Fidelity::Incomplete);
    assert!(lowered
        .fidelity
        .issues
        .iter()
        .any(|issue| issue.kind == LoweringIssueKind::LostStackValue));
}

#[test]
fn preserves_unknown_return_behavior() {
    let instructions = vec![instruction(0, OpCode::Push1), instruction(1, OpCode::Ret)];
    let lowered = lower_method_body(MethodIrRequest {
        start: 0,
        end: 2,
        instructions: &instructions,
        context: MethodContext::default(),
        symbol_types: MethodSymbolTypes::default(),
        reduce_temps: false,
    });

    assert_eq!(lowered.return_behavior, ReturnBehavior::Unknown);
}
