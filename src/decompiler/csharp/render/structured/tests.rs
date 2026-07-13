use std::collections::BTreeMap;

use crate::decompiler::analysis::call_graph::{CallEdge, CallGraph, CallTarget};
use crate::decompiler::analysis::method_contracts::{
    MethodContract, MethodContracts, ReturnBehavior,
};
use crate::decompiler::analysis::types::{TypeInfo, ValueType};
use crate::decompiler::analysis::MethodRef;
use crate::decompiler::cfg::method_body::{LoweringIssueKind, SymbolInfo, SymbolOrigin};
use crate::decompiler::ir::{
    BinOp, Block, ControlFlow, Expr, Intrinsic, Literal, SemanticCallTarget, Stmt, UnaryOp,
};
use crate::decompiler::output_format::RenderOptions;
use crate::instruction::{Instruction, OpCode, Operand};
use crate::manifest::ContractManifest;
use crate::nef::{NefFile, NefHeader};

use super::expr::{known_syscall_is_classified, render_expr, ExprContext};
use super::plan::{
    build_csharp_method_plans, plan_contract_symbols, plan_declarations, DeclarationKind,
};
use super::stmt::{render_block, terminates};

fn method_contract(
    offset: usize,
    name: &str,
    argument_count: usize,
    return_behavior: ReturnBehavior,
) -> MethodContract {
    MethodContract {
        method: MethodRef {
            offset,
            name: name.to_string(),
        },
        argument_count,
        return_behavior,
        may_return: true,
    }
}

fn expr_context_with_types(types: &[(&str, ValueType)]) -> ExprContext {
    let symbols = types
        .iter()
        .map(|(name, value_type)| {
            (
                (*name).to_string(),
                SymbolInfo {
                    origin: SymbolOrigin::Local(0),
                    value_type: *value_type,
                },
            )
        })
        .collect();
    ExprContext::for_block(&Block::new(), &symbols, false)
}

#[test]
fn plans_overloads_and_calls_together() {
    let manifest = ContractManifest::from_json_str(
        r#"{
            "name": "Overloads",
            "abi": { "methods": [
                {
                    "name": "transfer",
                    "parameters": [{ "name": "value", "type": "Integer" }],
                    "returntype": "Integer",
                    "offset": 0
                },
                {
                    "name": "transfer",
                    "parameters": [{ "name": "enabled", "type": "Boolean" }],
                    "returntype": "Integer",
                    "offset": 20
                },
                {
                    "name": "transfer",
                    "parameters": [{ "name": "value", "type": "Integer" }],
                    "returntype": "Integer",
                    "offset": 40
                }
            ] }
        }"#,
    )
    .expect("manifest parsed");
    let instructions = vec![
        Instruction::new(0, OpCode::Nop, None),
        Instruction::new(20, OpCode::Nop, None),
        Instruction::new(40, OpCode::Nop, None),
        Instruction::new(42, OpCode::Call, Some(Operand::Jump(-2))),
        Instruction::new(44, OpCode::Ret, None),
    ];
    let call_graph = CallGraph {
        methods: vec![
            MethodRef {
                offset: 0,
                name: "transfer".to_string(),
            },
            MethodRef {
                offset: 20,
                name: "transfer".to_string(),
            },
            MethodRef {
                offset: 40,
                name: "transfer".to_string(),
            },
        ],
        edges: vec![CallEdge {
            caller: MethodRef {
                offset: 40,
                name: "transfer".to_string(),
            },
            call_offset: 42,
            opcode: "CALL".to_string(),
            target: CallTarget::Internal {
                method: MethodRef {
                    offset: 40,
                    name: "transfer".to_string(),
                },
            },
        }],
    };
    let method_contracts = MethodContracts {
        methods: vec![
            method_contract(0, "transfer", 1, ReturnBehavior::Value),
            method_contract(20, "transfer", 1, ReturnBehavior::Value),
            method_contract(40, "transfer", 1, ReturnBehavior::Value),
        ],
    };

    let plans = build_csharp_method_plans(
        &instructions,
        Some(&manifest),
        &call_graph,
        &method_contracts,
        &TypeInfo::default(),
        &[0, 20, 40],
    );

    assert_eq!(plans[0].emitted_name, "transfer");
    assert_eq!(plans[1].emitted_name, "transfer");
    assert_eq!(plans[2].emitted_name, "transfer_2");
    assert_eq!(plans.method_return_types_by_offset()[&40], "BigInteger");
    assert_eq!(
        plans[2].method_context.calls_by_offset[&42]
            .target
            .display_name(),
        "transfer_2"
    );

    let ambiguous_manifest = ContractManifest::from_json_str(
        r#"{
            "name": "Ambiguous",
            "abi": { "methods": [
                {
                    "name": "caller",
                    "parameters": [],
                    "returntype": "Void",
                    "offset": 0
                },
                {
                    "name": "left",
                    "parameters": [],
                    "returntype": "Void",
                    "offset": 20
                },
                {
                    "name": "right",
                    "parameters": [],
                    "returntype": "Void",
                    "offset": 20
                }
            ] }
        }"#,
    )
    .expect("manifest parsed");
    let ambiguous_instructions = vec![
        Instruction::new(0, OpCode::Call, Some(Operand::Jump(20))),
        Instruction::new(2, OpCode::Ret, None),
        Instruction::new(20, OpCode::Ret, None),
    ];
    let ambiguous_call_graph = CallGraph {
        methods: vec![
            MethodRef {
                offset: 0,
                name: "caller".to_string(),
            },
            MethodRef {
                offset: 20,
                name: "left".to_string(),
            },
        ],
        edges: vec![CallEdge {
            caller: MethodRef {
                offset: 0,
                name: "caller".to_string(),
            },
            call_offset: 0,
            opcode: "CALL".to_string(),
            target: CallTarget::Internal {
                method: MethodRef {
                    offset: 20,
                    name: "left".to_string(),
                },
            },
        }],
    };
    let ambiguous_contracts = MethodContracts {
        methods: vec![
            method_contract(0, "caller", 0, ReturnBehavior::Void),
            method_contract(20, "left", 0, ReturnBehavior::Void),
        ],
    };
    let ambiguous_plans = build_csharp_method_plans(
        &ambiguous_instructions,
        Some(&ambiguous_manifest),
        &ambiguous_call_graph,
        &ambiguous_contracts,
        &TypeInfo::default(),
        &[0, 20],
    );
    let ambiguous = &ambiguous_plans[0];

    assert!(matches!(
        ambiguous.method_context.calls_by_offset[&0].target,
        SemanticCallTarget::Unresolved { .. }
    ));
    assert!(!ambiguous_plans
        .method_return_types_by_offset()
        .contains_key(&20));
    assert!(ambiguous
        .planning_issues
        .iter()
        .any(|issue| issue.kind == LoweringIssueKind::UnresolvedCall));
}

#[test]
fn null_checked_value_parameters_use_dynamic_csharp_signatures() {
    let manifest = ContractManifest::from_json_str(
        r#"{
            "name": "NullableParameter",
            "abi": { "methods": [{
                "name": "valueOrDefault",
                "parameters": [{ "name": "value", "type": "Integer" }],
                "returntype": "Integer",
                "offset": 0
            }] }
        }"#,
    )
    .expect("manifest parsed");
    let instructions = vec![
        Instruction::new(0, OpCode::Initslot, Some(Operand::Bytes(vec![0, 1]))),
        Instruction::new(3, OpCode::Ldarg0, None),
        Instruction::new(4, OpCode::Dup, None),
        Instruction::new(5, OpCode::Isnull, None),
        Instruction::new(6, OpCode::Ret, None),
    ];

    let plans = build_csharp_method_plans(
        &instructions,
        Some(&manifest),
        &CallGraph::default(),
        &MethodContracts::default(),
        &TypeInfo::default(),
        &[0],
    );

    assert_eq!(plans.manifest_method(0).parameters[0].ty, "dynamic");
}

#[test]
fn plans_declarations() {
    let body = Block::with_stmts(vec![
        Stmt::assign("t_0", Expr::int(1)),
        Stmt::expr(Expr::var("t_0")),
        Stmt::ControlFlow(Box::new(ControlFlow::if_else(
            Expr::var("cond"),
            Block::with_stmts(vec![Stmt::assign("loc0", Expr::int(1))]),
            Block::with_stmts(vec![Stmt::assign("loc0", Expr::int(2))]),
        ))),
        Stmt::expr(Expr::var("loc0")),
        Stmt::assign("loc1", Expr::int(0)),
        Stmt::ControlFlow(Box::new(ControlFlow::while_loop(
            Expr::binary(BinOp::Lt, Expr::var("loc1"), Expr::int(3)),
            Block::with_stmts(vec![Stmt::assign(
                "loc1",
                Expr::binary(BinOp::Add, Expr::var("loc1"), Expr::int(1)),
            )]),
        ))),
        Stmt::expr(Expr::var("loc1")),
        Stmt::assign("static1", Expr::var("loc0")),
        Stmt::expr(Expr::var("static1")),
        Stmt::expr(Expr::var("@class")),
        Stmt::expr(Expr::var("missing")),
    ]);
    let symbols = BTreeMap::from([
        (
            "cond".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Parameter(0),
                value_type: ValueType::Boolean,
            },
        ),
        (
            "@class".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Parameter(1),
                value_type: ValueType::Any,
            },
        ),
        (
            "loc0".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Local(0),
                value_type: ValueType::Integer,
            },
        ),
        (
            "loc1".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Local(1),
                value_type: ValueType::Integer,
            },
        ),
        (
            "static1".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Static(1),
                value_type: ValueType::Integer,
            },
        ),
        (
            "t_0".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Integer,
            },
        ),
        (
            "missing".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Unknown,
            },
        ),
        (
            "static3".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Static(3),
                value_type: ValueType::Boolean,
            },
        ),
    ]);

    let plan = plan_declarations(&body, &symbols, true);
    let root_scope = plan.scopes.root();

    assert_eq!(plan.declarations["t_0"].scope, root_scope);
    assert_eq!(plan.declarations["t_0"].kind, DeclarationKind::Inline);
    assert_eq!(plan.declarations["loc0"].scope, root_scope);
    assert_eq!(
        plan.declarations["loc0"].kind,
        DeclarationKind::HoistedAssignment
    );
    assert_eq!(plan.declarations["loc1"].scope, root_scope);
    assert_eq!(
        plan.declarations["loc1"].kind,
        DeclarationKind::HoistedAssignment
    );
    assert!(!plan.declarations.contains_key("cond"));
    assert!(!plan.declarations.contains_key("@class"));
    assert!(!plan.declarations.contains_key("static1"));
    assert!(plan.issues.iter().any(|issue| {
        issue.kind == LoweringIssueKind::LostStackValue && issue.detail.contains("missing")
    }));

    let types = TypeInfo {
        statics: vec![ValueType::Unknown, ValueType::Integer],
        ..TypeInfo::default()
    };
    let contract = plan_contract_symbols(&types, &[&symbols], true);
    assert_eq!(contract.static_fields[1].name, "static1");
    assert_eq!(contract.static_fields[1].csharp_type, "BigInteger");
    let static3 = contract
        .static_fields
        .iter()
        .find(|field| field.name == "static3")
        .expect("referenced static beyond TypeInfo is planned");
    assert_eq!(static3.csharp_type, "bool");
}

#[test]
fn for_body_definition_used_by_update_is_hoisted_to_the_loop_scope() {
    let body = Block::with_stmts(vec![Stmt::ControlFlow(Box::new(ControlFlow::for_loop(
        None,
        Some(Expr::int(1)),
        Some(Expr::var("body_value")),
        Block::with_stmts(vec![Stmt::assign("body_value", Expr::int(1))]),
    )))]);
    let symbols = BTreeMap::from([(
        "body_value".to_string(),
        SymbolInfo {
            origin: SymbolOrigin::Temporary,
            value_type: ValueType::Integer,
        },
    )]);

    let plan = plan_declarations(&body, &symbols, true);
    let declaration = &plan.declarations["body_value"];

    assert_ne!(declaration.scope, plan.scopes.root());
    assert_eq!(declaration.kind, DeclarationKind::HoistedAssignment);
}

#[test]
fn missing_uninitialized_symbol_is_a_lost_stack_value() {
    let plan = plan_declarations(
        &Block::with_stmts(vec![Stmt::expr(Expr::var("orphan"))]),
        &BTreeMap::new(),
        true,
    );

    assert!(plan.issues.iter().any(|issue| {
        issue.kind == LoweringIssueKind::LostStackValue && issue.detail.contains("orphan")
    }));
    assert!(!plan
        .issues
        .iter()
        .any(|issue| issue.kind == LoweringIssueKind::MissingProvenance));
}

#[test]
fn stack_placeholder_is_a_lost_stack_value() {
    let plan = plan_declarations(
        &Block::with_stmts(vec![Stmt::expr(Expr::StackTemp(7))]),
        &BTreeMap::new(),
        true,
    );

    assert!(plan.issues.iter().any(|issue| {
        issue.kind == LoweringIssueKind::LostStackValue && issue.detail.contains('7')
    }));
    assert!(plan.declarations.is_empty());
}

#[test]
fn csharp_emits_static_referenced_beyond_type_info() {
    let instructions = vec![
        Instruction::new(0, OpCode::Ldsfld3, None),
        Instruction::new(1, OpCode::Ret, None),
    ];
    let nef = NefFile {
        header: NefHeader {
            magic: *b"NEF3",
            compiler: "test".to_string(),
            source: String::new(),
        },
        method_tokens: Vec::new(),
        script: vec![0x5B, 0x40],
        checksum: 0,
    };
    let rendered = super::super::render_csharp(
        &nef,
        &instructions,
        None,
        &CallGraph::default(),
        &MethodContracts::default(),
        &TypeInfo::default(),
        &RenderOptions {
            typed_declarations: true,
            ..RenderOptions::default()
        },
    )
    .source;

    assert!(
        rendered.contains("private static dynamic static3;"),
        "referenced static beyond TypeInfo must be emitted at class scope: {rendered}"
    );
}

#[test]
fn referenced_static_beyond_type_info_reserves_the_method_name() {
    let instructions = vec![
        Instruction::new(0, OpCode::Ldsfld3, None),
        Instruction::new(1, OpCode::Ret, None),
    ];
    let nef = NefFile {
        header: NefHeader {
            magic: *b"NEF3",
            compiler: "test".to_string(),
            source: String::new(),
        },
        method_tokens: Vec::new(),
        script: vec![0x5B, 0x40],
        checksum: 0,
    };
    let manifest = ContractManifest::from_json_str(
        r#"{
            "name": "StaticCollision",
            "abi": { "methods": [{
                "name": "static3",
                "parameters": [],
                "returntype": "Any",
                "offset": 0
            }] }
        }"#,
    )
    .expect("manifest parsed");
    let rendered = super::super::render_csharp(
        &nef,
        &instructions,
        Some(&manifest),
        &CallGraph::default(),
        &MethodContracts::default(),
        &TypeInfo::default(),
        &RenderOptions {
            typed_declarations: true,
            ..RenderOptions::default()
        },
    )
    .source;

    assert!(rendered.contains("private static dynamic static3;"));
    assert!(
        rendered.contains("public static object static3_1()"),
        "referenced static names must be reserved before methods are named: {rendered}"
    );
}

#[test]
fn static_fields_reserve_contract_member_names() {
    let manifest = ContractManifest::from_json_str(
        r#"{
            "name": "StaticCollision",
            "abi": { "methods": [{
                "name": "static0",
                "parameters": [],
                "returntype": "Void",
                "offset": 0
            }] }
        }"#,
    )
    .expect("manifest parsed");
    let types = TypeInfo {
        statics: vec![ValueType::Integer],
        ..TypeInfo::default()
    };

    let plans = build_csharp_method_plans(
        &[Instruction::new(0, OpCode::Ret, None)],
        Some(&manifest),
        &CallGraph::default(),
        &MethodContracts::default(),
        &types,
        &[0],
    );

    assert_eq!(plans[0].emitted_name, "static0_1");
}

#[test]
fn renders_all_expression_variants() {
    let context = ExprContext::default();
    let cases = vec![
        (Expr::int(42), "42"),
        (
            Expr::Literal(Literal::BigInt("18446744073709551616".to_string())),
            "BigInteger.Parse(\"18446744073709551616\")",
        ),
        (Expr::Literal(Literal::Bool(true)), "true"),
        (
            Expr::Literal(Literal::String("quote \" slash \\ tab\t nul\0".to_string())),
            "\"quote \\\" slash \\\\ tab\\t nul\\0\"",
        ),
        (
            Expr::Literal(Literal::Bytes(vec![0, 255])),
            "(ByteString)new byte[] { 0x00, 0xFF }",
        ),
        (Expr::Literal(Literal::Null), "null"),
        (Expr::Unknown, "(dynamic)null"),
        (Expr::var("@class"), "@class"),
        (Expr::index(Expr::var("items"), Expr::int(1)), "items[1]"),
        (
            Expr::Member {
                base: Box::new(Expr::var("items")),
                name: "Count".to_string(),
            },
            "items.Count",
        ),
        (
            Expr::Member {
                base: Box::new(Expr::Literal(Literal::Bytes(vec![1]))),
                name: "Length".to_string(),
            },
            "((ByteString)new byte[] { 0x01 }).Length",
        ),
        (
            Expr::Member {
                base: Box::new(Expr::Unknown),
                name: "Length".to_string(),
            },
            "((dynamic)null).Length",
        ),
        (
            Expr::Cast {
                expr: Box::new(Expr::binary(BinOp::Add, Expr::var("a"), Expr::var("b"))),
                target_type: "BigInteger".to_string(),
            },
            "(BigInteger)(a + b)",
        ),
        (
            Expr::Convert {
                value: Box::new(Expr::var("value")),
                target: ValueType::Integer,
            },
            "__NeoDecompilerConvertInteger(value)",
        ),
        (
            Expr::IsType {
                value: Box::new(Expr::var("value")),
                target: ValueType::Integer,
            },
            "__NeoDecompilerIsTypeInteger(value)",
        ),
        (
            Expr::NewArray {
                length: Box::new(Expr::int(2)),
                element_type: Some(ValueType::Integer),
            },
            "new BigInteger[(int)(2)]",
        ),
        (
            Expr::Array(vec![Expr::int(1), Expr::int(2)]),
            "new object[] { 1, 2 }",
        ),
        (
            Expr::Struct(vec![Expr::int(1), Expr::int(2)]),
            "__NeoDecompilerConvertStruct(new object[] { 1, 2 })",
        ),
        (
            Expr::Map(vec![(
                Expr::Literal(Literal::String("key".to_string())),
                Expr::int(1),
            )]),
            "new Map<object, object> { [\"key\"] = 1 }",
        ),
        (
            Expr::Ternary {
                condition: Box::new(Expr::var("condition")),
                then_expr: Box::new(Expr::int(1)),
                else_expr: Box::new(Expr::int(2)),
            },
            "condition ? 1 : 2",
        ),
        (Expr::StackTemp(7), "_tmp7"),
    ];
    for (expression, expected) in cases {
        assert_eq!(
            render_expr(&expression, &context),
            expected,
            "{expression:?}"
        );
    }

    let binary_cases = [
        (BinOp::Add, "a + b"),
        (BinOp::Sub, "a - b"),
        (BinOp::Mul, "a * b"),
        (BinOp::Div, "a / b"),
        (BinOp::Mod, "a % b"),
        (BinOp::Pow, "BigInteger.Pow(a, (int)(b))"),
        (BinOp::And, "a & b"),
        (BinOp::Or, "a | b"),
        (BinOp::Xor, "a ^ b"),
        (BinOp::Shl, "a << (int)(b)"),
        (BinOp::Shr, "a >> (int)(b)"),
        (
            BinOp::Eq,
            "(bool)Runtime.LoadScript((ByteString)new byte[] { 0x97 }, CallFlags.All, new object[] { a, b })",
        ),
        (
            BinOp::Ne,
            "(bool)Runtime.LoadScript((ByteString)new byte[] { 0x98 }, CallFlags.All, new object[] { a, b })",
        ),
        (BinOp::Lt, "a < b"),
        (BinOp::Le, "a <= b"),
        (BinOp::Gt, "a > b"),
        (BinOp::Ge, "a >= b"),
        (
            BinOp::LogicalAnd,
            "(bool)Runtime.LoadScript((ByteString)new byte[] { 0xAB }, CallFlags.All, new object[] { a, b })",
        ),
        (
            BinOp::LogicalOr,
            "(bool)Runtime.LoadScript((ByteString)new byte[] { 0xAC }, CallFlags.All, new object[] { a, b })",
        ),
    ];
    for (operator, expected) in binary_cases {
        let expression = Expr::binary(operator, Expr::var("a"), Expr::var("b"));
        assert_eq!(render_expr(&expression, &context), expected, "{operator:?}");
    }

    let unary_cases = [
        (UnaryOp::Neg, "-value"),
        (UnaryOp::Not, "~value"),
        (UnaryOp::LogicalNot, "!(bool)(object)(value)"),
        (UnaryOp::Inc, "value + 1"),
        (UnaryOp::Dec, "value - 1"),
        (UnaryOp::Abs, "BigInteger.Abs(value)"),
        (UnaryOp::Sign, "value.Sign"),
    ];
    for (operator, expected) in unary_cases {
        let expression = Expr::unary(operator, Expr::var("value"));
        assert_eq!(render_expr(&expression, &context), expected, "{operator:?}");
    }

    let intrinsic_cases = [
        (OpCode::Within, 3, "Helper.Within(a, b, c)"),
        (
            OpCode::Substr,
            3,
            "Runtime.LoadScript((ByteString)new byte[] { 0x8C }, CallFlags.All, new object[] { a, b, c })",
        ),
        (OpCode::Modmul, 3, "Helper.ModMultiply(a, b, c)"),
        (OpCode::Modpow, 3, "BigInteger.ModPow(a, b, c)"),
        (OpCode::Sqrt, 1, "Helper.Sqrt(a)"),
        (OpCode::Nz, 1, "(BigInteger)(dynamic)(a) != 0"),
        (
            OpCode::Size,
            1,
            "(BigInteger)Runtime.LoadScript((ByteString)new byte[] { 0xCA }, CallFlags.All, new object[] { a })",
        ),
        (OpCode::Keys, 1, "a.Keys"),
        (OpCode::Values, 1, "a.Values"),
        (OpCode::Isnull, 1, "a is null"),
        (OpCode::Newbuffer, 1, "new byte[(int)(a)]"),
        (
            OpCode::Cat,
            2,
            "Runtime.LoadScript((ByteString)new byte[] { 0x8B }, CallFlags.All, new object[] { a, b })",
        ),
        (
            OpCode::Left,
            2,
            "Runtime.LoadScript((ByteString)new byte[] { 0x8D }, CallFlags.All, new object[] { a, b })",
        ),
        (
            OpCode::Right,
            2,
            "Runtime.LoadScript((ByteString)new byte[] { 0x8E }, CallFlags.All, new object[] { a, b })",
        ),
        (OpCode::Min, 2, "BigInteger.Min(a, b)"),
        (OpCode::Max, 2, "BigInteger.Max(a, b)"),
        (OpCode::Newarray0, 0, "new object[0]"),
        (OpCode::Newarray, 1, "new object[(int)(a)]"),
        (OpCode::NewarrayT, 1, "new object[(int)(a)]"),
        (OpCode::Newstruct0, 0, "new object[] { }"),
        (OpCode::Newstruct, 1, "new object[(int)(a)]"),
        (OpCode::Newmap, 0, "new Map<object, object>()"),
        (OpCode::Haskey, 2, "a.HasKey(b)"),
        (OpCode::Pickitem, 2, "((dynamic)(a))[b]"),
        (OpCode::Setitem, 3, "((dynamic)(a))[b] = c"),
        (
            OpCode::Append,
            2,
            "((Neo.SmartContract.Framework.List<object>)a).Add(b)",
        ),
        (
            OpCode::Remove,
            2,
            "Runtime.LoadScript((ByteString)new byte[] { 0xD2 }, CallFlags.All, new object[] { a, b })",
        ),
        (
            OpCode::Clearitems,
            1,
            "Runtime.LoadScript((ByteString)new byte[] { 0xD3 }, CallFlags.All, new object[] { a })",
        ),
        (OpCode::Reverseitems, 1, "Helper.Reverse(a)"),
        (
            OpCode::Popitem,
            1,
            "((Neo.SmartContract.Framework.List<object>)a).PopItem()",
        ),
        (
            OpCode::Memcpy,
            5,
            "Runtime.LoadScript((ByteString)new byte[] { 0x89 }, CallFlags.All, new object[] { a, b, c, d, e })",
        ),
        (OpCode::Convert, 1, "(object)(a)"),
        (OpCode::Istype, 1, "a is object"),
    ];
    for (opcode, argument_count, expected) in intrinsic_cases {
        let args = ["a", "b", "c", "d", "e"]
            .into_iter()
            .take(argument_count)
            .map(Expr::var)
            .collect();
        let expression = Expr::call(
            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(opcode)),
            args,
        );
        assert_eq!(render_expr(&expression, &context), expected, "{opcode:?}");
    }

    let user_append_call = Expr::call(
        SemanticCallTarget::Internal {
            offset: 12,
            name: "append".to_string(),
        },
        vec![Expr::var("items")],
    );
    let token_call = Expr::call(
        SemanticCallTarget::MethodToken {
            index: 2,
            name: "transfer".to_string(),
            hash_le: Some("00112233445566778899AABBCCDDEEFF00112233".to_string()),
            call_flags: Some(0x0F),
        },
        vec![Expr::var("items")],
    );
    let known_syscall = Expr::call(
        SemanticCallTarget::Syscall {
            hash: 0x8CEC_27F8,
            name: Some("not trusted for dispatch".to_string()),
        },
        vec![Expr::var("items")],
    );
    let unknown_syscall = Expr::call(
        SemanticCallTarget::Syscall {
            hash: 0xDEAD_BEEF,
            name: None,
        },
        vec![Expr::var("items")],
    );
    let vm_append_call = Expr::call(
        SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Append)),
        vec![Expr::var("items"), Expr::var("value")],
    );
    assert_eq!(render_expr(&user_append_call, &context), "append(items)");
    assert_eq!(
        render_expr(&token_call, &context),
        "(dynamic)Contract.Call((UInt160)new byte[] { 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x00, 0x11, 0x22, 0x33 }, \"transfer\", (CallFlags)0x0F, new object[] { items })"
    );
    assert_eq!(
        render_expr(&known_syscall, &context),
        "(bool)Runtime.LoadScript((ByteString)new byte[] { 0x41, 0xF8, 0x27, 0xEC, 0x8C }, CallFlags.All, new object[] { items })"
    );
    assert_eq!(
        render_expr(&unknown_syscall, &context),
        "Runtime.LoadScript((ByteString)new byte[] { 0x41, 0xEF, 0xBE, 0xAD, 0xDE }, CallFlags.All, new object[] { items })"
    );
    assert_eq!(
        render_expr(&vm_append_call, &context),
        "((Neo.SmartContract.Framework.List<object>)items).Add(value)"
    );
    assert_eq!(
        render_expr(
            &Expr::unresolved_call("call_0x1234", vec![Expr::var("items")]),
            &context,
        ),
        "__NeoDecompilerUnresolvedCall(\"call_0x1234\", new object[] { items })"
    );
}

#[test]
fn collection_intrinsics_use_the_receiver_container_type() {
    let context = expr_context_with_types(&[
        ("items", ValueType::Array),
        ("map", ValueType::Map),
        ("bytes", ValueType::Buffer),
        ("text", ValueType::ByteString),
        ("index", ValueType::Integer),
        ("value", ValueType::Integer),
    ]);
    let intrinsic = |opcode, args| {
        Expr::call(
            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(opcode)),
            args,
        )
    };

    let cases = [
        (
            intrinsic(OpCode::Size, vec![Expr::var("items")]),
            "items.Length",
        ),
        (intrinsic(OpCode::Size, vec![Expr::var("map")]), "map.Count"),
        (
            intrinsic(OpCode::Size, vec![Expr::var("bytes")]),
            "bytes.Length",
        ),
        (
            intrinsic(OpCode::Append, vec![Expr::var("items"), Expr::var("value")]),
            "((Neo.SmartContract.Framework.List<object>)items).Add(value)",
        ),
        (
            intrinsic(OpCode::Remove, vec![Expr::var("items"), Expr::var("index")]),
            "((Neo.SmartContract.Framework.List<object>)items).RemoveAt((int)(index))",
        ),
        (
            intrinsic(OpCode::Remove, vec![Expr::var("map"), Expr::var("key")]),
            "map.Remove(key)",
        ),
        (
            intrinsic(OpCode::Clearitems, vec![Expr::var("items")]),
            "((Neo.SmartContract.Framework.List<object>)items).Clear()",
        ),
        (
            intrinsic(OpCode::Clearitems, vec![Expr::var("map")]),
            "map.Clear()",
        ),
        (
            intrinsic(
                OpCode::Pickitem,
                vec![Expr::var("items"), Expr::var("index")],
            ),
            "items[(int)(index)]",
        ),
        (
            intrinsic(OpCode::Pickitem, vec![Expr::var("map"), Expr::var("key")]),
            "map[key]",
        ),
        (
            intrinsic(
                OpCode::Setitem,
                vec![Expr::var("bytes"), Expr::var("index"), Expr::var("value")],
            ),
            "bytes[(int)(index)] = (byte)(dynamic)(value)",
        ),
        (
            intrinsic(
                OpCode::Setitem,
                vec![Expr::var("items"), Expr::var("index"), Expr::var("value")],
            ),
            "items[(int)(index)] = value",
        ),
        (
            intrinsic(
                OpCode::Setitem,
                vec![Expr::var("text"), Expr::var("index"), Expr::var("value")],
            ),
            "((byte[])(text))[(int)(index)] = (byte)(dynamic)(value)",
        ),
    ];

    for (expression, expected) in cases {
        assert_eq!(
            render_expr(&expression, &context),
            expected,
            "{expression:?}"
        );
    }
}

#[test]
fn ambiguous_collection_intrinsics_use_low_level_wrappers() {
    let context = ExprContext::default();
    let intrinsic = |opcode, args| {
        Expr::call(
            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(opcode)),
            args,
        )
    };
    let cases = [
        (
            intrinsic(OpCode::Size, vec![Expr::var("container")]),
            "(BigInteger)Runtime.LoadScript((ByteString)new byte[] { 0xCA }, CallFlags.All, new object[] { container })",
        ),
        (
            intrinsic(
                OpCode::Remove,
                vec![Expr::var("container"), Expr::var("key")],
            ),
            "Runtime.LoadScript((ByteString)new byte[] { 0xD2 }, CallFlags.All, new object[] { container, key })",
        ),
        (
            intrinsic(OpCode::Clearitems, vec![Expr::var("container")]),
            "Runtime.LoadScript((ByteString)new byte[] { 0xD3 }, CallFlags.All, new object[] { container })",
        ),
    ];

    for (expression, expected) in cases {
        assert_eq!(
            render_expr(&expression, &context),
            expected,
            "{expression:?}"
        );
    }
}

#[test]
fn indexing_intrinsics_guard_unsupported_receivers() {
    let context =
        expr_context_with_types(&[("flag", ValueType::Boolean), ("items", ValueType::Any)]);
    let intrinsic = |opcode, args| {
        Expr::call(
            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(opcode)),
            args,
        )
    };
    let cases = [
        (
            intrinsic(OpCode::Pickitem, vec![Expr::var("flag"), Expr::var("key")]),
            "((dynamic)(flag))[key]",
        ),
        (
            intrinsic(OpCode::Pickitem, vec![Expr::var("items"), Expr::var("key")]),
            "((dynamic)(items))[key]",
        ),
        (
            intrinsic(OpCode::Pickitem, vec![Expr::Unknown, Expr::var("key")]),
            "((dynamic)((dynamic)null))[key]",
        ),
        (
            intrinsic(
                OpCode::Setitem,
                vec![Expr::var("flag"), Expr::var("key"), Expr::var("value")],
            ),
            "((dynamic)(flag))[key] = value",
        ),
    ];

    for (expression, expected) in cases {
        assert_eq!(
            render_expr(&expression, &context),
            expected,
            "{expression:?}"
        );
    }
}

#[test]
fn byte_intrinsics_use_framework_compatible_conversions() {
    let context = expr_context_with_types(&[
        ("text", ValueType::ByteString),
        ("buffer", ValueType::Buffer),
        ("destination", ValueType::Buffer),
        ("integer", ValueType::Integer),
        ("items", ValueType::Array),
    ]);
    let intrinsic = |opcode, args| {
        Expr::call(
            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(opcode)),
            args,
        )
    };
    let cases = [
        (
            intrinsic(
                OpCode::Cat,
                vec![Expr::var("text"), Expr::var("buffer")],
            ),
            "Helper.Concat((ByteString)(text), (ByteString)(buffer))",
        ),
        (
            intrinsic(
                OpCode::Cat,
                vec![Expr::var("buffer"), Expr::var("text")],
            ),
            "Helper.Concat((byte[])(buffer), (ByteString)(text))",
        ),
        (
            intrinsic(
                OpCode::Cat,
                vec![Expr::var("integer"), Expr::var("items")],
            ),
            "Helper.Concat((ByteString)(dynamic)(integer), (ByteString)(dynamic)(items))",
        ),
        (
            intrinsic(
                OpCode::Substr,
                vec![Expr::var("text"), Expr::var("start"), Expr::var("length")],
            ),
            "(ByteString)(Helper.Range((byte[])(ByteString)(text), (int)(start), (int)(length)))",
        ),
        (
            intrinsic(
                OpCode::Left,
                vec![Expr::var("text"), Expr::var("count")],
            ),
            "(ByteString)(Helper.Take((byte[])(ByteString)(text), (int)(count)))",
        ),
        (
            intrinsic(
                OpCode::Right,
                vec![Expr::var("buffer"), Expr::var("count")],
            ),
            "Helper.Last((byte[])(buffer), (int)(count))",
        ),
        (
            intrinsic(
                OpCode::Memcpy,
                vec![
                    Expr::var("destination"),
                    Expr::var("destination_index"),
                    Expr::var("text"),
                    Expr::var("source_index"),
                    Expr::var("count"),
                ],
            ),
            "Array.Copy((byte[])(text), (int)(source_index), (byte[])(destination), (int)(destination_index), (int)(count))",
        ),
    ];

    for (expression, expected) in cases {
        assert_eq!(
            render_expr(&expression, &context),
            expected,
            "{expression:?}"
        );
    }
}

#[test]
fn value_equality_uses_csharp_operators_only_for_known_value_types() {
    let context = expr_context_with_types(&[
        ("integer_left", ValueType::Integer),
        ("integer_right", ValueType::Integer),
        ("boolean_left", ValueType::Boolean),
        ("boolean_right", ValueType::Boolean),
        ("text", ValueType::ByteString),
        ("buffer", ValueType::Buffer),
        ("array", ValueType::Array),
        ("structure", ValueType::Struct),
        ("map", ValueType::Map),
        ("unknown", ValueType::Unknown),
    ]);
    let cases = [
        (
            Expr::binary(
                BinOp::Eq,
                Expr::var("integer_left"),
                Expr::var("integer_right"),
            ),
            "integer_left == integer_right",
        ),
        (
            Expr::binary(
                BinOp::Ne,
                Expr::var("boolean_left"),
                Expr::var("boolean_right"),
            ),
            "boolean_left != boolean_right",
        ),
        (
            Expr::binary(BinOp::Eq, Expr::var("text"), Expr::var("text")),
            "(bool)Runtime.LoadScript((ByteString)new byte[] { 0x97 }, CallFlags.All, new object[] { text, text })",
        ),
        (
            Expr::binary(BinOp::Ne, Expr::var("buffer"), Expr::var("buffer")),
            "(bool)Runtime.LoadScript((ByteString)new byte[] { 0x98 }, CallFlags.All, new object[] { buffer, buffer })",
        ),
        (
            Expr::binary(BinOp::Eq, Expr::var("array"), Expr::var("array")),
            "(bool)Runtime.LoadScript((ByteString)new byte[] { 0x97 }, CallFlags.All, new object[] { array, array })",
        ),
        (
            Expr::binary(
                BinOp::Ne,
                Expr::var("structure"),
                Expr::var("structure"),
            ),
            "(bool)Runtime.LoadScript((ByteString)new byte[] { 0x98 }, CallFlags.All, new object[] { structure, structure })",
        ),
        (
            Expr::binary(BinOp::Eq, Expr::var("map"), Expr::var("map")),
            "(bool)Runtime.LoadScript((ByteString)new byte[] { 0x97 }, CallFlags.All, new object[] { map, map })",
        ),
        (
            Expr::binary(
                BinOp::Ne,
                Expr::Literal(Literal::Null),
                Expr::Literal(Literal::Null),
            ),
            "(bool)Runtime.LoadScript((ByteString)new byte[] { 0x98 }, CallFlags.All, new object[] { null, null })",
        ),
        (
            Expr::binary(BinOp::Eq, Expr::var("unknown"), Expr::var("unknown")),
            "(bool)Runtime.LoadScript((ByteString)new byte[] { 0x97 }, CallFlags.All, new object[] { unknown, unknown })",
        ),
        (
            Expr::binary(BinOp::Ne, Expr::var("text"), Expr::var("buffer")),
            "(bool)Runtime.LoadScript((ByteString)new byte[] { 0x98 }, CallFlags.All, new object[] { text, buffer })",
        ),
    ];

    for (expression, expected) in cases {
        assert_eq!(
            render_expr(&expression, &context),
            expected,
            "{expression:?}"
        );
    }
}

#[test]
fn logical_not_uses_vm_truthiness_for_integer_operands() {
    let context = expr_context_with_types(&[("number", ValueType::Integer)]);

    assert_eq!(
        render_expr(
            &Expr::unary(UnaryOp::LogicalNot, Expr::var("number")),
            &context,
        ),
        "(BigInteger)(dynamic)(number) == 0"
    );
}

#[test]
fn isnull_is_false_for_non_nullable_value_types() {
    let context =
        expr_context_with_types(&[("number", ValueType::Integer), ("flag", ValueType::Boolean)]);
    for name in ["number", "flag"] {
        let expression = Expr::call(
            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Isnull)),
            vec![Expr::var(name)],
        );
        assert_eq!(render_expr(&expression, &context), "false");
    }
}

#[test]
fn numeric_operators_use_vm_wrappers_for_static_any_values() {
    let context = expr_context_with_types(&[
        ("left", ValueType::Any),
        ("right", ValueType::Any),
        ("integer_left", ValueType::Integer),
        ("integer_right", ValueType::Integer),
    ]);

    assert_eq!(
        render_expr(
            &Expr::binary(BinOp::Add, Expr::var("left"), Expr::var("right")),
            &context,
        ),
        "Runtime.LoadScript((ByteString)new byte[] { 0x9E }, CallFlags.All, new object[] { left, right })"
    );
    assert_eq!(
        render_expr(
            &Expr::binary(
                BinOp::Add,
                Expr::var("integer_left"),
                Expr::var("integer_right"),
            ),
            &context,
        ),
        "integer_left + integer_right"
    );
}

#[test]
fn vm_boolean_binary_operators_are_eager_only_for_known_booleans() {
    let context = expr_context_with_types(&[
        ("boolean_left", ValueType::Boolean),
        ("boolean_right", ValueType::Boolean),
        ("integer_left", ValueType::Integer),
        ("integer_right", ValueType::Integer),
        ("unknown", ValueType::Unknown),
    ]);
    let cases = [
        (
            Expr::binary(
                BinOp::LogicalAnd,
                Expr::var("boolean_left"),
                Expr::var("boolean_right"),
            ),
            "boolean_left & boolean_right",
        ),
        (
            Expr::binary(
                BinOp::LogicalOr,
                Expr::var("boolean_left"),
                Expr::var("boolean_right"),
            ),
            "boolean_left | boolean_right",
        ),
        (
            Expr::binary(
                BinOp::LogicalAnd,
                Expr::var("unknown"),
                Expr::var("unknown"),
            ),
            "(bool)Runtime.LoadScript((ByteString)new byte[] { 0xAB }, CallFlags.All, new object[] { unknown, unknown })",
        ),
        (
            Expr::binary(
                BinOp::LogicalOr,
                Expr::var("integer_left"),
                Expr::var("integer_right"),
            ),
            "(bool)Runtime.LoadScript((ByteString)new byte[] { 0xAC }, CallFlags.All, new object[] { integer_left, integer_right })",
        ),
        (
            Expr::binary(
                BinOp::LogicalAnd,
                Expr::var("boolean_left"),
                Expr::var("unknown"),
            ),
            "(bool)Runtime.LoadScript((ByteString)new byte[] { 0xAB }, CallFlags.All, new object[] { boolean_left, unknown })",
        ),
    ];

    for (expression, expected) in cases {
        assert_eq!(
            render_expr(&expression, &context),
            expected,
            "{expression:?}"
        );
    }
}

#[test]
fn syscall_rendering_uses_hash_identity_and_drops_display_metadata() {
    let context = ExprContext::default();
    let cases = [
        (
            Expr::call(
                SemanticCallTarget::Syscall {
                    hash: 0x8CEC_27F8,
                    name: Some("System.Runtime.CheckWitness".to_string()),
                },
                vec![
                    Expr::Literal(Literal::String(
                        "System.Runtime.CheckWitness".to_string(),
                    )),
                    Expr::var("account"),
                ],
            ),
            "(bool)Runtime.LoadScript((ByteString)new byte[] { 0x41, 0xF8, 0x27, 0xEC, 0x8C }, CallFlags.All, new object[] { account })",
        ),
        (
            Expr::call(
                SemanticCallTarget::Syscall {
                    hash: 0x0388_C3B7,
                    name: Some("ignored".to_string()),
                },
                vec![Expr::Literal(Literal::String(
                    "System.Runtime.GetTime".to_string(),
                ))],
            ),
            "Runtime.Time",
        ),
        (
            Expr::call(
                SemanticCallTarget::Syscall {
                    hash: 0xCE67_F69B,
                    name: None,
                },
                vec![Expr::Literal(Literal::String(
                    "System.Storage.GetContext".to_string(),
                ))],
            ),
            "Storage.CurrentContext",
        ),
        (
            Expr::call(
                SemanticCallTarget::Syscall {
                    hash: 0x9CED_089C,
                    name: None,
                },
                vec![
                    Expr::Literal(Literal::String("System.Iterator.Next".to_string())),
                    Expr::var("iterator"),
                ],
            ),
            "((Iterator)iterator).Next()",
        ),
        (
            Expr::call(
                SemanticCallTarget::Syscall {
                    hash: 0x616F_0195,
                    name: Some("System.Runtime.Notify".to_string()),
                },
                vec![
                    Expr::Literal(Literal::String("System.Runtime.Notify".to_string())),
                    Expr::var("event_name"),
                    Expr::var("state"),
                ],
            ),
            "Runtime.LoadScript((ByteString)new byte[] { 0x41, 0x95, 0x01, 0x6F, 0x61 }, CallFlags.All, new object[] { event_name, state })",
        ),
        (
            Expr::call(
                SemanticCallTarget::Syscall {
                    hash: 0xDEAD_BEEF,
                    name: None,
                },
                vec![Expr::Literal(Literal::String("0xDEADBEEF".to_string()))],
            ),
            "Runtime.LoadScript((ByteString)new byte[] { 0x41, 0xEF, 0xBE, 0xAD, 0xDE }, CallFlags.All, new object[] {  })",
        ),
    ];

    for (expression, expected) in cases {
        assert_eq!(
            render_expr(&expression, &context),
            expected,
            "{expression:?}"
        );
    }
}

#[test]
fn check_witness_requires_explicit_framework_overload_evidence() {
    let context = expr_context_with_types(&[
        ("account_bytes", ValueType::ByteString),
        ("unknown_account", ValueType::Unknown),
    ]);
    let check_witness = |argument| {
        Expr::call(
            SemanticCallTarget::Syscall {
                hash: 0x8CEC_27F8,
                name: Some("System.Runtime.CheckWitness".to_string()),
            },
            vec![argument],
        )
    };
    let cases = [
        (
            check_witness(Expr::var("account_bytes")),
            "(bool)Runtime.LoadScript((ByteString)new byte[] { 0x41, 0xF8, 0x27, 0xEC, 0x8C }, CallFlags.All, new object[] { account_bytes })",
        ),
        (
            check_witness(Expr::var("unknown_account")),
            "(bool)Runtime.LoadScript((ByteString)new byte[] { 0x41, 0xF8, 0x27, 0xEC, 0x8C }, CallFlags.All, new object[] { unknown_account })",
        ),
        (
            check_witness(Expr::Cast {
                expr: Box::new(Expr::var("account_bytes")),
                target_type: "UInt160".to_string(),
            }),
            "Runtime.CheckWitness((UInt160)(account_bytes))",
        ),
        (
            check_witness(Expr::Cast {
                expr: Box::new(Expr::var("account_bytes")),
                target_type: "ECPoint".to_string(),
            }),
            "Runtime.CheckWitness((ECPoint)(account_bytes))",
        ),
    ];

    for (expression, expected) in cases {
        assert_eq!(
            render_expr(&expression, &context),
            expected,
            "{expression:?}"
        );
    }
}

#[test]
fn syscall_arguments_match_framework_signatures() {
    let context = ExprContext::default();
    let syscall = |hash, name: &str, args: Vec<Expr>| {
        let mut with_metadata = vec![Expr::Literal(Literal::String(name.to_string()))];
        with_metadata.extend(args);
        Expr::call(
            SemanticCallTarget::Syscall {
                hash,
                name: Some(name.to_string()),
            },
            with_metadata,
        )
    };
    let cases = [
        (
            syscall(
                0x0287_99CF,
                "System.Contract.CreateStandardAccount",
                vec![Expr::var("pubkey")],
            ),
            "Contract.CreateStandardAccount((ECPoint)(pubkey))",
        ),
        (
            syscall(
                0x09E9_336A,
                "System.Contract.CreateMultisigAccount",
                vec![Expr::var("m"), Expr::var("pubkeys")],
            ),
            "Contract.CreateMultisigAccount((int)(m), (ECPoint[])(pubkeys))",
        ),
        (
            syscall(
                0x27B3_E756,
                "System.Crypto.CheckSig",
                vec![Expr::var("pubkey"), Expr::var("signature")],
            ),
            "Crypto.CheckSig((ECPoint)(pubkey), (ByteString)(signature))",
        ),
        (
            syscall(
                0x3ADC_D09E,
                "System.Crypto.CheckMultisig",
                vec![Expr::var("pubkeys"), Expr::var("signatures")],
            ),
            "Crypto.CheckMultisig((ECPoint[])(pubkeys), (ByteString[])(signatures))",
        ),
        (
            syscall(
                0x525B_7D62,
                "System.Contract.Call",
                vec![
                    Expr::var("script_hash"),
                    Expr::var("method"),
                    Expr::var("flags"),
                    Expr::var("arguments"),
                ],
            ),
            "Contract.Call((UInt160)(script_hash), (string)(method), (CallFlags)(int)(flags), (object[])(arguments))",
        ),
        (
            syscall(
                0xBC8C_5AC3,
                "System.Runtime.BurnGas",
                vec![Expr::var("amount")],
            ),
            "Runtime.BurnGas((long)(BigInteger)(amount))",
        ),
    ];

    for (expression, expected) in cases {
        assert_eq!(
            render_expr(&expression, &context),
            expected,
            "{expression:?}"
        );
    }
}

#[test]
fn syscall_metadata_is_removed_only_from_the_extra_selector_slot() {
    let expression = Expr::call(
        SemanticCallTarget::Syscall {
            hash: 0x9647_E7CF,
            name: Some("System.Runtime.Log".to_string()),
        },
        vec![Expr::Literal(Literal::String(
            "System.Runtime.Log".to_string(),
        ))],
    );

    assert_eq!(
        render_expr(&expression, &ExprContext::default()),
        "Runtime.Log((string)(\"System.Runtime.Log\"))"
    );
}

#[test]
fn storage_syscalls_select_overloads_from_neutral_types() {
    let context = expr_context_with_types(&[
        ("storage", ValueType::InteropInterface),
        ("key", ValueType::Buffer),
        ("value", ValueType::Integer),
    ]);
    let expression = Expr::call(
        SemanticCallTarget::Syscall {
            hash: 0x8418_3FE6,
            name: Some("System.Storage.Put".to_string()),
        },
        vec![
            Expr::Literal(Literal::String("System.Storage.Put".to_string())),
            Expr::var("storage"),
            Expr::var("key"),
            Expr::var("value"),
        ],
    );

    assert_eq!(
        render_expr(&expression, &context),
        "Storage.Put((StorageContext)(storage), (byte[])(key), (BigInteger)(value))"
    );
}

#[test]
fn every_known_syscall_has_an_explicit_csharp_policy() {
    for syscall in crate::syscalls::all() {
        assert!(
            known_syscall_is_classified(syscall.hash),
            "missing C# syscall policy for {} (0x{:08X})",
            syscall.name,
            syscall.hash
        );
    }
}

#[test]
fn unmodeled_intrinsic_uses_a_low_level_wrapper() {
    let expression = Expr::call(
        SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Ldloc0)),
        vec![],
    );

    assert_eq!(
        render_expr(&expression, &ExprContext::default()),
        "Runtime.LoadScript((ByteString)new byte[] { 0x68 }, CallFlags.All, new object[] {  })"
    );
}

#[test]
fn renders_expression_precedence_from_structure() {
    let context = ExprContext::default();
    let cases = [
        (
            Expr::binary(
                BinOp::Mul,
                Expr::binary(BinOp::Add, Expr::var("a"), Expr::var("b")),
                Expr::var("c"),
            ),
            "(a + b) * c",
        ),
        (
            Expr::binary(
                BinOp::Add,
                Expr::var("a"),
                Expr::binary(BinOp::Mul, Expr::var("b"), Expr::var("c")),
            ),
            "a + b * c",
        ),
        (
            Expr::binary(
                BinOp::Sub,
                Expr::var("a"),
                Expr::binary(BinOp::Sub, Expr::var("b"), Expr::var("c")),
            ),
            "a - (b - c)",
        ),
        (
            Expr::unary(
                UnaryOp::LogicalNot,
                Expr::binary(BinOp::LogicalAnd, Expr::var("a"), Expr::var("b")),
            ),
            "!((bool)Runtime.LoadScript((ByteString)new byte[] { 0xAB }, CallFlags.All, new object[] { a, b }))",
        ),
        (
            Expr::unary(UnaryOp::Neg, Expr::unary(UnaryOp::Neg, Expr::var("value"))),
            "-(-value)",
        ),
        (
            Expr::binary(
                BinOp::Add,
                Expr::var("a"),
                Expr::Ternary {
                    condition: Box::new(Expr::var("condition")),
                    then_expr: Box::new(Expr::var("b")),
                    else_expr: Box::new(Expr::var("c")),
                },
            ),
            "a + (condition ? b : c)",
        ),
    ];

    for (expression, expected) in cases {
        assert_eq!(
            render_expr(&expression, &context),
            expected,
            "{expression:?}"
        );
    }
}

#[test]
fn nested_predicate_calls_parenthesize_ternary_operands() {
    let context = ExprContext::default();
    let cases = [
        (
            OpCode::Nz,
            "__NeoDecompilerUnresolvedCall(\"consume\", new object[] { (BigInteger)(dynamic)(condition ? left : right) != 0 })",
        ),
        (
            OpCode::Isnull,
            "__NeoDecompilerUnresolvedCall(\"consume\", new object[] { (condition ? left : right) is null })",
        ),
        (
            OpCode::Istype,
            "__NeoDecompilerUnresolvedCall(\"consume\", new object[] { (condition ? left : right) is object })",
        ),
    ];

    for (opcode, expected) in cases {
        let predicate = Expr::call(
            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(opcode)),
            vec![Expr::Ternary {
                condition: Box::new(Expr::var("condition")),
                then_expr: Box::new(Expr::var("left")),
                else_expr: Box::new(Expr::var("right")),
            }],
        );
        let nested = Expr::unresolved_call("consume", vec![predicate]);

        assert_eq!(render_expr(&nested, &context), expected, "{opcode:?}");
    }
}

#[test]
fn negative_integer_literal_does_not_form_a_decrement_token() {
    let expression = Expr::unary(UnaryOp::Neg, Expr::int(-1));

    assert_eq!(render_expr(&expression, &ExprContext::default()), "-(-1)");
}

#[test]
fn csharp_strings_escape_unicode_line_separators() {
    let expression = Expr::Literal(Literal::String(
        "before\u{2028}middle\u{2029}after".to_string(),
    ));

    assert_eq!(
        render_expr(&expression, &ExprContext::default()),
        "\"before\\u2028middle\\u2029after\""
    );
}

#[test]
fn typed_shift_counts_render_as_int() {
    let context = ExprContext::default();

    assert_eq!(
        render_expr(
            &Expr::binary(BinOp::Shl, Expr::var("value"), Expr::var("count")),
            &context,
        ),
        "value << (int)(count)"
    );
    assert_eq!(
        render_expr(
            &Expr::binary(BinOp::Shr, Expr::var("value"), Expr::var("count")),
            &context,
        ),
        "value >> (int)(count)"
    );
}

#[test]
fn renders_all_control_flow_variants() {
    let body = Block::with_stmts(vec![
        Stmt::assign("loc0", Expr::int(1)),
        Stmt::comment("trace"),
        Stmt::ControlFlow(Box::new(ControlFlow::If {
            condition: Expr::var("condition"),
            then_branch: Block::with_stmts(vec![Stmt::ret(Expr::int(1))]),
            else_branch: Some(Block::with_stmts(vec![Stmt::ret(Expr::int(2))])),
        })),
        Stmt::ControlFlow(Box::new(ControlFlow::While {
            condition: Expr::var("condition"),
            body: Block::with_stmts(vec![Stmt::assign("loc0", Expr::int(2))]),
        })),
        Stmt::ControlFlow(Box::new(ControlFlow::DoWhile {
            body: Block::with_stmts(vec![Stmt::assign("loc0", Expr::int(3))]),
            condition: Expr::var("condition"),
        })),
        Stmt::ControlFlow(Box::new(ControlFlow::For {
            init: Some(Box::new(Stmt::assign("index", Expr::int(0)))),
            condition: Some(Expr::binary(BinOp::Lt, Expr::var("index"), Expr::int(3))),
            update: Some(Expr::unary(UnaryOp::Inc, Expr::var("index"))),
            body: Block::with_stmts(vec![Stmt::comment("loop")]),
        })),
        Stmt::ControlFlow(Box::new(ControlFlow::Switch {
            expr: Expr::var("loc0"),
            cases: vec![
                (
                    Expr::int(0),
                    Block::with_stmts(vec![Stmt::ret(Expr::int(4))]),
                ),
                (
                    Expr::int(1),
                    Block::with_stmts(vec![Stmt::assign("loc0", Expr::int(5))]),
                ),
            ],
            default: Some(Block::with_stmts(vec![Stmt::ret(Expr::int(6))])),
        })),
        Stmt::ControlFlow(Box::new(ControlFlow::TryCatch {
            try_body: Block::with_stmts(vec![Stmt::ret(Expr::int(7))]),
            catch_var: Some("error".to_string()),
            catch_body: Some(Block::with_stmts(vec![Stmt::ret(Expr::int(8))])),
            finally_body: Some(Block::with_stmts(vec![Stmt::comment("finally")])),
        })),
    ]);
    let symbols = BTreeMap::from([
        (
            "condition".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Parameter(0),
                value_type: ValueType::Boolean,
            },
        ),
        (
            "loc0".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Local(0),
                value_type: ValueType::Integer,
            },
        ),
        (
            "index".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Integer,
            },
        ),
    ]);
    let plan = plan_declarations(&body, &symbols, true);

    assert_eq!(
        render_block(&body, &plan, &symbols, ReturnBehavior::Value, false),
        "BigInteger loc0;\nloc0 = 1;\n// trace\nif (condition) {\n    return 1;\n} else {\n    return 2;\n}\nwhile (condition) {\n    loc0 = 2;\n}\ndo {\n    loc0 = 3;\n} while (condition);\nfor (BigInteger index = 0; index < 3; index++) {\n    // loop\n}\nswitch (loc0) {\n    case var __switchValue0 when (bool)Runtime.LoadScript((ByteString)new byte[] { 0x97 }, CallFlags.All, new object[] { __switchValue0, 0 }): {\n        return 4;\n    }\n    case var __switchValue1 when (bool)Runtime.LoadScript((ByteString)new byte[] { 0x97 }, CallFlags.All, new object[] { __switchValue1, 1 }): {\n        loc0 = 5;\n        break;\n    }\n    default: {\n        return 6;\n    }\n}\ntry {\n    return 7;\n} catch (Exception __caughtException0) {\n    dynamic error = __caughtException0 is __NeoDecompilerVmException __vmException1 ? __vmException1.Payload : __caughtException0.Message;\n    return 8;\n} finally {\n    // finally\n}"
    );
}

#[test]
fn typed_statement_termination_is_recursive() {
    assert!(terminates(&Block::from(vec![Stmt::ret(Expr::int(1))])));
    assert!(terminates(&Block::from(vec![Stmt::ControlFlow(Box::new(
        ControlFlow::If {
            condition: Expr::var("condition"),
            then_branch: Block::from(vec![Stmt::ret(Expr::int(1))]),
            else_branch: Some(Block::from(vec![Stmt::ret(Expr::int(2))])),
        },
    ))])));
    assert!(!terminates(&Block::from(vec![Stmt::ControlFlow(
        Box::new(ControlFlow::If {
            condition: Expr::var("condition"),
            then_branch: Block::from(vec![Stmt::ret(Expr::int(1))]),
            else_branch: Some(Block::new()),
        },)
    )])));
}

#[test]
fn typed_statement_returns_follow_the_method_contract() {
    let void_body = Block::from(vec![Stmt::comment("done"), Stmt::ret_void()]);
    let void_symbols = BTreeMap::new();
    let void_plan = plan_declarations(&void_body, &void_symbols, true);
    assert_eq!(
        render_block(
            &void_body,
            &void_plan,
            &void_symbols,
            ReturnBehavior::Void,
            false,
        ),
        "// done"
    );

    let value_body = Block::from(vec![Stmt::ret_void()]);
    let value_symbols = BTreeMap::new();
    let value_plan = plan_declarations(&value_body, &value_symbols, true);
    assert_eq!(
        render_block(
            &value_body,
            &value_plan,
            &value_symbols,
            ReturnBehavior::Value,
            false,
        ),
        "return default;"
    );
}

#[test]
fn typed_statement_rendering_removes_inlined_temporary_definitions() {
    let body = Block::from(vec![
        Stmt::assign(
            "sum_0",
            Expr::binary(BinOp::Add, Expr::var("left"), Expr::var("right")),
        ),
        Stmt::ret(Expr::var("sum_0")),
    ]);
    let symbols = BTreeMap::from([
        (
            "sum_0".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Integer,
            },
        ),
        (
            "left".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Parameter(0),
                value_type: ValueType::Integer,
            },
        ),
        (
            "right".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Parameter(1),
                value_type: ValueType::Integer,
            },
        ),
    ]);
    let plan = plan_declarations(&body, &symbols, true);

    assert_eq!(
        render_block(&body, &plan, &symbols, ReturnBehavior::Value, true),
        "return left + right;"
    );
    assert_eq!(
        render_block(&body, &plan, &symbols, ReturnBehavior::Value, false),
        "BigInteger sum_0 = left + right;\nreturn sum_0;"
    );
}

#[test]
fn typed_for_rendering_preserves_the_planned_loop_scope() {
    let body = Block::from(vec![Stmt::ControlFlow(Box::new(ControlFlow::For {
        init: None,
        condition: Some(Expr::Literal(Literal::Bool(true))),
        update: Some(Expr::var("body_value")),
        body: Block::from(vec![Stmt::assign("body_value", Expr::int(1))]),
    }))]);
    let symbols = BTreeMap::from([(
        "body_value".to_string(),
        SymbolInfo {
            origin: SymbolOrigin::Temporary,
            value_type: ValueType::Integer,
        },
    )]);
    let plan = plan_declarations(&body, &symbols, true);

    assert_eq!(
        render_block(&body, &plan, &symbols, ReturnBehavior::Void, false),
        "{\n    BigInteger body_value;\n    for (; true; _ = body_value) {\n        body_value = 1;\n    }\n}"
    );
}

#[test]
fn typed_expression_statements_are_compile_valid_and_effect_preserving() {
    let body = Block::from(vec![
        Stmt::expr(Expr::int(1)),
        Stmt::expr(Expr::call(
            SemanticCallTarget::Syscall {
                hash: 0x0388_C3B7,
                name: Some("System.Runtime.GetTime".to_string()),
            },
            vec![],
        )),
        Stmt::expr(Expr::call(
            SemanticCallTarget::Syscall {
                hash: 0x9647_E7CF,
                name: Some("System.Runtime.Log".to_string()),
            },
            vec![Expr::Literal(Literal::String("hello".to_string()))],
        )),
        Stmt::expr(Expr::call(
            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Size)),
            vec![Expr::var("items")],
        )),
        Stmt::expr(Expr::call(
            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Append)),
            vec![Expr::var("items"), Expr::int(2)],
        )),
        Stmt::expr(Expr::call(
            SemanticCallTarget::MethodToken {
                index: 0,
                name: "notify".to_string(),
                hash_le: Some("00112233445566778899AABBCCDDEEFF00112233".to_string()),
                call_flags: Some(0x0F),
            },
            vec![Expr::var("items")],
        )),
        Stmt::expr(Expr::unresolved_call("observe", vec![])),
    ]);
    let symbols = BTreeMap::from([(
        "items".to_string(),
        SymbolInfo {
            origin: SymbolOrigin::Parameter(0),
            value_type: ValueType::Array,
        },
    )]);
    let plan = plan_declarations(&body, &symbols, true);

    assert_eq!(
        render_block(&body, &plan, &symbols, ReturnBehavior::Void, false),
        "_ = 1;\n_ = Runtime.Time;\nRuntime.Log((string)(\"hello\"));\n_ = items.Length;\n((Neo.SmartContract.Framework.List<object>)items).Add(2);\n_ = (dynamic)Contract.Call((UInt160)new byte[] { 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x00, 0x11, 0x22, 0x33 }, \"notify\", (CallFlags)0x0F, new object[] { items });\n__NeoDecompilerUnresolvedCall(\"observe\", new object[] {  });"
    );
}

#[test]
fn typed_statement_references_use_planned_csharp_identifiers() {
    let body = Block::from(vec![
        Stmt::assign("class", Expr::int(1)),
        Stmt::ret(Expr::var("class")),
    ]);
    let symbols = BTreeMap::from([(
        "class".to_string(),
        SymbolInfo {
            origin: SymbolOrigin::Local(0),
            value_type: ValueType::Integer,
        },
    )]);
    let plan = plan_declarations(&body, &symbols, true);

    assert_eq!(
        render_block(&body, &plan, &symbols, ReturnBehavior::Value, false),
        "BigInteger @class = 1;\nreturn @class;"
    );
}

#[test]
fn typed_boundaries_render_valid_explicit_conversions() {
    let body = Block::from(vec![
        Stmt::assign("text", Expr::var("buffer")),
        Stmt::ret(Expr::var("buffer")),
    ]);
    let symbols = BTreeMap::from([
        (
            "text".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Local(0),
                value_type: ValueType::ByteString,
            },
        ),
        (
            "buffer".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Parameter(0),
                value_type: ValueType::Buffer,
            },
        ),
    ]);
    let plan = plan_declarations(&body, &symbols, true);

    assert_eq!(
        super::stmt::render_block_with_trace(
            &body,
            &plan,
            &symbols,
            ReturnBehavior::Value,
            false,
            "__assert",
            "__NeoDecompilerVmException",
            &BTreeMap::new(),
            &BTreeMap::new(),
            Some("UInt160"),
            None,
            &[],
        ),
        "ByteString text = (ByteString)(buffer);\nreturn (UInt160)(buffer);"
    );
}

#[test]
fn typed_internal_call_boundaries_use_exact_resolved_return_types() {
    let call = Expr::call(
        SemanticCallTarget::Internal {
            offset: 42,
            name: "helper".to_string(),
        },
        Vec::new(),
    );
    let body = Block::from(vec![Stmt::ret(call)]);
    let symbols = BTreeMap::new();
    let plan = plan_declarations(&body, &symbols, true);

    let render = |return_types: &BTreeMap<usize, String>| {
        super::stmt::render_block_with_trace(
            &body,
            &plan,
            &symbols,
            ReturnBehavior::Value,
            false,
            "__assert",
            "__NeoDecompilerVmException",
            &BTreeMap::new(),
            return_types,
            Some("BigInteger"),
            None,
            &[],
        )
    };

    assert_eq!(
        render(&BTreeMap::from([(42, "BigInteger".to_string())])),
        "return helper();"
    );
    assert_eq!(
        render(&BTreeMap::from([(42, "ByteString".to_string())])),
        "return (BigInteger)(dynamic)(helper());"
    );
    assert_eq!(
        render(&BTreeMap::new()),
        "return (BigInteger)(dynamic)(helper());"
    );
}

#[test]
fn typed_ambient_assignments_render_boundary_conversions() {
    let body = Block::from(vec![Stmt::assign("static0", Expr::Unknown)]);
    let symbols = BTreeMap::from([(
        "static0".to_string(),
        SymbolInfo {
            origin: SymbolOrigin::Static(0),
            value_type: ValueType::Integer,
        },
    )]);

    let typed = plan_declarations(&body, &symbols, true);
    assert_eq!(
        render_block(&body, &typed, &symbols, ReturnBehavior::Void, false),
        "static0 = (BigInteger)(dynamic)((dynamic)null);"
    );

    let dynamic = plan_declarations(&body, &symbols, false);
    assert_eq!(
        render_block(&body, &dynamic, &symbols, ReturnBehavior::Void, false),
        "static0 = (dynamic)null;"
    );
}

#[test]
fn hoisted_phi_declarations_are_default_initialized() {
    let body = Block::from(vec![
        Stmt::ControlFlow(Box::new(ControlFlow::if_then(
            Expr::var("condition"),
            Block::from(vec![Stmt::assign(
                "p3_0",
                Expr::Literal(Literal::Bool(true)),
            )]),
        ))),
        Stmt::ret(Expr::var("p3_0")),
    ]);
    let symbols = BTreeMap::from([
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
                value_type: ValueType::Boolean,
            },
        ),
    ]);
    let plan = plan_declarations(&body, &symbols, true);

    assert_eq!(
        render_block(&body, &plan, &symbols, ReturnBehavior::Value, false),
        "bool p3_0 = default;\nif (condition) {\n    p3_0 = true;\n}\nreturn p3_0;"
    );
}

#[test]
fn typed_boundaries_bridge_incompatible_known_types() {
    let body = Block::from(vec![
        Stmt::assign("flag", Expr::int(1)),
        Stmt::assign(
            "static0",
            Expr::NewArray {
                length: Box::new(Expr::int(2)),
                element_type: Some(ValueType::Integer),
            },
        ),
    ]);
    let symbols = BTreeMap::from([
        (
            "flag".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Local(0),
                value_type: ValueType::Boolean,
            },
        ),
        (
            "static0".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Static(0),
                value_type: ValueType::Array,
            },
        ),
    ]);
    let plan = plan_declarations(&body, &symbols, true);

    assert_eq!(
        render_block(&body, &plan, &symbols, ReturnBehavior::Void, false),
        "bool flag = (bool)(dynamic)(1);\nstatic0 = (object[])(dynamic)(new BigInteger[(int)(2)]);"
    );
}

#[test]
fn inlines_only_pure_single_use_temporaries() {
    let pure = "pure_0";
    let call = "call_0";
    let array = "array_0";
    let body = Block::with_stmts(vec![
        Stmt::assign(
            pure,
            Expr::binary(BinOp::Add, Expr::var("left"), Expr::var("right")),
        ),
        Stmt::assign(call, Expr::unresolved_call("read", vec![])),
        Stmt::assign(array, Expr::Array(vec![Expr::int(1)])),
        Stmt::expr(Expr::unresolved_call(
            "consume",
            vec![Expr::var(pure), Expr::var(call), Expr::var(array)],
        )),
    ]);
    let symbols = BTreeMap::from([
        (
            pure.to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Integer,
            },
        ),
        (
            call.to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Any,
            },
        ),
        (
            array.to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Array,
            },
        ),
    ]);

    let context = ExprContext::for_block(&body, &symbols, true);
    assert_eq!(render_expr(&Expr::var(pure), &context), "left + right");
    assert_eq!(render_expr(&Expr::var(call), &context), call);
    assert_eq!(render_expr(&Expr::var(array), &context), array);
    assert!(context.is_inlined(pure));
    assert!(!context.is_inlined(call));
    assert!(!context.is_inlined(array));

    let disabled = ExprContext::for_block(&body, &symbols, false);
    assert_eq!(render_expr(&Expr::var(pure), &disabled), pure);
}

#[test]
fn observable_state_and_allocations_are_not_inlineable() {
    let names = ["bytes_0", "index_0", "member_0", "static_read_0"];
    let body = Block::with_stmts(vec![
        Stmt::assign(names[0], Expr::Literal(Literal::Bytes(vec![0x01, 0x02]))),
        Stmt::assign(names[1], Expr::index(Expr::var("items"), Expr::int(0))),
        Stmt::assign(
            names[2],
            Expr::Member {
                base: Box::new(Expr::var("holder")),
                name: "Value".to_string(),
            },
        ),
        Stmt::assign(names[3], Expr::var("static0")),
        Stmt::expr(Expr::unresolved_call(
            "mutate",
            vec![Expr::var("items"), Expr::var("holder")],
        )),
        Stmt::expr(Expr::unresolved_call(
            "consume",
            names.iter().copied().map(Expr::var).collect(),
        )),
    ]);
    let symbols = BTreeMap::from([
        (
            "items".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Parameter(0),
                value_type: ValueType::Array,
            },
        ),
        (
            "holder".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Parameter(1),
                value_type: ValueType::Any,
            },
        ),
        (
            "static0".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Static(0),
                value_type: ValueType::Integer,
            },
        ),
        (
            names[0].to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Buffer,
            },
        ),
        (
            names[1].to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Any,
            },
        ),
        (
            names[2].to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Any,
            },
        ),
        (
            names[3].to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Integer,
            },
        ),
    ]);

    let context = ExprContext::for_block(&body, &symbols, true);

    for name in names {
        assert!(
            !context.is_inlined(name),
            "{name} must stay at its definition"
        );
    }
}

#[test]
fn temporary_inlining_does_not_move_throwing_expressions() {
    let body = Block::with_stmts(vec![
        Stmt::assign(
            "quotient_0",
            Expr::binary(BinOp::Div, Expr::var("dividend"), Expr::var("divisor")),
        ),
        Stmt::expr(Expr::unresolved_call("observe", vec![])),
        Stmt::expr(Expr::unresolved_call(
            "consume",
            vec![Expr::var("quotient_0")],
        )),
    ]);
    let symbols = BTreeMap::from([(
        "quotient_0".to_string(),
        SymbolInfo {
            origin: SymbolOrigin::Temporary,
            value_type: ValueType::Integer,
        },
    )]);

    let context = ExprContext::for_block(&body, &symbols, true);

    assert!(!context.is_inlined("quotient_0"));
}

#[test]
fn temporary_inlining_does_not_move_wrapper_backed_predicates() {
    let body = Block::with_stmts(vec![
        Stmt::assign(
            "equal_0",
            Expr::binary(BinOp::Eq, Expr::var("left"), Expr::var("right")),
        ),
        Stmt::assign(
            "and_0",
            Expr::binary(
                BinOp::LogicalAnd,
                Expr::var("opaque_left"),
                Expr::var("opaque_right"),
            ),
        ),
        Stmt::expr(Expr::unresolved_call("observe", vec![])),
        Stmt::expr(Expr::unresolved_call(
            "consume",
            vec![Expr::var("equal_0"), Expr::var("and_0")],
        )),
    ]);
    let symbols = BTreeMap::from([
        (
            "left".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Parameter(0),
                value_type: ValueType::ByteString,
            },
        ),
        (
            "right".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Parameter(1),
                value_type: ValueType::ByteString,
            },
        ),
        (
            "opaque_left".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Parameter(2),
                value_type: ValueType::Unknown,
            },
        ),
        (
            "opaque_right".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Parameter(3),
                value_type: ValueType::Unknown,
            },
        ),
        (
            "equal_0".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Boolean,
            },
        ),
        (
            "and_0".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Boolean,
            },
        ),
    ]);

    let context = ExprContext::for_block(&body, &symbols, true);

    assert!(!context.is_inlined("equal_0"));
    assert!(!context.is_inlined("and_0"));
}

#[test]
fn temporary_inlining_does_not_move_casts() {
    let body = Block::with_stmts(vec![
        Stmt::assign(
            "cast_0",
            Expr::Cast {
                expr: Box::new(Expr::var("value")),
                target_type: "BigInteger".to_string(),
            },
        ),
        Stmt::expr(Expr::unresolved_call("observe", vec![])),
        Stmt::expr(Expr::unresolved_call("consume", vec![Expr::var("cast_0")])),
    ]);
    let symbols = BTreeMap::from([(
        "cast_0".to_string(),
        SymbolInfo {
            origin: SymbolOrigin::Temporary,
            value_type: ValueType::Integer,
        },
    )]);

    let context = ExprContext::for_block(&body, &symbols, true);

    assert!(!context.is_inlined("cast_0"));
}

#[test]
fn temporary_inlining_does_not_move_values_into_while_conditions() {
    let body = Block::with_stmts(vec![
        Stmt::assign(
            "predicate_0",
            Expr::binary(BinOp::Lt, Expr::var("left"), Expr::var("right")),
        ),
        Stmt::ControlFlow(Box::new(ControlFlow::While {
            condition: Expr::var("predicate_0"),
            body: Block::new(),
        })),
    ]);
    let symbols = BTreeMap::from([(
        "predicate_0".to_string(),
        SymbolInfo {
            origin: SymbolOrigin::Temporary,
            value_type: ValueType::Boolean,
        },
    )]);

    let context = ExprContext::for_block(&body, &symbols, true);

    assert!(!context.is_inlined("predicate_0"));
}

#[test]
fn temporary_inlining_does_not_move_for_initializers_into_conditions() {
    let body = Block::with_stmts(vec![Stmt::ControlFlow(Box::new(ControlFlow::For {
        init: Some(Box::new(Stmt::assign(
            "predicate_0",
            Expr::binary(BinOp::Lt, Expr::var("left"), Expr::var("right")),
        ))),
        condition: Some(Expr::var("predicate_0")),
        update: None,
        body: Block::new(),
    }))]);
    let symbols = BTreeMap::from([(
        "predicate_0".to_string(),
        SymbolInfo {
            origin: SymbolOrigin::Temporary,
            value_type: ValueType::Boolean,
        },
    )]);

    let context = ExprContext::for_block(&body, &symbols, true);

    assert!(!context.is_inlined("predicate_0"));
}

#[test]
fn temporary_inlining_does_not_move_for_initializers_into_updates() {
    let body = Block::with_stmts(vec![Stmt::ControlFlow(Box::new(ControlFlow::For {
        init: Some(Box::new(Stmt::assign(
            "next_0",
            Expr::binary(BinOp::Add, Expr::var("index"), Expr::int(1)),
        ))),
        condition: None,
        update: Some(Expr::var("next_0")),
        body: Block::new(),
    }))]);
    let symbols = BTreeMap::from([(
        "next_0".to_string(),
        SymbolInfo {
            origin: SymbolOrigin::Temporary,
            value_type: ValueType::Integer,
        },
    )]);

    let context = ExprContext::for_block(&body, &symbols, true);

    assert!(!context.is_inlined("next_0"));
}

#[test]
fn temporary_inlining_does_not_move_values_into_do_while_conditions() {
    let body = Block::with_stmts(vec![
        Stmt::assign(
            "predicate_0",
            Expr::binary(BinOp::Lt, Expr::var("left"), Expr::var("right")),
        ),
        Stmt::ControlFlow(Box::new(ControlFlow::DoWhile {
            body: Block::new(),
            condition: Expr::var("predicate_0"),
        })),
    ]);
    let symbols = BTreeMap::from([(
        "predicate_0".to_string(),
        SymbolInfo {
            origin: SymbolOrigin::Temporary,
            value_type: ValueType::Boolean,
        },
    )]);

    let context = ExprContext::for_block(&body, &symbols, true);

    assert!(!context.is_inlined("predicate_0"));
}

#[test]
fn temporary_inlining_rejects_reassigned_dependencies() {
    let body = Block::with_stmts(vec![
        Stmt::assign("source_0", Expr::int(1)),
        Stmt::assign("saved_0", Expr::var("source_0")),
        Stmt::assign("source_0", Expr::int(2)),
        Stmt::expr(Expr::unresolved_call("consume", vec![Expr::var("saved_0")])),
    ]);
    let symbols = BTreeMap::from([
        (
            "source_0".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Integer,
            },
        ),
        (
            "saved_0".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Integer,
            },
        ),
    ]);

    let context = ExprContext::for_block(&body, &symbols, true);

    assert!(!context.is_inlined("saved_0"));
}

#[test]
fn temporary_inlining_requires_a_concrete_value_type() {
    let body = Block::with_stmts(vec![
        Stmt::assign("opaque_0", Expr::int(1)),
        Stmt::expr(Expr::unresolved_call(
            "consume",
            vec![Expr::var("opaque_0")],
        )),
    ]);
    let symbols = BTreeMap::from([(
        "opaque_0".to_string(),
        SymbolInfo {
            origin: SymbolOrigin::Temporary,
            value_type: ValueType::Unknown,
        },
    )]);

    let context = ExprContext::for_block(&body, &symbols, true);

    assert!(!context.is_inlined("opaque_0"));
}

#[test]
fn temporary_inlining_requires_definition_before_use() {
    let name = "late_0";
    let body = Block::with_stmts(vec![
        Stmt::expr(Expr::unresolved_call("consume", vec![Expr::var(name)])),
        Stmt::assign(name, Expr::int(1)),
    ]);
    let symbols = BTreeMap::from([(
        name.to_string(),
        SymbolInfo {
            origin: SymbolOrigin::Temporary,
            value_type: ValueType::Integer,
        },
    )]);

    let context = ExprContext::for_block(&body, &symbols, true);

    assert!(!context.is_inlined(name));
    assert_eq!(render_expr(&Expr::var(name), &context), name);
}
