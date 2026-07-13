use std::collections::BTreeSet;

use crate::decompiler::ir::{Block, ControlFlow, Expr, Stmt};

pub(super) fn collect_block_names(block: &Block, names: &mut BTreeSet<String>) {
    for statement in &block.stmts {
        collect_statement_names(statement, names);
    }
}

pub(super) fn collect_statement_names(statement: &Stmt, names: &mut BTreeSet<String>) {
    match statement {
        Stmt::Assign { target, value } => {
            names.insert(target.clone());
            collect_expr_names(value, names);
        }
        Stmt::Return(value) => {
            if let Some(value) = value {
                collect_expr_names(value, names);
            }
        }
        Stmt::Throw(value) | Stmt::Abort(value) => {
            if let Some(value) = value {
                collect_expr_names(value, names);
            }
        }
        Stmt::Assert { condition, message } => {
            collect_expr_names(condition, names);
            if let Some(message) = message {
                collect_expr_names(message, names);
            }
        }
        Stmt::ExprStmt(value) => collect_expr_names(value, names),
        Stmt::Comment(_) | Stmt::Break | Stmt::Continue | Stmt::Label(_) | Stmt::Goto(_) => {}
        Stmt::ControlFlow(control) => collect_control_names(control, names),
    }
}

fn collect_control_names(control: &ControlFlow, names: &mut BTreeSet<String>) {
    match control {
        ControlFlow::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_expr_names(condition, names);
            collect_block_names(then_branch, names);
            if let Some(branch) = else_branch {
                collect_block_names(branch, names);
            }
        }
        ControlFlow::While { condition, body } => {
            collect_expr_names(condition, names);
            collect_block_names(body, names);
        }
        ControlFlow::DoWhile { body, condition } => {
            collect_block_names(body, names);
            collect_expr_names(condition, names);
        }
        ControlFlow::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                collect_statement_names(init, names);
            }
            if let Some(condition) = condition {
                collect_expr_names(condition, names);
            }
            if let Some(update) = update {
                collect_expr_names(update, names);
            }
            collect_block_names(body, names);
        }
        ControlFlow::TryCatch {
            try_body,
            catch_var,
            catch_body,
            finally_body,
        } => {
            collect_block_names(try_body, names);
            if let Some(catch_var) = catch_var {
                names.insert(catch_var.clone());
            }
            if let Some(body) = catch_body {
                collect_block_names(body, names);
            }
            if let Some(body) = finally_body {
                collect_block_names(body, names);
            }
        }
        ControlFlow::Switch {
            expr,
            cases,
            default,
        } => {
            collect_expr_names(expr, names);
            for (value, body) in cases {
                collect_expr_names(value, names);
                collect_block_names(body, names);
            }
            if let Some(body) = default {
                collect_block_names(body, names);
            }
        }
    }
}

fn collect_expr_names(expression: &Expr, names: &mut BTreeSet<String>) {
    match expression {
        Expr::Variable(name) => {
            names.insert(name.clone());
        }
        Expr::Binary { left, right, .. } => {
            collect_expr_names(left, names);
            collect_expr_names(right, names);
        }
        Expr::Unary { operand, .. } => collect_expr_names(operand, names),
        Expr::Call { args, .. } | Expr::Array(args) => {
            for argument in args {
                collect_expr_names(argument, names);
            }
        }
        Expr::Index { base, index } => {
            collect_expr_names(base, names);
            collect_expr_names(index, names);
        }
        Expr::Member { base, .. } => collect_expr_names(base, names),
        Expr::Cast { expr, .. } => collect_expr_names(expr, names),
        Expr::Convert { value, .. } | Expr::IsType { value, .. } => {
            collect_expr_names(value, names);
        }
        Expr::NewArray { length, .. } => collect_expr_names(length, names),
        Expr::Map(pairs) => {
            for (key, value) in pairs {
                collect_expr_names(key, names);
                collect_expr_names(value, names);
            }
        }
        Expr::Struct(values) => {
            for value in values {
                collect_expr_names(value, names);
            }
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_expr_names(condition, names);
            collect_expr_names(then_expr, names);
            collect_expr_names(else_expr, names);
        }
        Expr::Unknown | Expr::Literal(_) | Expr::StackTemp(_) => {}
    }
}
