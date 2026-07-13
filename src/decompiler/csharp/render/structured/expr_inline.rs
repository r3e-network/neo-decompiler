use std::collections::BTreeMap;

use crate::decompiler::analysis::types::ValueType;
use crate::decompiler::cfg::method_body::{SymbolInfo, SymbolOrigin};
use crate::decompiler::ir::{BinOp, Block, ControlFlow, Expr, Literal, Stmt};

use super::expr::low_level_binary_opcode;

#[derive(Debug)]
pub(super) struct Definition {
    pub(super) value: Expr,
    pub(super) scope: u32,
    pub(super) order: usize,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct UseOccurrence {
    pub(super) scope: u32,
    pub(super) order: usize,
}

#[derive(Default)]
pub(super) struct InlineCollector {
    pub(super) definitions: BTreeMap<String, Vec<Definition>>,
    pub(super) uses: BTreeMap<String, Vec<UseOccurrence>>,
    next_scope: u32,
    next_order: usize,
}

impl InlineCollector {
    fn child_scope(&mut self) -> u32 {
        self.next_scope = self.next_scope.saturating_add(1);
        self.next_scope
    }

    pub(super) fn visit_block(&mut self, block: &Block, scope: u32) {
        for statement in &block.stmts {
            self.visit_statement(statement, scope);
        }
    }

    fn visit_statement(&mut self, statement: &Stmt, scope: u32) {
        match statement {
            Stmt::Assign { target, value } => {
                self.visit_expr(value, scope);
                let order = self.take_order();
                self.definitions
                    .entry(target.clone())
                    .or_default()
                    .push(Definition {
                        value: value.clone(),
                        scope,
                        order,
                    });
            }
            Stmt::Return(value) => {
                if let Some(value) = value {
                    self.visit_expr(value, scope);
                }
            }
            Stmt::Throw(value) | Stmt::Abort(value) => {
                if let Some(value) = value {
                    self.visit_expr(value, scope);
                }
            }
            Stmt::Assert { condition, message } => {
                self.visit_expr(condition, scope);
                if let Some(message) = message {
                    self.visit_expr(message, scope);
                }
            }
            Stmt::ExprStmt(value) => self.visit_expr(value, scope),
            Stmt::Comment(_) | Stmt::Break | Stmt::Continue | Stmt::Label(_) | Stmt::Goto(_) => {}
            Stmt::ControlFlow(control) => self.visit_control(control, scope),
        }
    }

    fn visit_control(&mut self, control: &ControlFlow, scope: u32) {
        match control {
            ControlFlow::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.visit_expr(condition, scope);
                let then_scope = self.child_scope();
                self.visit_block(then_branch, then_scope);
                if let Some(branch) = else_branch {
                    let else_scope = self.child_scope();
                    self.visit_block(branch, else_scope);
                }
            }
            ControlFlow::While { condition, body } => {
                let condition_scope = self.child_scope();
                self.visit_expr(condition, condition_scope);
                let body_scope = self.child_scope();
                self.visit_block(body, body_scope);
            }
            ControlFlow::DoWhile { body, condition } => {
                let body_scope = self.child_scope();
                self.visit_block(body, body_scope);
                let condition_scope = self.child_scope();
                self.visit_expr(condition, condition_scope);
            }
            ControlFlow::For {
                init,
                condition,
                update,
                body,
            } => {
                let loop_scope = self.child_scope();
                if let Some(init) = init {
                    self.visit_statement(init, loop_scope);
                }
                if let Some(condition) = condition {
                    let condition_scope = self.child_scope();
                    self.visit_expr(condition, condition_scope);
                }
                let body_scope = self.child_scope();
                self.visit_block(body, body_scope);
                if let Some(update) = update {
                    let update_scope = self.child_scope();
                    self.visit_expr(update, update_scope);
                }
            }
            ControlFlow::TryCatch {
                try_body,
                catch_body,
                finally_body,
                ..
            } => {
                let try_scope = self.child_scope();
                self.visit_block(try_body, try_scope);
                if let Some(body) = catch_body {
                    let catch_scope = self.child_scope();
                    self.visit_block(body, catch_scope);
                }
                if let Some(body) = finally_body {
                    let finally_scope = self.child_scope();
                    self.visit_block(body, finally_scope);
                }
            }
            ControlFlow::Switch {
                expr,
                cases,
                default,
            } => {
                self.visit_expr(expr, scope);
                for (value, body) in cases {
                    self.visit_expr(value, scope);
                    let case_scope = self.child_scope();
                    self.visit_block(body, case_scope);
                }
                if let Some(body) = default {
                    let default_scope = self.child_scope();
                    self.visit_block(body, default_scope);
                }
            }
        }
    }

    fn visit_expr(&mut self, expression: &Expr, scope: u32) {
        match expression {
            Expr::Variable(name) => {
                let order = self.take_order();
                self.uses
                    .entry(name.clone())
                    .or_default()
                    .push(UseOccurrence { scope, order });
            }
            Expr::Binary { left, right, .. } => {
                self.visit_expr(left, scope);
                self.visit_expr(right, scope);
            }
            Expr::Unary { operand, .. } => self.visit_expr(operand, scope),
            Expr::Call { args, .. } | Expr::Array(args) => {
                for argument in args {
                    self.visit_expr(argument, scope);
                }
            }
            Expr::Index { base, index } => {
                self.visit_expr(base, scope);
                self.visit_expr(index, scope);
            }
            Expr::Member { base, .. } | Expr::Cast { expr: base, .. } => {
                self.visit_expr(base, scope);
            }
            Expr::Convert { value, .. } | Expr::IsType { value, .. } => {
                self.visit_expr(value, scope);
            }
            Expr::NewArray { length, .. } => self.visit_expr(length, scope),
            Expr::Map(pairs) => {
                for (key, value) in pairs {
                    self.visit_expr(key, scope);
                    self.visit_expr(value, scope);
                }
            }
            Expr::Struct(values) => {
                for value in values {
                    self.visit_expr(value, scope);
                }
            }
            Expr::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                self.visit_expr(condition, scope);
                self.visit_expr(then_expr, scope);
                self.visit_expr(else_expr, scope);
            }
            Expr::Unknown | Expr::Literal(_) | Expr::StackTemp(_) => {}
        }
    }

    fn take_order(&mut self) -> usize {
        let order = self.next_order;
        self.next_order = self.next_order.saturating_add(1);
        order
    }
}

pub(super) fn is_inline_pure(
    expression: &Expr,
    definitions: &BTreeMap<String, Vec<Definition>>,
    definition_order: usize,
    usage_order: usize,
    symbols: &BTreeMap<String, SymbolInfo>,
) -> bool {
    match expression {
        Expr::Literal(Literal::Bytes(_)) => false,
        Expr::Literal(_) => true,
        Expr::Variable(name) => {
            !symbols
                .get(name)
                .is_some_and(|symbol| matches!(symbol.origin, SymbolOrigin::Static(_)))
                && definitions.get(name).is_none_or(|assignments| {
                    assignments.iter().all(|assignment| {
                        assignment.order <= definition_order || assignment.order >= usage_order
                    })
                })
        }
        Expr::Binary { op, left, right } => {
            low_level_binary_opcode(
                *op,
                expression_value_type(left, symbols),
                expression_value_type(right, symbols),
            )
            .is_none()
                && !matches!(
                    op,
                    BinOp::Div | BinOp::Mod | BinOp::Pow | BinOp::Shl | BinOp::Shr
                )
                && is_inline_pure(left, definitions, definition_order, usage_order, symbols)
                && is_inline_pure(right, definitions, definition_order, usage_order, symbols)
        }
        Expr::Unary { operand, .. } => {
            is_inline_pure(operand, definitions, definition_order, usage_order, symbols)
        }
        Expr::Index { .. } | Expr::Member { .. } => false,
        Expr::Cast { .. } | Expr::Convert { .. } | Expr::IsType { .. } | Expr::NewArray { .. } => {
            false
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            is_inline_pure(
                condition,
                definitions,
                definition_order,
                usage_order,
                symbols,
            ) && is_inline_pure(
                then_expr,
                definitions,
                definition_order,
                usage_order,
                symbols,
            ) && is_inline_pure(
                else_expr,
                definitions,
                definition_order,
                usage_order,
                symbols,
            )
        }
        Expr::Call { .. }
        | Expr::Array(_)
        | Expr::Struct(_)
        | Expr::Map(_)
        | Expr::StackTemp(_)
        | Expr::Unknown => false,
    }
}

fn expression_value_type(expression: &Expr, symbols: &BTreeMap<String, SymbolInfo>) -> ValueType {
    match expression {
        Expr::Unknown => ValueType::Unknown,
        Expr::Variable(name) => symbols
            .get(name)
            .map_or(ValueType::Unknown, |symbol| symbol.value_type),
        Expr::Literal(Literal::Int(_) | Literal::BigInt(_)) => ValueType::Integer,
        Expr::Literal(Literal::Bool(_)) => ValueType::Boolean,
        Expr::Literal(Literal::String(_)) => ValueType::ByteString,
        Expr::Literal(Literal::Bytes(_)) => ValueType::ByteString,
        Expr::Literal(Literal::Null) => ValueType::Null,
        Expr::Convert { target, .. } => *target,
        Expr::IsType { .. } => ValueType::Boolean,
        Expr::NewArray { .. } | Expr::Array(_) => ValueType::Array,
        Expr::Struct(_) => ValueType::Struct,
        Expr::Map(_) => ValueType::Map,
        _ => ValueType::Unknown,
    }
}
