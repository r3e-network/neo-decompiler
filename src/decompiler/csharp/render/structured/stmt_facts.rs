//! Prefix definition facts used to validate indexed-loop recovery.

use std::collections::BTreeMap;

use crate::decompiler::ir::{Block, ControlFlow, Expr, Stmt};

pub(super) type DefinitionFacts = BTreeMap<String, Expr>;

pub(super) fn update_definition_facts(facts: &mut DefinitionFacts, statement: &Stmt) {
    match statement {
        Stmt::Assign { target, value } => {
            if expr_contains_call(value) {
                facts.clear();
            }
            facts.insert(target.clone(), value.clone());
        }
        Stmt::ExprStmt(value) if expr_contains_call(value) => facts.clear(),
        Stmt::ControlFlow(control) if control_defines_variable(control) => facts.clear(),
        _ => {}
    }
}

fn control_defines_variable(control: &ControlFlow) -> bool {
    match control {
        ControlFlow::If {
            then_branch,
            else_branch,
            ..
        } => {
            block_defines_variable(then_branch)
                || else_branch.as_ref().is_some_and(block_defines_variable)
        }
        ControlFlow::While { body, .. } | ControlFlow::DoWhile { body, .. } => {
            block_defines_variable(body)
        }
        ControlFlow::For { init, body, .. } => {
            init.as_deref().is_some_and(statement_defines_variable) || block_defines_variable(body)
        }
        ControlFlow::TryCatch {
            try_body,
            catch_body,
            finally_body,
            ..
        } => {
            block_defines_variable(try_body)
                || catch_body.as_ref().is_some_and(block_defines_variable)
                || finally_body.as_ref().is_some_and(block_defines_variable)
        }
        ControlFlow::Switch { cases, default, .. } => {
            cases.iter().any(|(_, body)| block_defines_variable(body))
                || default.as_ref().is_some_and(block_defines_variable)
        }
    }
}

fn block_defines_variable(block: &Block) -> bool {
    block.stmts.iter().any(statement_defines_variable)
}

fn statement_defines_variable(statement: &Stmt) -> bool {
    match statement {
        Stmt::Assign { .. } => true,
        Stmt::ControlFlow(control) => control_defines_variable(control),
        _ => false,
    }
}

fn expr_contains_call(expression: &Expr) -> bool {
    match expression {
        Expr::Call { .. } => true,
        Expr::Binary { left, right, .. } => expr_contains_call(left) || expr_contains_call(right),
        Expr::Unary { operand, .. }
        | Expr::Cast { expr: operand, .. }
        | Expr::Convert { value: operand, .. }
        | Expr::IsType { value: operand, .. } => expr_contains_call(operand),
        Expr::Index { base, index } => expr_contains_call(base) || expr_contains_call(index),
        Expr::Member { base, .. } => expr_contains_call(base),
        Expr::NewArray { length, .. } => expr_contains_call(length),
        Expr::Array(values) | Expr::Struct(values) => values.iter().any(expr_contains_call),
        Expr::Map(entries) => entries
            .iter()
            .any(|(key, value)| expr_contains_call(key) || expr_contains_call(value)),
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_contains_call(condition)
                || expr_contains_call(then_expr)
                || expr_contains_call(else_expr)
        }
        Expr::Literal(_) | Expr::Variable(_) | Expr::Unknown | Expr::StackTemp(_) => false,
    }
}
