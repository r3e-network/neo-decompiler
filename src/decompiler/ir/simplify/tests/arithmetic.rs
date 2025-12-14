use super::*;

#[test]
fn test_add_zero() {
    let expr = Expr::binary(BinOp::Add, Expr::var("x"), Expr::int(0));
    assert_eq!(simplify(expr), Expr::var("x"));

    let expr = Expr::binary(BinOp::Add, Expr::int(0), Expr::var("y"));
    assert_eq!(simplify(expr), Expr::var("y"));
}

#[test]
fn test_mul_one() {
    let expr = Expr::binary(BinOp::Mul, Expr::var("x"), Expr::int(1));
    assert_eq!(simplify(expr), Expr::var("x"));

    let expr = Expr::binary(BinOp::Mul, Expr::int(1), Expr::var("y"));
    assert_eq!(simplify(expr), Expr::var("y"));
}

#[test]
fn test_mul_zero() {
    let expr = Expr::binary(BinOp::Mul, Expr::var("x"), Expr::int(0));
    assert_eq!(simplify(expr), Expr::int(0));
}

#[test]
fn test_sub_self() {
    let expr = Expr::binary(BinOp::Sub, Expr::var("x"), Expr::var("x"));
    assert_eq!(simplify(expr), Expr::int(0));
}

#[test]
fn test_pow_zero() {
    let expr = Expr::binary(BinOp::Pow, Expr::var("x"), Expr::int(0));
    assert_eq!(simplify(expr), Expr::int(1));
}

#[test]
fn test_pow_one() {
    let expr = Expr::binary(BinOp::Pow, Expr::var("x"), Expr::int(1));
    assert_eq!(simplify(expr), Expr::var("x"));
}

#[test]
fn test_nested_simplification() {
    let expr = Expr::binary(
        BinOp::Mul,
        Expr::binary(BinOp::Add, Expr::var("x"), Expr::int(0)),
        Expr::int(1),
    );
    assert_eq!(simplify(expr), Expr::var("x"));
}
