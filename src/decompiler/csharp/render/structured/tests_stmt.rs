use super::*;

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
fn typed_array_index_loops_render_as_foreach_when_the_index_is_private() {
    let body = Block::from(vec![Stmt::ControlFlow(Box::new(ControlFlow::For {
        init: Some(Box::new(Stmt::assign("index", Expr::int(0)))),
        condition: Some(Expr::binary(
            BinOp::Lt,
            Expr::var("index"),
            Expr::Member {
                base: Box::new(Expr::var("items")),
                name: "Length".to_string(),
            },
        )),
        update: Some(Expr::unary(UnaryOp::Inc, Expr::var("index"))),
        body: Block::from(vec![
            Stmt::assign(
                "element_temp",
                Expr::index(Expr::var("items"), Expr::var("index")),
            ),
            Stmt::assign("item", Expr::var("element_temp")),
            Stmt::assign(
                "total",
                Expr::binary(BinOp::Add, Expr::var("total"), Expr::var("item")),
            ),
        ]),
    }))]);
    let symbols = BTreeMap::from([
        (
            "items".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Parameter(0),
                value_type: ValueType::Array,
            },
        ),
        (
            "index".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Integer,
            },
        ),
        (
            "element_temp".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Unknown,
            },
        ),
        (
            "item".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Unknown,
            },
        ),
        (
            "total".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Integer,
            },
        ),
    ]);
    let plan = plan_declarations(&body, &symbols, true);

    let rendered = render_block(&body, &plan, &symbols, ReturnBehavior::Void, false);

    assert!(
        rendered.contains("foreach (dynamic item in items)"),
        "{rendered}"
    );
    assert!(!rendered.contains("for ("), "{rendered}");
    assert!(!rendered.contains("element_temp"), "{rendered}");
    assert!(!rendered.contains("index"), "{rendered}");
}

#[test]
fn typed_array_index_loops_follow_size_aliases() {
    let body = Block::from(vec![
        Stmt::assign(
            "limit_temp",
            Expr::call(
                SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Size)),
                vec![Expr::var("items")],
            ),
        ),
        Stmt::assign("limit", Expr::var("limit_temp")),
        Stmt::assign("source", Expr::var("items")),
        Stmt::ControlFlow(Box::new(ControlFlow::For {
            init: Some(Box::new(Stmt::assign("index", Expr::int(0)))),
            condition: Some(Expr::binary(
                BinOp::Lt,
                Expr::var("index"),
                Expr::var("limit"),
            )),
            update: Some(Expr::unary(UnaryOp::Inc, Expr::var("index"))),
            body: Block::from(vec![
                Stmt::assign("item", Expr::index(Expr::var("source"), Expr::var("index"))),
                Stmt::assign(
                    "total",
                    Expr::binary(BinOp::Add, Expr::var("total"), Expr::var("item")),
                ),
            ]),
        })),
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
            "limit_temp".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Integer,
            },
        ),
        (
            "limit".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Integer,
            },
        ),
        (
            "source".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Array,
            },
        ),
        (
            "index".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Integer,
            },
        ),
        (
            "item".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Unknown,
            },
        ),
        (
            "total".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Integer,
            },
        ),
    ]);
    let plan = plan_declarations(&body, &symbols, true);

    let rendered = render_block(&body, &plan, &symbols, ReturnBehavior::Void, false);

    assert!(
        rendered.contains("foreach (dynamic item in source)"),
        "{rendered}"
    );
    assert!(!rendered.contains("for ("), "{rendered}");
}

#[test]
fn typed_array_index_loops_keep_for_when_bound_is_not_collection_size() {
    let body = Block::from(vec![Stmt::ControlFlow(Box::new(ControlFlow::For {
        init: Some(Box::new(Stmt::assign("index", Expr::int(0)))),
        condition: Some(Expr::binary(BinOp::Lt, Expr::var("index"), Expr::int(2))),
        update: Some(Expr::unary(UnaryOp::Inc, Expr::var("index"))),
        body: Block::from(vec![
            Stmt::assign("item", Expr::index(Expr::var("items"), Expr::var("index"))),
            Stmt::assign(
                "total",
                Expr::binary(BinOp::Add, Expr::var("total"), Expr::var("item")),
            ),
        ]),
    }))]);
    let symbols = BTreeMap::from([
        (
            "items".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Parameter(0),
                value_type: ValueType::Array,
            },
        ),
        (
            "index".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Integer,
            },
        ),
        (
            "item".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Unknown,
            },
        ),
        (
            "total".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Integer,
            },
        ),
    ]);
    let plan = plan_declarations(&body, &symbols, true);

    let rendered = render_block(&body, &plan, &symbols, ReturnBehavior::Void, false);

    assert!(rendered.contains("for (index = 0;"), "{rendered}");
    assert!(!rendered.contains("foreach ("), "{rendered}");
}

#[test]
fn typed_array_index_loops_keep_for_when_counter_escapes_after_loop() {
    let body = Block::from(vec![
        Stmt::ControlFlow(Box::new(ControlFlow::For {
            init: Some(Box::new(Stmt::assign("index", Expr::int(0)))),
            condition: Some(Expr::binary(
                BinOp::Lt,
                Expr::var("index"),
                Expr::Member {
                    base: Box::new(Expr::var("items")),
                    name: "Length".to_string(),
                },
            )),
            update: Some(Expr::unary(UnaryOp::Inc, Expr::var("index"))),
            body: Block::from(vec![Stmt::assign(
                "item",
                Expr::index(Expr::var("items"), Expr::var("index")),
            )]),
        })),
        Stmt::assign("after", Expr::var("index")),
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
            "index".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Integer,
            },
        ),
        (
            "item".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Unknown,
            },
        ),
        (
            "after".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Integer,
            },
        ),
    ]);
    let plan = plan_declarations(&body, &symbols, true);

    let rendered = render_block(&body, &plan, &symbols, ReturnBehavior::Void, false);

    assert!(rendered.contains("for ("), "{rendered}");
    assert!(!rendered.contains("foreach ("), "{rendered}");
}

#[test]
fn typed_array_index_loops_keep_for_when_extraction_temporary_escapes() {
    let body = Block::from(vec![Stmt::ControlFlow(Box::new(ControlFlow::For {
        init: Some(Box::new(Stmt::assign("index", Expr::int(0)))),
        condition: Some(Expr::binary(
            BinOp::Lt,
            Expr::var("index"),
            Expr::Member {
                base: Box::new(Expr::var("items")),
                name: "Length".to_string(),
            },
        )),
        update: Some(Expr::unary(UnaryOp::Inc, Expr::var("index"))),
        body: Block::from(vec![
            Stmt::assign(
                "element_temp",
                Expr::index(Expr::var("items"), Expr::var("index")),
            ),
            Stmt::assign("item", Expr::var("element_temp")),
            Stmt::assign("observed", Expr::var("element_temp")),
        ]),
    }))]);
    let symbols = BTreeMap::from([
        (
            "items".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Parameter(0),
                value_type: ValueType::Array,
            },
        ),
        (
            "index".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Integer,
            },
        ),
        (
            "element_temp".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Unknown,
            },
        ),
        (
            "item".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Unknown,
            },
        ),
        (
            "observed".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Unknown,
            },
        ),
    ]);
    let plan = plan_declarations(&body, &symbols, true);

    let rendered = render_block(&body, &plan, &symbols, ReturnBehavior::Void, false);

    assert!(rendered.contains("for ("), "{rendered}");
    assert!(!rendered.contains("foreach ("), "{rendered}");
}

#[test]
fn typed_array_index_loops_keep_for_when_the_counter_escapes() {
    let body = Block::from(vec![Stmt::ControlFlow(Box::new(ControlFlow::For {
        init: Some(Box::new(Stmt::assign("index", Expr::int(0)))),
        condition: Some(Expr::binary(BinOp::Lt, Expr::var("index"), Expr::int(2))),
        update: Some(Expr::unary(UnaryOp::Inc, Expr::var("index"))),
        body: Block::from(vec![
            Stmt::assign("item", Expr::index(Expr::var("items"), Expr::var("index"))),
            Stmt::expr(Expr::unresolved_call("observe", vec![Expr::var("index")])),
        ]),
    }))]);
    let symbols = BTreeMap::from([
        (
            "items".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Parameter(0),
                value_type: ValueType::Array,
            },
        ),
        (
            "index".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Integer,
            },
        ),
        (
            "item".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Unknown,
            },
        ),
    ]);
    let plan = plan_declarations(&body, &symbols, true);

    let rendered = render_block(&body, &plan, &symbols, ReturnBehavior::Void, false);

    assert!(rendered.contains("for (index = 0;"), "{rendered}");
    assert!(!rendered.contains("foreach ("), "{rendered}");
}

#[test]
fn typed_array_index_loops_keep_for_across_opaque_body_calls() {
    let body = Block::from(vec![Stmt::ControlFlow(Box::new(ControlFlow::For {
        init: Some(Box::new(Stmt::assign("index", Expr::int(0)))),
        condition: Some(Expr::binary(BinOp::Lt, Expr::var("index"), Expr::int(2))),
        update: Some(Expr::unary(UnaryOp::Inc, Expr::var("index"))),
        body: Block::from(vec![
            Stmt::assign("item", Expr::index(Expr::var("items"), Expr::var("index"))),
            Stmt::expr(Expr::unresolved_call("mutate", vec![Expr::var("item")])),
        ]),
    }))]);
    let symbols = BTreeMap::from([
        (
            "items".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Parameter(0),
                value_type: ValueType::Array,
            },
        ),
        (
            "index".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Integer,
            },
        ),
        (
            "item".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Unknown,
            },
        ),
    ]);
    let plan = plan_declarations(&body, &symbols, true);

    let rendered = render_block(&body, &plan, &symbols, ReturnBehavior::Void, false);

    assert!(rendered.contains("for (index = 0;"), "{rendered}");
    assert!(!rendered.contains("foreach ("), "{rendered}");
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
        super::super::stmt::render_block_with_trace(
            &body,
            &plan,
            &symbols,
            ReturnBehavior::Value,
            false,
            "__assert",
            "__NeoDecompilerVmException",
            None,
            None,
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
        super::super::stmt::render_block_with_trace(
            &body,
            &plan,
            &symbols,
            ReturnBehavior::Value,
            false,
            "__assert",
            "__NeoDecompilerVmException",
            None,
            None,
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
        "return (BigInteger)(helper());"
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
fn typed_static_field_boundaries_use_contract_field_types() {
    let body = Block::from(vec![Stmt::assign("static0", Expr::Unknown)]);
    let symbols = BTreeMap::new();
    let static_field_types =
        BTreeMap::from([(String::from("static0"), String::from("BigInteger"))]);
    let plan =
        plan_declarations(&body, &symbols, true).with_static_field_types(&static_field_types);

    assert_eq!(
        render_block(&body, &plan, &symbols, ReturnBehavior::Void, false),
        "static0 = (BigInteger)(dynamic)((dynamic)null);"
    );
}

#[test]
fn typed_index_definitions_ignore_stale_slot_collection_types() {
    let body = Block::from(vec![
        Stmt::assign("t0", Expr::index(Expr::var("items"), Expr::int(0))),
        Stmt::assign("loc0", Expr::var("t0")),
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
            "t0".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Unknown,
            },
        ),
        (
            "loc0".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Local(0),
                value_type: ValueType::Struct,
            },
        ),
    ]);
    let plan = plan_declarations(&body, &symbols, true);
    let rendered = render_block(&body, &plan, &symbols, ReturnBehavior::Void, false);

    assert!(rendered.contains("dynamic t0 = "), "{rendered}");
    assert!(rendered.contains("dynamic loc0 = t0;"), "{rendered}");
    assert!(!rendered.contains("object[]"), "{rendered}");
}

#[test]
fn typed_index_copy_provenance_converges_independently_of_statement_order() {
    let body = Block::from(vec![
        Stmt::assign("loc0", Expr::var("t0")),
        Stmt::assign("t0", Expr::index(Expr::var("items"), Expr::int(0))),
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
            "t0".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Temporary,
                value_type: ValueType::Unknown,
            },
        ),
        (
            "loc0".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Local(0),
                value_type: ValueType::Struct,
            },
        ),
    ]);

    let plan = plan_declarations(&body, &symbols, true);

    assert_eq!(plan.declarations["t0"].csharp_type, "dynamic");
    assert_eq!(plan.declarations["loc0"].csharp_type, "dynamic");
}

#[test]
fn typed_index_assignments_dynamicize_parameter_and_static_storage() {
    let manifest = ContractManifest::from_json_str(
        r#"{
            "name": "IndexStorage",
            "abi": { "methods": [{
                "name": "store",
                "parameters": [
                    { "name": "target", "type": "Array" },
                    { "name": "items", "type": "Array" }
                ],
                "returntype": "Array",
                "offset": 0
            }] }
        }"#,
    )
    .expect("manifest parsed");
    let instructions = vec![
        Instruction::new(0, OpCode::Initslot, Some(Operand::Bytes(vec![0, 2]))),
        Instruction::new(3, OpCode::Push1, None),
        Instruction::new(4, OpCode::Push2, None),
        Instruction::new(5, OpCode::Push2, None),
        Instruction::new(6, OpCode::Packstruct, None),
        Instruction::new(7, OpCode::Dup, None),
        Instruction::new(8, OpCode::Push0, None),
        Instruction::new(9, OpCode::Ldarg1, None),
        Instruction::new(10, OpCode::Setitem, None),
        Instruction::new(11, OpCode::Unpack, None),
        Instruction::new(12, OpCode::Drop, None),
        Instruction::new(13, OpCode::Drop, None),
        Instruction::new(14, OpCode::Dup, None),
        Instruction::new(15, OpCode::Starg0, None),
        Instruction::new(16, OpCode::Stsfld0, None),
        Instruction::new(17, OpCode::Ldarg0, None),
        Instruction::new(18, OpCode::Ret, None),
    ];
    let types = TypeInfo {
        statics: vec![ValueType::Struct],
        ..TypeInfo::default()
    };

    let plans = build_csharp_method_plans(
        &instructions,
        Some(&manifest),
        &CallGraph::default(),
        &MethodContracts::default(),
        &types,
        &[0],
    );
    let contract = plan_contract_symbols(
        &types,
        &plans.method_symbol_maps().iter().collect::<Vec<_>>(),
        true,
        plans.index_defined_statics(),
    );

    assert_eq!(plans.manifest_method(0).parameters[0].ty, "dynamic");
    assert_eq!(contract.static_fields[0].csharp_type, "dynamic");
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
