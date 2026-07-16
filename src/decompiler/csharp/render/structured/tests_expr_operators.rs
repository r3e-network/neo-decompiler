use super::super::*;

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
    let context = ExprContext::default().with_concrete_types(&BTreeMap::from([
        ("number".to_string(), "BigInteger".to_string()),
        ("flag".to_string(), "bool".to_string()),
    ]));
    for name in ["number", "flag"] {
        let expression = Expr::call(
            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Isnull)),
            vec![Expr::var(name)],
        );
        assert_eq!(render_expr(&expression, &context), "false");
    }
}

#[test]
fn isnull_preserves_ambiguous_integer_aliases() {
    let context =
        expr_context_with_types(&[("number", ValueType::Integer), ("flag", ValueType::Boolean)]);
    for name in ["number", "flag"] {
        let expression = Expr::call(
            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Isnull)),
            vec![Expr::var(name)],
        );
        assert_eq!(
            render_expr(&expression, &context),
            format!("{name} is null")
        );
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
