//! Conversion from IR to SSA form.
//!
//! This module handles the conversion of IR expressions and statements
//! into their SSA equivalents, handling variable versioning and φ nodes.

use crate::decompiler::ir::{Expr, Stmt};

use super::form::{SsaExpr, SsaStmt};
use super::variable::SsaVariable;

/// Convert an IR expression to an SSA expression.
pub fn expr_to_ssa(expr: &Expr) -> SsaExpr {
    match expr {
        Expr::Literal(lit) => SsaExpr::Literal(lit.clone()),
        Expr::Variable(name) => {
            // For now, use the variable name as-is
            // Full implementation would look up the current SSA version
            SsaExpr::var(SsaVariable::initial(name.clone()))
        }
        Expr::Binary { op, left, right } => {
            SsaExpr::binary(*op, expr_to_ssa(left), expr_to_ssa(right))
        }
        Expr::Unary { op, operand } => SsaExpr::unary(*op, expr_to_ssa(operand)),
        Expr::Call { name, args } => {
            SsaExpr::call(name.clone(), args.iter().map(expr_to_ssa).collect())
        }
        Expr::Index { base, index } => SsaExpr::Index {
            base: Box::new(expr_to_ssa(base)),
            index: Box::new(expr_to_ssa(index)),
        },
        Expr::Member { base, name } => SsaExpr::Member {
            base: Box::new(expr_to_ssa(base)),
            name: name.clone(),
        },
        Expr::Cast { expr, target_type } => SsaExpr::Cast {
            expr: Box::new(expr_to_ssa(expr)),
            target_type: target_type.clone(),
        },
        Expr::Array(elements) => SsaExpr::Array(elements.iter().map(expr_to_ssa).collect()),
        Expr::Map(pairs) => SsaExpr::Map(
            pairs
                .iter()
                .map(|(k, v)| (expr_to_ssa(k), expr_to_ssa(v)))
                .collect(),
        ),
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => SsaExpr::Ternary {
            condition: Box::new(expr_to_ssa(condition)),
            then_expr: Box::new(expr_to_ssa(then_expr)),
            else_expr: Box::new(expr_to_ssa(else_expr)),
        },
        Expr::StackTemp(n) => {
            // Stack temporaries become SSA variables with a special naming
            SsaExpr::var(SsaVariable::initial(format!("stack_{}", n)))
        }
    }
}

/// Convert an IR statement to an SSA statement.
///
/// Note: This is a simplified conversion that wraps IR statements in `SsaStmt::Other`.
/// Full implementation would convert all statement types to proper SSA form.
pub fn stmt_to_ssa(stmt: &Stmt) -> SsaStmt {
    match stmt {
        Stmt::Assign { target, value } => {
            let target_var = SsaVariable::initial(target.clone());
            SsaStmt::assign(target_var, expr_to_ssa(value))
        }
        // For statements that don't define SSA variables, wrap them as-is
        _ => SsaStmt::other(stmt.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decompiler::ir::{BinOp, Literal};

    #[test]
    fn test_expr_to_ssa_literal() {
        let expr = Expr::Literal(Literal::Int(42));
        let ssa_expr = expr_to_ssa(&expr);
        assert!(matches!(ssa_expr, SsaExpr::Literal(Literal::Int(42))));
    }

    #[test]
    fn test_expr_to_ssa_variable() {
        let expr = Expr::var("x");
        let ssa_expr = expr_to_ssa(&expr);
        assert!(matches!(ssa_expr, SsaExpr::Variable(_)));
    }

    #[test]
    fn test_expr_to_ssa_binary() {
        let expr = Expr::binary(BinOp::Add, Expr::int(1), Expr::int(2));
        let ssa_expr = expr_to_ssa(&expr);
        assert!(matches!(ssa_expr, SsaExpr::Binary { .. }));
    }

    #[test]
    fn test_stmt_to_ssa_assign() {
        let stmt = Stmt::assign("x", Expr::int(42));
        let ssa_stmt = stmt_to_ssa(&stmt);
        assert!(matches!(ssa_stmt, SsaStmt::Assign { .. }));
    }
}
