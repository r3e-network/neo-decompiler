//! Redundant cast collapsing for structured IR.

use crate::decompiler::ir::{Block as IrBlock, Expr, Literal};

use super::support::mutate_stmt_exprs;

// Redundant cast collapsing
// ---------------------------------------------------------------------------

pub(super) fn simplify_casts(block: &mut IrBlock) {
    for statement in &mut block.stmts {
        mutate_stmt_exprs(statement, &mut simplify_cast_expr);
    }
}

fn simplify_cast_expr(expr: &mut Expr) {
    loop {
        let Expr::Cast {
            expr: inner,
            target_type,
        } = expr
        else {
            return;
        };
        // `(T)((T)x)` or `(T)(dynamic)x` collapses to `(T)x`.
        if let Expr::Cast {
            expr: inner_inner,
            target_type: inner_type,
        } = inner.as_ref()
        {
            if inner_type == target_type
                || inner_type.eq_ignore_ascii_case("dynamic")
                || inner_type.eq_ignore_ascii_case("object")
            {
                *expr = Expr::Cast {
                    expr: inner_inner.clone(),
                    target_type: std::mem::take(target_type),
                };
                continue;
            }
        }
        if literal_cast_is_redundant(inner, target_type) {
            *expr = (**inner).clone();
            continue;
        }
        return;
    }
}

/// C# already types literals, so casts of literals to their natural type are
/// noise (`(int)1`, `(string)"a"`). Casts to *other* numeric types change the
/// expression's C# type (overload resolution, array element types) and are
/// kept even for constants.
fn literal_cast_is_redundant(inner: &Expr, target_type: &str) -> bool {
    let Expr::Literal(literal) = inner else {
        return false;
    };
    match literal {
        Literal::Int(_) => matches!(target_type, "int" | "long" | "BigInteger"),
        Literal::BigInt(_) => target_type == "BigInteger",
        Literal::String(_) => target_type == "string",
        Literal::Bool(_) => target_type == "bool",
        Literal::Bytes(_) | Literal::Null => false,
    }
}
