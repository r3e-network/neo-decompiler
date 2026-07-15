use super::*;

#[test]
fn typed_literal_array_foreach_uses_uniform_element_type_through_aliases() {
    let body = Block::from(vec![
        Stmt::assign(
            "items",
            Expr::Array(vec![Expr::int(1), Expr::int(2), Expr::int(3)]),
        ),
        Stmt::assign("source", Expr::var("items")),
        Stmt::ControlFlow(Box::new(ControlFlow::For {
            init: Some(Box::new(Stmt::assign("index", Expr::int(0)))),
            condition: Some(Expr::binary(
                BinOp::Lt,
                Expr::var("index"),
                Expr::Member {
                    base: Box::new(Expr::var("source")),
                    name: "Length".to_string(),
                },
            )),
            update: Some(Expr::unary(UnaryOp::Inc, Expr::var("index"))),
            body: Block::from(vec![Stmt::assign(
                "item",
                Expr::index(Expr::var("source"), Expr::var("index")),
            )]),
        })),
    ]);
    let symbols = BTreeMap::from([
        (
            "items".to_string(),
            SymbolInfo {
                origin: SymbolOrigin::Local(0),
                value_type: ValueType::Array,
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
                value_type: ValueType::Integer,
            },
        ),
    ]);
    let plan = plan_declarations(&body, &symbols, true);

    let rendered = render_block(&body, &plan, &symbols, ReturnBehavior::Void, false);

    assert!(
        rendered.contains("foreach (BigInteger item in source)"),
        "{rendered}"
    );
    assert!(
        rendered.contains("BigInteger[] items = new BigInteger[] { 1, 2, 3 };"),
        "{rendered}"
    );
    assert!(!rendered.contains("foreach (dynamic item"), "{rendered}");
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
