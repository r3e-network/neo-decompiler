//! Recursive safety guards for indexed-loop recovery.

use crate::decompiler::ir::{Block, ControlFlow, Expr, Intrinsic, SemanticCallTarget, Stmt};
use crate::instruction::OpCode;

pub(super) fn block_mentions_variable(block: &Block, name: &str) -> bool {
    block
        .stmts
        .iter()
        .any(|statement| statement_mentions_variable(statement, name))
}

pub(super) fn block_assigns_variable(block: &Block, name: &str) -> bool {
    block
        .stmts
        .iter()
        .any(|statement| statement_assigns_variable(statement, name))
}

fn statement_mentions_variable(statement: &Stmt, name: &str) -> bool {
    match statement {
        Stmt::Assign { target, value } => target == name || expr_mentions_variable(value, name),
        Stmt::Return(value) | Stmt::Throw(value) | Stmt::Abort(value) => value
            .as_ref()
            .is_some_and(|value| expr_mentions_variable(value, name)),
        Stmt::Assert { condition, message } => {
            expr_mentions_variable(condition, name)
                || message
                    .as_ref()
                    .is_some_and(|message| expr_mentions_variable(message, name))
        }
        Stmt::ExprStmt(value) => expr_mentions_variable(value, name),
        Stmt::ControlFlow(control) => control_mentions_variable(control, name),
        Stmt::Comment(_) | Stmt::Break | Stmt::Continue | Stmt::Label(_) | Stmt::Goto(_) => false,
    }
}

fn statement_assigns_variable(statement: &Stmt, name: &str) -> bool {
    match statement {
        Stmt::Assign { target, .. } => target == name,
        Stmt::ControlFlow(control) => control_assigns_variable(control, name),
        _ => false,
    }
}

fn control_mentions_variable(control: &ControlFlow, name: &str) -> bool {
    match control {
        ControlFlow::If {
            condition,
            then_branch,
            else_branch,
        } => {
            expr_mentions_variable(condition, name)
                || block_mentions_variable(then_branch, name)
                || else_branch
                    .as_ref()
                    .is_some_and(|branch| block_mentions_variable(branch, name))
        }
        ControlFlow::While { condition, body } => {
            expr_mentions_variable(condition, name) || block_mentions_variable(body, name)
        }
        ControlFlow::DoWhile { body, condition } => {
            block_mentions_variable(body, name) || expr_mentions_variable(condition, name)
        }
        ControlFlow::For {
            init,
            condition,
            update,
            body,
        } => {
            init.as_ref()
                .is_some_and(|init| statement_mentions_variable(init, name))
                || condition
                    .as_ref()
                    .is_some_and(|condition| expr_mentions_variable(condition, name))
                || update
                    .as_ref()
                    .is_some_and(|update| expr_mentions_variable(update, name))
                || block_mentions_variable(body, name)
        }
        ControlFlow::TryCatch {
            try_body,
            catch_var,
            catch_body,
            finally_body,
        } => {
            block_mentions_variable(try_body, name)
                || catch_var.as_deref() == Some(name)
                || catch_body
                    .as_ref()
                    .is_some_and(|body| block_mentions_variable(body, name))
                || finally_body
                    .as_ref()
                    .is_some_and(|body| block_mentions_variable(body, name))
        }
        ControlFlow::Switch {
            expr,
            cases,
            default,
        } => {
            expr_mentions_variable(expr, name)
                || cases.iter().any(|(value, body)| {
                    expr_mentions_variable(value, name) || block_mentions_variable(body, name)
                })
                || default
                    .as_ref()
                    .is_some_and(|body| block_mentions_variable(body, name))
        }
    }
}

fn control_assigns_variable(control: &ControlFlow, name: &str) -> bool {
    match control {
        ControlFlow::If {
            then_branch,
            else_branch,
            ..
        } => {
            block_assigns_variable(then_branch, name)
                || else_branch
                    .as_ref()
                    .is_some_and(|branch| block_assigns_variable(branch, name))
        }
        ControlFlow::While { body, .. } | ControlFlow::DoWhile { body, .. } => {
            block_assigns_variable(body, name)
        }
        ControlFlow::For { init, body, .. } => {
            init.as_ref()
                .is_some_and(|init| statement_assigns_variable(init, name))
                || block_assigns_variable(body, name)
        }
        ControlFlow::TryCatch {
            try_body,
            catch_body,
            finally_body,
            ..
        } => {
            block_assigns_variable(try_body, name)
                || catch_body
                    .as_ref()
                    .is_some_and(|body| block_assigns_variable(body, name))
                || finally_body
                    .as_ref()
                    .is_some_and(|body| block_assigns_variable(body, name))
        }
        ControlFlow::Switch { cases, default, .. } => {
            cases
                .iter()
                .any(|(_, body)| block_assigns_variable(body, name))
                || default
                    .as_ref()
                    .is_some_and(|body| block_assigns_variable(body, name))
        }
    }
}

fn expr_mentions_variable(expression: &Expr, name: &str) -> bool {
    match expression {
        Expr::Variable(value) => value == name,
        Expr::Binary { left, right, .. } => {
            expr_mentions_variable(left, name) || expr_mentions_variable(right, name)
        }
        Expr::Unary { operand, .. }
        | Expr::Cast { expr: operand, .. }
        | Expr::Convert { value: operand, .. }
        | Expr::IsType { value: operand, .. } => expr_mentions_variable(operand, name),
        Expr::Call { args, .. } | Expr::Array(args) | Expr::Struct(args) => args
            .iter()
            .any(|argument| expr_mentions_variable(argument, name)),
        Expr::Index { base, index } => {
            expr_mentions_variable(base, name) || expr_mentions_variable(index, name)
        }
        Expr::Member { base, .. } => expr_mentions_variable(base, name),
        Expr::NewArray { length, .. } => expr_mentions_variable(length, name),
        Expr::Map(pairs) => pairs.iter().any(|(key, value)| {
            expr_mentions_variable(key, name) || expr_mentions_variable(value, name)
        }),
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_mentions_variable(condition, name)
                || expr_mentions_variable(then_expr, name)
                || expr_mentions_variable(else_expr, name)
        }
        Expr::Literal(_) | Expr::Unknown | Expr::StackTemp(_) => false,
    }
}

pub(super) fn block_has_opaque_calls(block: &Block) -> bool {
    block.stmts.iter().any(statement_has_opaque_calls)
}

fn statement_has_opaque_calls(statement: &Stmt) -> bool {
    match statement {
        Stmt::Assign { value, .. }
        | Stmt::ExprStmt(value)
        | Stmt::Return(Some(value))
        | Stmt::Throw(Some(value))
        | Stmt::Abort(Some(value)) => expr_has_opaque_call(value),
        Stmt::Assert { condition, message } => {
            expr_has_opaque_call(condition) || message.as_ref().is_some_and(expr_has_opaque_call)
        }
        Stmt::ControlFlow(control) => control_has_opaque_calls(control),
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

fn control_has_opaque_calls(control: &ControlFlow) -> bool {
    match control {
        ControlFlow::If {
            condition,
            then_branch,
            else_branch,
        } => {
            expr_has_opaque_call(condition)
                || block_has_opaque_calls(then_branch)
                || else_branch.as_ref().is_some_and(block_has_opaque_calls)
        }
        ControlFlow::While { condition, body } => {
            expr_has_opaque_call(condition) || block_has_opaque_calls(body)
        }
        ControlFlow::DoWhile { body, condition } => {
            block_has_opaque_calls(body) || expr_has_opaque_call(condition)
        }
        ControlFlow::For {
            init,
            condition,
            update,
            body,
        } => {
            init.as_ref()
                .is_some_and(|init| statement_has_opaque_calls(init))
                || condition.as_ref().is_some_and(expr_has_opaque_call)
                || update.as_ref().is_some_and(expr_has_opaque_call)
                || block_has_opaque_calls(body)
        }
        ControlFlow::TryCatch {
            try_body,
            catch_body,
            finally_body,
            ..
        } => {
            block_has_opaque_calls(try_body)
                || catch_body.as_ref().is_some_and(block_has_opaque_calls)
                || finally_body.as_ref().is_some_and(block_has_opaque_calls)
        }
        ControlFlow::Switch {
            expr,
            cases,
            default,
        } => {
            expr_has_opaque_call(expr)
                || cases.iter().any(|(value, body)| {
                    expr_has_opaque_call(value) || block_has_opaque_calls(body)
                })
                || default.as_ref().is_some_and(block_has_opaque_calls)
        }
    }
}

fn expr_has_opaque_call(expression: &Expr) -> bool {
    match expression {
        Expr::Call { target, args } => {
            !matches!(target, SemanticCallTarget::Intrinsic(_))
                || args.iter().any(expr_has_opaque_call)
                || matches!(
                    target,
                    SemanticCallTarget::Intrinsic(Intrinsic::Opcode(
                        OpCode::Append
                            | OpCode::Setitem
                            | OpCode::Memcpy
                            | OpCode::Remove
                            | OpCode::Clearitems
                            | OpCode::Reverseitems
                            | OpCode::Popitem
                    ))
                )
        }
        Expr::Binary { left, right, .. } => {
            expr_has_opaque_call(left) || expr_has_opaque_call(right)
        }
        Expr::Unary { operand, .. }
        | Expr::Cast { expr: operand, .. }
        | Expr::Convert { value: operand, .. }
        | Expr::IsType { value: operand, .. } => expr_has_opaque_call(operand),
        Expr::Index { base, index } => expr_has_opaque_call(base) || expr_has_opaque_call(index),
        Expr::Member { base, .. } => expr_has_opaque_call(base),
        Expr::NewArray { length, .. } => expr_has_opaque_call(length),
        Expr::Array(values) | Expr::Struct(values) => values.iter().any(expr_has_opaque_call),
        Expr::Map(pairs) => pairs
            .iter()
            .any(|(key, value)| expr_has_opaque_call(key) || expr_has_opaque_call(value)),
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_has_opaque_call(condition)
                || expr_has_opaque_call(then_expr)
                || expr_has_opaque_call(else_expr)
        }
        Expr::Literal(_) | Expr::Variable(_) | Expr::Unknown | Expr::StackTemp(_) => false,
    }
}

pub(super) fn block_writes_static(block: &Block) -> bool {
    block.stmts.iter().any(statement_writes_static)
}

fn statement_writes_static(statement: &Stmt) -> bool {
    match statement {
        Stmt::Assign { target, .. } => target.starts_with("static"),
        Stmt::ControlFlow(control) => control_writes_static(control),
        _ => false,
    }
}

fn control_writes_static(control: &ControlFlow) -> bool {
    match control {
        ControlFlow::If {
            then_branch,
            else_branch,
            ..
        } => {
            block_writes_static(then_branch)
                || else_branch.as_ref().is_some_and(block_writes_static)
        }
        ControlFlow::While { body, .. } | ControlFlow::DoWhile { body, .. } => {
            block_writes_static(body)
        }
        ControlFlow::For { init, body, .. } => {
            init.as_ref()
                .is_some_and(|init| statement_writes_static(init))
                || block_writes_static(body)
        }
        ControlFlow::TryCatch {
            try_body,
            catch_body,
            finally_body,
            ..
        } => {
            block_writes_static(try_body)
                || catch_body.as_ref().is_some_and(block_writes_static)
                || finally_body.as_ref().is_some_and(block_writes_static)
        }
        ControlFlow::Switch { cases, default, .. } => {
            cases.iter().any(|(_, body)| block_writes_static(body))
                || default.as_ref().is_some_and(block_writes_static)
        }
    }
}
