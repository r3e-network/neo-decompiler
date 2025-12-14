use super::super::*;

#[test]
fn test_literal_rendering() {
    assert_eq!(render_expr(&Expr::int(42)), "42");
    assert_eq!(render_expr(&Expr::Literal(Literal::Bool(true))), "true");
    assert_eq!(
        render_expr(&Expr::Literal(Literal::String("hello".into()))),
        "\"hello\""
    );
    assert_eq!(render_expr(&Expr::Literal(Literal::Null)), "null");
    assert_eq!(
        render_expr(&Expr::Literal(Literal::Bytes(vec![0xDE, 0xAD]))),
        "0xdead"
    );
}

#[test]
fn test_variable_rendering() {
    assert_eq!(render_expr(&Expr::var("x")), "x");
    assert_eq!(render_expr(&Expr::var("local_0")), "local_0");
}

#[test]
fn test_binary_expression_rendering() {
    let expr = Expr::binary(BinOp::Add, Expr::var("x"), Expr::int(1));
    assert_eq!(render_expr(&expr), "(x + 1)");

    let nested = Expr::binary(
        BinOp::Mul,
        Expr::binary(BinOp::Add, Expr::var("a"), Expr::var("b")),
        Expr::var("c"),
    );
    assert_eq!(render_expr(&nested), "((a + b) * c)");
}

#[test]
fn test_unary_expression_rendering() {
    let neg = Expr::unary(UnaryOp::Neg, Expr::var("x"));
    assert_eq!(render_expr(&neg), "-x");

    let not = Expr::unary(UnaryOp::LogicalNot, Expr::var("flag"));
    assert_eq!(render_expr(&not), "!flag");

    let abs = Expr::unary(UnaryOp::Abs, Expr::var("n"));
    assert_eq!(render_expr(&abs), "abs(n)");
}

#[test]
fn test_call_expression_rendering() {
    let call = Expr::call("foo", vec![Expr::int(1), Expr::int(2)]);
    assert_eq!(render_expr(&call), "foo(1, 2)");

    let no_args = Expr::call("bar", vec![]);
    assert_eq!(render_expr(&no_args), "bar()");
}

#[test]
fn test_index_expression_rendering() {
    let idx = Expr::index(Expr::var("arr"), Expr::int(0));
    assert_eq!(render_expr(&idx), "arr[0]");
}

#[test]
fn test_cast_expression_rendering() {
    let cast = Expr::Cast {
        expr: Box::new(Expr::var("x")),
        target_type: "int".into(),
    };
    assert_eq!(render_expr(&cast), "(int)(x)");
}

#[test]
fn test_array_rendering() {
    let arr = Expr::Array(vec![Expr::int(1), Expr::int(2), Expr::int(3)]);
    assert_eq!(render_expr(&arr), "[1, 2, 3]");
}
