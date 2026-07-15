//! Expression/statement liveness checks used by int32 normalization cleanup.

use crate::decompiler::ir::{ControlFlow, Expr, Stmt};

pub(super) fn statement_uses_variable(statement: &Stmt, variable: &str) -> bool {
    match statement {
        Stmt::Assign { value, .. }
        | Stmt::Return(Some(value))
        | Stmt::Throw(Some(value))
        | Stmt::Abort(Some(value))
        | Stmt::ExprStmt(value) => expression_uses_variable(value, variable),
        Stmt::Assert { condition, message } => {
            expression_uses_variable(condition, variable)
                || message
                    .as_ref()
                    .is_some_and(|message| expression_uses_variable(message, variable))
        }
        Stmt::ControlFlow(control) => match control.as_ref() {
            ControlFlow::If {
                condition,
                then_branch,
                else_branch,
            } => {
                expression_uses_variable(condition, variable)
                    || then_branch
                        .stmts
                        .iter()
                        .any(|statement| statement_uses_variable(statement, variable))
                    || else_branch.as_ref().is_some_and(|branch| {
                        branch
                            .stmts
                            .iter()
                            .any(|statement| statement_uses_variable(statement, variable))
                    })
            }
            ControlFlow::While { condition, body } | ControlFlow::DoWhile { condition, body } => {
                expression_uses_variable(condition, variable)
                    || body
                        .stmts
                        .iter()
                        .any(|statement| statement_uses_variable(statement, variable))
            }
            ControlFlow::For {
                init,
                condition,
                update,
                body,
            } => {
                init.as_ref()
                    .is_some_and(|statement| statement_uses_variable(statement, variable))
                    || condition
                        .as_ref()
                        .is_some_and(|condition| expression_uses_variable(condition, variable))
                    || update
                        .as_ref()
                        .is_some_and(|update| expression_uses_variable(update, variable))
                    || body
                        .stmts
                        .iter()
                        .any(|statement| statement_uses_variable(statement, variable))
            }
            ControlFlow::TryCatch {
                try_body,
                catch_body,
                finally_body,
                ..
            } => {
                try_body
                    .stmts
                    .iter()
                    .any(|statement| statement_uses_variable(statement, variable))
                    || catch_body.as_ref().is_some_and(|body| {
                        body.stmts
                            .iter()
                            .any(|statement| statement_uses_variable(statement, variable))
                    })
                    || finally_body.as_ref().is_some_and(|body| {
                        body.stmts
                            .iter()
                            .any(|statement| statement_uses_variable(statement, variable))
                    })
            }
            ControlFlow::Switch {
                expr,
                cases,
                default,
            } => {
                expression_uses_variable(expr, variable)
                    || cases.iter().any(|(_, body)| {
                        body.stmts
                            .iter()
                            .any(|statement| statement_uses_variable(statement, variable))
                    })
                    || default.as_ref().is_some_and(|body| {
                        body.stmts
                            .iter()
                            .any(|statement| statement_uses_variable(statement, variable))
                    })
            }
        },
        Stmt::Return(None)
        | Stmt::Throw(None)
        | Stmt::Abort(None)
        | Stmt::Comment(_)
        | Stmt::Break
        | Stmt::Continue
        | Stmt::Label(_)
        | Stmt::Goto(_) => false,
    }
}

fn expression_uses_variable(expression: &Expr, variable: &str) -> bool {
    match expression {
        Expr::Variable(name) => name == variable,
        Expr::Binary { left, right, .. } => {
            expression_uses_variable(left, variable) || expression_uses_variable(right, variable)
        }
        Expr::Unary { operand, .. }
        | Expr::Cast { expr: operand, .. }
        | Expr::Convert { value: operand, .. }
        | Expr::IsType { value: operand, .. } => expression_uses_variable(operand, variable),
        Expr::Call { args, .. } | Expr::Array(args) | Expr::Struct(args) => args
            .iter()
            .any(|argument| expression_uses_variable(argument, variable)),
        Expr::Index { base, index } => {
            expression_uses_variable(base, variable) || expression_uses_variable(index, variable)
        }
        Expr::Member { base, .. } => expression_uses_variable(base, variable),
        Expr::NewArray { length, .. } => expression_uses_variable(length, variable),
        Expr::Map(entries) => entries.iter().any(|(key, value)| {
            expression_uses_variable(key, variable) || expression_uses_variable(value, variable)
        }),
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expression_uses_variable(condition, variable)
                || expression_uses_variable(then_expr, variable)
                || expression_uses_variable(else_expr, variable)
        }
        Expr::Unknown | Expr::Literal(_) | Expr::StackTemp(_) => false,
    }
}
