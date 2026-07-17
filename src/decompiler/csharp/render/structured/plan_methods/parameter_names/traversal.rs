//! Recursive expression traversal for helper-parameter inference.

use crate::decompiler::ir::{Block, ControlFlow, Expr, Stmt};

pub(super) fn visit_statement_exprs(statement: &Stmt, f: &mut impl FnMut(&Expr)) {
    match statement {
        Stmt::Assign { value, .. }
        | Stmt::Return(Some(value))
        | Stmt::Throw(Some(value))
        | Stmt::Abort(Some(value))
        | Stmt::ExprStmt(value) => visit_expr(value, f),
        Stmt::Assert { condition, message } => {
            visit_expr(condition, f);
            if let Some(message) = message {
                visit_expr(message, f);
            }
        }
        Stmt::ControlFlow(control) => match control.as_ref() {
            ControlFlow::If {
                condition,
                then_branch,
                else_branch,
            } => {
                visit_expr(condition, f);
                visit_block_exprs(then_branch, f);
                if let Some(branch) = else_branch {
                    visit_block_exprs(branch, f);
                }
            }
            ControlFlow::While { condition, body } | ControlFlow::DoWhile { condition, body } => {
                visit_expr(condition, f);
                visit_block_exprs(body, f);
            }
            ControlFlow::For {
                init,
                condition,
                update,
                body,
            } => {
                if let Some(init) = init.as_deref() {
                    visit_statement_exprs(init, f);
                }
                if let Some(condition) = condition {
                    visit_expr(condition, f);
                }
                if let Some(update) = update {
                    visit_expr(update, f);
                }
                visit_block_exprs(body, f);
            }
            ControlFlow::TryCatch {
                try_body,
                catch_body,
                finally_body,
                ..
            } => {
                visit_block_exprs(try_body, f);
                if let Some(body) = catch_body {
                    visit_block_exprs(body, f);
                }
                if let Some(body) = finally_body {
                    visit_block_exprs(body, f);
                }
            }
            ControlFlow::Switch {
                expr,
                cases,
                default,
            } => {
                visit_expr(expr, f);
                for (case, body) in cases {
                    visit_expr(case, f);
                    visit_block_exprs(body, f);
                }
                if let Some(body) = default {
                    visit_block_exprs(body, f);
                }
            }
        },
        Stmt::Return(None)
        | Stmt::Throw(None)
        | Stmt::Abort(None)
        | Stmt::Comment(_)
        | Stmt::Break
        | Stmt::Continue
        | Stmt::Label(_)
        | Stmt::Goto(_) => {}
    }
}

pub(super) fn visit_block_exprs(block: &Block, f: &mut impl FnMut(&Expr)) {
    for statement in &block.stmts {
        visit_statement_exprs(statement, f);
    }
}

pub(super) fn visit_expr(expression: &Expr, f: &mut impl FnMut(&Expr)) {
    f(expression);
    match expression {
        Expr::Binary { left, right, .. } => {
            visit_expr(left, f);
            visit_expr(right, f);
        }
        Expr::Unary { operand, .. }
        | Expr::Cast { expr: operand, .. }
        | Expr::Convert { value: operand, .. }
        | Expr::IsType { value: operand, .. } => visit_expr(operand, f),
        Expr::Call { args, .. } | Expr::Array(args) | Expr::Struct(args) => {
            for argument in args {
                visit_expr(argument, f);
            }
        }
        Expr::Index { base, index } => {
            visit_expr(base, f);
            visit_expr(index, f);
        }
        Expr::Member { base, .. } => visit_expr(base, f),
        Expr::NewArray { length, .. } => visit_expr(length, f),
        Expr::Map(entries) => {
            for (key, value) in entries {
                visit_expr(key, f);
                visit_expr(value, f);
            }
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            visit_expr(condition, f);
            visit_expr(then_expr, f);
            visit_expr(else_expr, f);
        }
        Expr::Unknown | Expr::Literal(_) | Expr::Variable(_) | Expr::StackTemp(_) => {}
    }
}
