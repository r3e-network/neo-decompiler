//! Shared IR traversal helpers for temporary-reduction passes.

use crate::decompiler::ir::{Block as IrBlock, ControlFlow, Expr, Stmt};
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Global use counting
// ---------------------------------------------------------------------------

pub(super) struct UseCounts {
    totals: BTreeMap<String, usize>,
}

impl UseCounts {
    pub(super) fn of(block: &IrBlock) -> Self {
        let mut totals = BTreeMap::new();
        for statement in &block.stmts {
            visit_stmt_exprs(statement, &mut |expr| {
                if let Expr::Variable(name) = expr {
                    *totals.entry(name.clone()).or_insert(0usize) += 1;
                }
            });
        }
        Self { totals }
    }

    pub(super) fn total(&self, variable: &str) -> usize {
        self.totals.get(variable).copied().unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// Shared read-only visitors
// ---------------------------------------------------------------------------

pub(super) fn for_each_child_block_mut(statement: &mut Stmt, f: &mut impl FnMut(&mut IrBlock)) {
    let Stmt::ControlFlow(control) = statement else {
        return;
    };
    match control.as_mut() {
        ControlFlow::If {
            then_branch,
            else_branch,
            ..
        } => {
            f(then_branch);
            if let Some(branch) = else_branch {
                f(branch);
            }
        }
        ControlFlow::While { body, .. } | ControlFlow::DoWhile { body, .. } => f(body),
        ControlFlow::For { init, body, .. } => {
            if let Some(init) = init.as_deref_mut() {
                for_each_child_block_mut(init, f);
            }
            f(body);
        }
        ControlFlow::TryCatch {
            try_body,
            catch_body,
            finally_body,
            ..
        } => {
            f(try_body);
            if let Some(body) = catch_body {
                f(body);
            }
            if let Some(body) = finally_body {
                f(body);
            }
        }
        ControlFlow::Switch { cases, default, .. } => {
            for (_, body) in cases {
                f(body);
            }
            if let Some(body) = default {
                f(body);
            }
        }
    }
}

pub(super) fn visit_stmt_exprs(statement: &Stmt, f: &mut impl FnMut(&Expr)) {
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
                    visit_stmt_exprs(init, f);
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
                for (_, body) in cases {
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

pub(super) fn visit_block_exprs(block: &IrBlock, f: &mut impl FnMut(&Expr)) {
    for statement in &block.stmts {
        visit_stmt_exprs(statement, f);
    }
}

pub(super) fn visit_expr(expr: &Expr, f: &mut impl FnMut(&Expr)) {
    match expr {
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
    f(expr);
}

// ---------------------------------------------------------------------------
// Mutating expression walker (postorder, used for rewrites)
// ---------------------------------------------------------------------------

pub(super) fn mutate_stmt_exprs(statement: &mut Stmt, f: &mut impl FnMut(&mut Expr)) {
    match statement {
        Stmt::Assign { value, .. }
        | Stmt::Return(Some(value))
        | Stmt::Throw(Some(value))
        | Stmt::Abort(Some(value))
        | Stmt::ExprStmt(value) => mutate_expr_postorder(value, f),
        Stmt::Assert { condition, message } => {
            mutate_expr_postorder(condition, f);
            if let Some(message) = message {
                mutate_expr_postorder(message, f);
            }
        }
        Stmt::ControlFlow(control) => match control.as_mut() {
            ControlFlow::If {
                condition,
                then_branch,
                else_branch,
            } => {
                mutate_expr_postorder(condition, f);
                mutate_block_exprs(then_branch, f);
                if let Some(branch) = else_branch {
                    mutate_block_exprs(branch, f);
                }
            }
            ControlFlow::While { condition, body } | ControlFlow::DoWhile { condition, body } => {
                mutate_expr_postorder(condition, f);
                mutate_block_exprs(body, f);
            }
            ControlFlow::For {
                init,
                condition,
                update,
                body,
            } => {
                if let Some(init) = init.as_deref_mut() {
                    mutate_stmt_exprs(init, f);
                }
                if let Some(condition) = condition {
                    mutate_expr_postorder(condition, f);
                }
                if let Some(update) = update {
                    mutate_expr_postorder(update, f);
                }
                mutate_block_exprs(body, f);
            }
            ControlFlow::TryCatch {
                try_body,
                catch_body,
                finally_body,
                ..
            } => {
                mutate_block_exprs(try_body, f);
                if let Some(body) = catch_body {
                    mutate_block_exprs(body, f);
                }
                if let Some(body) = finally_body {
                    mutate_block_exprs(body, f);
                }
            }
            ControlFlow::Switch {
                expr,
                cases,
                default,
            } => {
                mutate_expr_postorder(expr, f);
                for (_, body) in cases {
                    mutate_block_exprs(body, f);
                }
                if let Some(body) = default {
                    mutate_block_exprs(body, f);
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

/// Replace every occurrence of `variable` with `value`. Callers guarantee the
/// variable has a single use and `value` does not mention it.
pub(super) fn substitute_variable(block: &mut IrBlock, variable: &str, value: &Expr) {
    for statement in &mut block.stmts {
        mutate_stmt_exprs(statement, &mut |expr| {
            if matches!(expr, Expr::Variable(name) if name == variable) {
                *expr = value.clone();
            }
        });
    }
}

pub(super) fn mutate_block_exprs(block: &mut IrBlock, f: &mut impl FnMut(&mut Expr)) {
    for statement in &mut block.stmts {
        mutate_stmt_exprs(statement, f);
    }
}

pub(super) fn mutate_expr_postorder(expr: &mut Expr, f: &mut impl FnMut(&mut Expr)) {
    match expr {
        Expr::Binary { left, right, .. } => {
            mutate_expr_postorder(left, f);
            mutate_expr_postorder(right, f);
        }
        Expr::Unary { operand, .. }
        | Expr::Cast { expr: operand, .. }
        | Expr::Convert { value: operand, .. }
        | Expr::IsType { value: operand, .. } => mutate_expr_postorder(operand, f),
        Expr::Call { args, .. } | Expr::Array(args) | Expr::Struct(args) => {
            for argument in args {
                mutate_expr_postorder(argument, f);
            }
        }
        Expr::Index { base, index } => {
            mutate_expr_postorder(base, f);
            mutate_expr_postorder(index, f);
        }
        Expr::Member { base, .. } => mutate_expr_postorder(base, f),
        Expr::NewArray { length, .. } => mutate_expr_postorder(length, f),
        Expr::Map(entries) => {
            for (key, value) in entries {
                mutate_expr_postorder(key, f);
                mutate_expr_postorder(value, f);
            }
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            mutate_expr_postorder(condition, f);
            mutate_expr_postorder(then_expr, f);
            mutate_expr_postorder(else_expr, f);
        }
        Expr::Unknown | Expr::Literal(_) | Expr::Variable(_) | Expr::StackTemp(_) => {}
    }
    f(expr);
}
