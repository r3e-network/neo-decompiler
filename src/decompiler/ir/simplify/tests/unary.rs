use super::*;

#[test]
fn test_double_negation() {
    let expr = Expr::unary(
        UnaryOp::LogicalNot,
        Expr::unary(UnaryOp::LogicalNot, Expr::var("x")),
    );
    assert_eq!(simplify(expr), Expr::var("x"));
}

#[test]
fn test_not_true() {
    let expr = Expr::unary(UnaryOp::LogicalNot, Expr::Literal(Literal::Bool(true)));
    assert_eq!(simplify(expr), Expr::Literal(Literal::Bool(false)));
}

#[test]
fn test_not_false() {
    let expr = Expr::unary(UnaryOp::LogicalNot, Expr::Literal(Literal::Bool(false)));
    assert_eq!(simplify(expr), Expr::Literal(Literal::Bool(true)));
}
