use super::super::expression::{BinOp, Expr, Literal, UnaryOp};

use super::helpers::{is_false, is_same_variable, is_true};

/// x == true → x, x == false → !x, x == x → true
pub(super) fn simplify_eq(left: Expr, right: Expr) -> Expr {
    if is_true(&right) {
        return left;
    }
    if is_true(&left) {
        return right;
    }
    if is_false(&right) {
        return Expr::unary(UnaryOp::LogicalNot, left);
    }
    if is_false(&left) {
        return Expr::unary(UnaryOp::LogicalNot, right);
    }
    if is_same_variable(&left, &right) {
        return Expr::Literal(Literal::Bool(true));
    }
    Expr::binary(BinOp::Eq, left, right)
}

/// x != true → !x, x != false → x, x != x → false
pub(super) fn simplify_ne(left: Expr, right: Expr) -> Expr {
    if is_true(&right) {
        return Expr::unary(UnaryOp::LogicalNot, left);
    }
    if is_true(&left) {
        return Expr::unary(UnaryOp::LogicalNot, right);
    }
    if is_false(&right) {
        return left;
    }
    if is_false(&left) {
        return right;
    }
    if is_same_variable(&left, &right) {
        return Expr::Literal(Literal::Bool(false));
    }
    Expr::binary(BinOp::Ne, left, right)
}

/// true && x → x, x && true → x, false && x → false, x && false → false
pub(super) fn simplify_and(left: Expr, right: Expr) -> Expr {
    if is_true(&left) {
        return right;
    }
    if is_true(&right) {
        return left;
    }
    if is_false(&left) || is_false(&right) {
        return Expr::Literal(Literal::Bool(false));
    }
    Expr::binary(BinOp::LogicalAnd, left, right)
}

/// true || x → true, x || true → true, false || x → x, x || false → x
pub(super) fn simplify_or(left: Expr, right: Expr) -> Expr {
    if is_true(&left) || is_true(&right) {
        return Expr::Literal(Literal::Bool(true));
    }
    if is_false(&left) {
        return right;
    }
    if is_false(&right) {
        return left;
    }
    Expr::binary(BinOp::LogicalOr, left, right)
}
