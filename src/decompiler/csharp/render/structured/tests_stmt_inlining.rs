use super::super::*;

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
