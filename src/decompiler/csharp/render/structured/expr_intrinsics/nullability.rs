use crate::decompiler::analysis::types::ValueType;
use crate::decompiler::ir::{BinOp, Expr, Literal, UnaryOp};

use super::super::expr::ExprContext;
use super::super::plan::csharp_type_value_type;

/// VM value categories are broader than C# nullability. In particular, a
/// `PICKITEM` from an object array may be inferred as an integer from its
/// consumers while still containing a runtime null. Fold `ISNULL` only when
/// the expression itself proves a non-null result.
pub(super) fn definitely_non_null_value(expression: &Expr, context: &ExprContext) -> bool {
    match expression {
        Expr::Literal(literal) => !matches!(literal, Literal::Null),
        Expr::Variable(_) => exact_non_nullable_value(expression, context),
        Expr::Binary { op, left, right } => match op {
            BinOp::Eq
            | BinOp::Ne
            | BinOp::Lt
            | BinOp::Le
            | BinOp::Gt
            | BinOp::Ge
            | BinOp::LogicalAnd
            | BinOp::LogicalOr => true,
            _ => {
                definitely_non_null_value(left, context)
                    && definitely_non_null_value(right, context)
            }
        },
        Expr::Unary { op, operand } => match op {
            UnaryOp::LogicalNot => true,
            UnaryOp::Neg
            | UnaryOp::Not
            | UnaryOp::Inc
            | UnaryOp::Dec
            | UnaryOp::Abs
            | UnaryOp::Sign => definitely_non_null_value(operand, context),
        },
        Expr::Cast { target_type, .. } => {
            csharp_type_value_type(target_type).is_some_and(is_non_nullable_value_type)
        }
        Expr::Convert { target, .. } => is_non_nullable_value_type(*target),
        Expr::IsType { .. } => true,
        Expr::Call { .. } | Expr::Index { .. } | Expr::Member { .. } => {
            exact_non_nullable_value(expression, context)
        }
        Expr::NewArray { .. } | Expr::Array(_) | Expr::Struct(_) | Expr::Map(_) => true,
        Expr::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            definitely_non_null_value(then_expr, context)
                && definitely_non_null_value(else_expr, context)
        }
        Expr::Unknown | Expr::StackTemp(_) => false,
    }
}

fn exact_non_nullable_value(expression: &Expr, context: &ExprContext) -> bool {
    context
        .exact_csharp_type(expression)
        .and_then(csharp_type_value_type)
        .is_some_and(is_non_nullable_value_type)
}

fn is_non_nullable_value_type(value_type: ValueType) -> bool {
    matches!(value_type, ValueType::Boolean | ValueType::Integer)
}
