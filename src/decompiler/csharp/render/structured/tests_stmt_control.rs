use super::super::*;

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
