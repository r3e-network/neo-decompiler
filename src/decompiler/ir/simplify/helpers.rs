use super::super::expression::{Expr, Literal};

pub(super) fn is_zero(expr: &Expr) -> bool {
    matches!(expr, Expr::Literal(Literal::Int(0)))
}

pub(super) fn is_one(expr: &Expr) -> bool {
    matches!(expr, Expr::Literal(Literal::Int(1)))
}

pub(super) fn is_all_ones(expr: &Expr) -> bool {
    matches!(expr, Expr::Literal(Literal::Int(-1)))
}

pub(super) fn is_true(expr: &Expr) -> bool {
    matches!(expr, Expr::Literal(Literal::Bool(true)))
}

pub(super) fn is_false(expr: &Expr) -> bool {
    matches!(expr, Expr::Literal(Literal::Bool(false)))
}

pub(super) fn is_same_variable(a: &Expr, b: &Expr) -> bool {
    match (a, b) {
        (Expr::Variable(va), Expr::Variable(vb)) => va == vb,
        _ => false,
    }
}
