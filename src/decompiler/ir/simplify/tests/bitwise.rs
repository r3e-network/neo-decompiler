use super::*;

#[test]
fn test_xor_self() {
    let expr = Expr::binary(BinOp::Xor, Expr::var("x"), Expr::var("x"));
    assert_eq!(simplify(expr), Expr::int(0));
}
