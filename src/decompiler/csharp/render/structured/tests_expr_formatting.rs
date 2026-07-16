use super::super::*;

#[test]
fn map_intrinsics_guard_known_non_map_receivers() {
    let context = expr_context_with_types(&[
        ("flag", ValueType::Boolean),
        ("items", ValueType::Array),
        ("map", ValueType::Map),
    ]);
    let intrinsic = |opcode, args| {
        Expr::call(
            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(opcode)),
            args,
        )
    };

    let keys = render_expr(&intrinsic(OpCode::Keys, vec![Expr::var("flag")]), &context);
    assert!(keys.contains("Runtime.LoadScript"), "{keys}");
    assert!(!keys.contains(".Keys"), "{keys}");

    let values = render_expr(
        &intrinsic(OpCode::Values, vec![Expr::var("items")]),
        &context,
    );
    assert!(values.contains("Runtime.LoadScript"), "{values}");
    assert!(!values.contains(".Values"), "{values}");

    let has_key = render_expr(
        &intrinsic(OpCode::Haskey, vec![Expr::var("flag"), Expr::var("map")]),
        &context,
    );
    assert!(has_key.contains("Runtime.LoadScript"), "{has_key}");
    assert!(!has_key.contains(".HasKey"), "{has_key}");

    let append = render_expr(
        &intrinsic(OpCode::Append, vec![Expr::var("map"), Expr::var("flag")]),
        &context,
    );
    assert!(append.contains("Runtime.LoadScript"), "{append}");
    assert!(!append.contains("List<object>"), "{append}");

    let popitem = render_expr(
        &intrinsic(OpCode::Popitem, vec![Expr::var("map")]),
        &context,
    );
    assert!(popitem.contains("Runtime.LoadScript"), "{popitem}");
    assert!(!popitem.contains("List<object>"), "{popitem}");

    let reverse = render_expr(
        &intrinsic(OpCode::Reverseitems, vec![Expr::var("map")]),
        &context,
    );
    assert!(reverse.contains("Runtime.LoadScript"), "{reverse}");
    assert!(!reverse.contains("Helper.Reverse"), "{reverse}");
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
