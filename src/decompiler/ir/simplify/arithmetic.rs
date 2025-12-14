use super::super::expression::{BinOp, Expr, Literal};

use super::helpers::{is_one, is_same_variable, is_zero};

/// x + 0 → x, 0 + x → x
pub(super) fn simplify_add(left: Expr, right: Expr) -> Expr {
    if is_zero(&right) {
        return left;
    }
    if is_zero(&left) {
        return right;
    }
    Expr::binary(BinOp::Add, left, right)
}

/// x - 0 → x, x - x → 0
pub(super) fn simplify_sub(left: Expr, right: Expr) -> Expr {
    if is_zero(&right) {
        return left;
    }
    if is_same_variable(&left, &right) {
        return Expr::Literal(Literal::Int(0));
    }
    Expr::binary(BinOp::Sub, left, right)
}

/// x * 1 → x, 1 * x → x, x * 0 → 0, 0 * x → 0
pub(super) fn simplify_mul(left: Expr, right: Expr) -> Expr {
    if is_one(&right) {
        return left;
    }
    if is_one(&left) {
        return right;
    }
    if is_zero(&right) || is_zero(&left) {
        return Expr::Literal(Literal::Int(0));
    }
    Expr::binary(BinOp::Mul, left, right)
}

/// x / 1 → x
pub(super) fn simplify_div(left: Expr, right: Expr) -> Expr {
    if is_one(&right) {
        return left;
    }
    Expr::binary(BinOp::Div, left, right)
}

/// x ** 1 → x, x ** 0 → 1
pub(super) fn simplify_pow(left: Expr, right: Expr) -> Expr {
    if is_one(&right) {
        return left;
    }
    if is_zero(&right) {
        return Expr::Literal(Literal::Int(1));
    }
    Expr::binary(BinOp::Pow, left, right)
}
