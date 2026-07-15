//! Internal-call argument extraction for private-helper inference.

use std::collections::{BTreeMap, HashMap};

use crate::decompiler::cfg::method_body::{StructuredMethodBody, SymbolInfo};
use crate::decompiler::ir::{Block, ControlFlow, Expr, SemanticCallTarget, Stmt};

use super::super::concrete_definition_type_with_symbols_and_known_types_and_calls;

pub(super) fn collect_internal_call_arguments(
    body: &Block,
    lowered: &StructuredMethodBody,
    known_types: &BTreeMap<String, String>,
    known_call_types: &BTreeMap<usize, String>,
) -> HashMap<usize, Vec<Vec<Option<String>>>> {
    let mut calls = HashMap::new();
    collect_block_calls(
        body,
        &mut calls,
        &lowered.symbols,
        known_types,
        known_call_types,
    );
    calls
}

fn collect_block_calls(
    block: &Block,
    calls: &mut HashMap<usize, Vec<Vec<Option<String>>>>,
    symbols: &BTreeMap<String, SymbolInfo>,
    known_types: &BTreeMap<String, String>,
    known_call_types: &BTreeMap<usize, String>,
) {
    for statement in &block.stmts {
        match statement {
            Stmt::Assign { value, .. }
            | Stmt::Return(Some(value))
            | Stmt::Throw(Some(value))
            | Stmt::Abort(Some(value))
            | Stmt::ExprStmt(value) => {
                collect_expr_calls(value, calls, symbols, known_types, known_call_types)
            }
            Stmt::Assert { condition, message } => {
                collect_expr_calls(condition, calls, symbols, known_types, known_call_types);
                if let Some(message) = message {
                    collect_expr_calls(message, calls, symbols, known_types, known_call_types);
                }
            }
            Stmt::ControlFlow(control) => {
                collect_control_calls(control, calls, symbols, known_types, known_call_types)
            }
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
}

fn collect_control_calls(
    control: &ControlFlow,
    calls: &mut HashMap<usize, Vec<Vec<Option<String>>>>,
    symbols: &BTreeMap<String, SymbolInfo>,
    known_types: &BTreeMap<String, String>,
    known_call_types: &BTreeMap<usize, String>,
) {
    match control {
        ControlFlow::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_expr_calls(condition, calls, symbols, known_types, known_call_types);
            collect_block_calls(then_branch, calls, symbols, known_types, known_call_types);
            if let Some(else_branch) = else_branch {
                collect_block_calls(else_branch, calls, symbols, known_types, known_call_types);
            }
        }
        ControlFlow::While { condition, body } => {
            collect_expr_calls(condition, calls, symbols, known_types, known_call_types);
            collect_block_calls(body, calls, symbols, known_types, known_call_types);
        }
        ControlFlow::DoWhile { body, condition } => {
            collect_block_calls(body, calls, symbols, known_types, known_call_types);
            collect_expr_calls(condition, calls, symbols, known_types, known_call_types);
        }
        ControlFlow::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                collect_block_calls(
                    &Block::from(vec![init.as_ref().clone()]),
                    calls,
                    symbols,
                    known_types,
                    known_call_types,
                );
            }
            if let Some(condition) = condition {
                collect_expr_calls(condition, calls, symbols, known_types, known_call_types);
            }
            if let Some(update) = update {
                collect_expr_calls(update, calls, symbols, known_types, known_call_types);
            }
            collect_block_calls(body, calls, symbols, known_types, known_call_types);
        }
        ControlFlow::TryCatch {
            try_body,
            catch_body,
            finally_body,
            ..
        } => {
            collect_block_calls(try_body, calls, symbols, known_types, known_call_types);
            if let Some(catch_body) = catch_body {
                collect_block_calls(catch_body, calls, symbols, known_types, known_call_types);
            }
            if let Some(finally_body) = finally_body {
                collect_block_calls(finally_body, calls, symbols, known_types, known_call_types);
            }
        }
        ControlFlow::Switch {
            expr,
            cases,
            default,
        } => {
            collect_expr_calls(expr, calls, symbols, known_types, known_call_types);
            for (case, body) in cases {
                collect_expr_calls(case, calls, symbols, known_types, known_call_types);
                collect_block_calls(body, calls, symbols, known_types, known_call_types);
            }
            if let Some(default) = default {
                collect_block_calls(default, calls, symbols, known_types, known_call_types);
            }
        }
    }
}

fn collect_expr_calls(
    expression: &Expr,
    calls: &mut HashMap<usize, Vec<Vec<Option<String>>>>,
    symbols: &BTreeMap<String, SymbolInfo>,
    known_types: &BTreeMap<String, String>,
    known_call_types: &BTreeMap<usize, String>,
) {
    match expression {
        Expr::Call { target, args } => {
            if let SemanticCallTarget::Internal { offset, .. } = target {
                calls.entry(*offset).or_default().push(
                    args.iter()
                        .map(|argument| {
                            concrete_definition_type_with_symbols_and_known_types_and_calls(
                                argument,
                                symbols,
                                known_types,
                                known_call_types,
                            )
                            .filter(|type_name| is_concrete_type(type_name))
                        })
                        .collect(),
                );
            }
            for argument in args {
                collect_expr_calls(argument, calls, symbols, known_types, known_call_types);
            }
        }
        Expr::Binary { left, right, .. } => {
            collect_expr_calls(left, calls, symbols, known_types, known_call_types);
            collect_expr_calls(right, calls, symbols, known_types, known_call_types);
        }
        Expr::Unary { operand, .. }
        | Expr::Cast { expr: operand, .. }
        | Expr::Convert { value: operand, .. }
        | Expr::IsType { value: operand, .. } => {
            collect_expr_calls(operand, calls, symbols, known_types, known_call_types);
        }
        Expr::Index { base, index } => {
            collect_expr_calls(base, calls, symbols, known_types, known_call_types);
            collect_expr_calls(index, calls, symbols, known_types, known_call_types);
        }
        Expr::Member { base, .. } => {
            collect_expr_calls(base, calls, symbols, known_types, known_call_types);
        }
        Expr::NewArray { length, .. } => {
            collect_expr_calls(length, calls, symbols, known_types, known_call_types);
        }
        Expr::Array(elements) | Expr::Struct(elements) => {
            for element in elements {
                collect_expr_calls(element, calls, symbols, known_types, known_call_types);
            }
        }
        Expr::Map(entries) => {
            for (key, value) in entries {
                collect_expr_calls(key, calls, symbols, known_types, known_call_types);
                collect_expr_calls(value, calls, symbols, known_types, known_call_types);
            }
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_expr_calls(condition, calls, symbols, known_types, known_call_types);
            collect_expr_calls(then_expr, calls, symbols, known_types, known_call_types);
            collect_expr_calls(else_expr, calls, symbols, known_types, known_call_types);
        }
        Expr::Unknown | Expr::Literal(_) | Expr::Variable(_) | Expr::StackTemp(_) => {}
    }
}

fn is_concrete_type(type_name: &str) -> bool {
    !matches!(type_name, "" | "dynamic" | "object" | "void")
}
