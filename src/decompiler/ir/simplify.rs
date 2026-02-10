//! Expression simplification pass.
//!
//! Applies algebraic simplifications to reduce expression complexity:
//! - Arithmetic identities (x + 0 → x, x * 1 → x, etc.)
//! - Boolean simplifications (x == true → x, !!x → x, etc.)
//! - Constant folding for simple cases

use super::expression::Expr;

mod arithmetic;
mod bitwise;
mod boolean;
mod dispatch;
mod helpers;
mod walk;

#[cfg(test)]
mod tests;

/// Simplify an expression by applying algebraic rules.
///
/// This function recursively simplifies subexpressions and then applies
/// simplification rules to the result.
#[must_use]
pub fn simplify(expr: Expr) -> Expr {
    // First, recursively simplify subexpressions
    let expr = walk::simplify_children(expr);

    // Then apply simplification rules
    match expr {
        Expr::Binary { op, left, right } => dispatch::simplify_binary(op, *left, *right),
        Expr::Unary { op, operand } => dispatch::simplify_unary(op, *operand),
        other => other,
    }
}
