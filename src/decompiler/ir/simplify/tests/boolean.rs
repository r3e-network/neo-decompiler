use super::*;

#[test]
fn test_eq_true() {
    let expr = Expr::binary(
        BinOp::Eq,
        Expr::var("x"),
        Expr::Literal(Literal::Bool(true)),
    );
    assert_eq!(simplify(expr), Expr::var("x"));
}

#[test]
fn test_eq_false() {
    let expr = Expr::binary(
        BinOp::Eq,
        Expr::var("x"),
        Expr::Literal(Literal::Bool(false)),
    );
    assert_eq!(
        simplify(expr),
        Expr::unary(UnaryOp::LogicalNot, Expr::var("x"))
    );
}

#[test]
fn test_and_true() {
    let expr = Expr::binary(
        BinOp::LogicalAnd,
        Expr::Literal(Literal::Bool(true)),
        Expr::var("x"),
    );
    assert_eq!(simplify(expr), Expr::var("x"));
}

#[test]
fn test_and_false() {
    let expr = Expr::binary(
        BinOp::LogicalAnd,
        Expr::var("x"),
        Expr::Literal(Literal::Bool(false)),
    );
    assert_eq!(simplify(expr), Expr::Literal(Literal::Bool(false)));
}

#[test]
fn test_or_true() {
    let expr = Expr::binary(
        BinOp::LogicalOr,
        Expr::Literal(Literal::Bool(true)),
        Expr::var("x"),
    );
    assert_eq!(simplify(expr), Expr::Literal(Literal::Bool(true)));
}

#[test]
fn test_or_false() {
    let expr = Expr::binary(
        BinOp::LogicalOr,
        Expr::Literal(Literal::Bool(false)),
        Expr::var("x"),
    );
    assert_eq!(simplify(expr), Expr::var("x"));
}
