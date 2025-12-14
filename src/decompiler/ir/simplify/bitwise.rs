use super::super::expression::{BinOp, Expr, Literal};

use super::helpers::{is_all_ones, is_same_variable, is_zero};

/// x & 0 → 0, x & -1 → x (all bits set)
pub(super) fn simplify_bitwise_and(left: Expr, right: Expr) -> Expr {
    if is_zero(&left) || is_zero(&right) {
        return Expr::Literal(Literal::Int(0));
    }
    if is_all_ones(&right) {
        return left;
    }
    if is_all_ones(&left) {
        return right;
    }
    Expr::binary(BinOp::And, left, right)
}

/// x | 0 → x, x | -1 → -1
pub(super) fn simplify_bitwise_or(left: Expr, right: Expr) -> Expr {
    if is_zero(&right) {
        return left;
    }
    if is_zero(&left) {
        return right;
    }
    if is_all_ones(&left) || is_all_ones(&right) {
        return Expr::Literal(Literal::Int(-1));
    }
    Expr::binary(BinOp::Or, left, right)
}

/// x ^ 0 → x, x ^ x → 0
pub(super) fn simplify_xor(left: Expr, right: Expr) -> Expr {
    if is_zero(&right) {
        return left;
    }
    if is_zero(&left) {
        return right;
    }
    if is_same_variable(&left, &right) {
        return Expr::Literal(Literal::Int(0));
    }
    Expr::binary(BinOp::Xor, left, right)
}

/// x << 0 → x, x >> 0 → x
pub(super) fn simplify_shift(op: BinOp, left: Expr, right: Expr) -> Expr {
    if is_zero(&right) {
        return left;
    }
    Expr::binary(op, left, right)
}
