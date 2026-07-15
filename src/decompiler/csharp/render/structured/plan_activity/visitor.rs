use crate::decompiler::ir::{Block, ControlFlow, Expr, Intrinsic, SemanticCallTarget, Stmt};
use crate::instruction::OpCode;

use super::super::plan::ScopeId;
use super::super::plan_activity::ActivityCollector;

impl ActivityCollector<'_> {
    pub(in crate::decompiler::csharp::render::structured) fn visit_block(
        &mut self,
        block: &Block,
        scope: ScopeId,
    ) {
        for statement in &block.stmts {
            self.visit_statement(statement, scope);
        }
    }

    fn visit_statement(&mut self, statement: &Stmt, scope: ScopeId) {
        match statement {
            Stmt::Assign { target, value } => {
                self.visit_expr(value, scope);
                self.record_definition(target, value, scope);
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

    fn visit_control(&mut self, control: &ControlFlow, scope: ScopeId) {
        match control {
            ControlFlow::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.visit_expr(condition, scope);
                self.visit_child_block(then_branch, scope);
                if let Some(branch) = else_branch {
                    self.visit_child_block(branch, scope);
                }
            }
            ControlFlow::While { condition, body } => {
                self.visit_expr(condition, scope);
                self.visit_child_block(body, scope);
            }
            ControlFlow::DoWhile { body, condition } => {
                self.visit_child_block(body, scope);
                self.visit_expr(condition, scope);
            }
            ControlFlow::For {
                init,
                condition,
                update,
                body,
            } => {
                let loop_scope = self.scopes.add_child(scope);
                if let Some(init) = init {
                    self.visit_statement(init, loop_scope);
                }
                if let Some(condition) = condition {
                    self.visit_expr(condition, loop_scope);
                }
                self.visit_child_block(body, loop_scope);
                if let Some(update) = update {
                    self.visit_expr(update, loop_scope);
                }
            }
            ControlFlow::TryCatch {
                try_body,
                catch_var,
                catch_body,
                finally_body,
            } => {
                self.visit_child_block(try_body, scope);
                if let Some(body) = catch_body {
                    let catch_scope = self.scopes.add_child(scope);
                    if let Some(catch_var) = catch_var {
                        self.implicit_declarations.insert(catch_var.clone());
                    }
                    self.visit_block(body, catch_scope);
                }
                if let Some(body) = finally_body {
                    self.visit_child_block(body, scope);
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
                    self.visit_child_block(body, scope);
                }
                if let Some(body) = default {
                    self.visit_child_block(body, scope);
                }
            }
        }
    }

    fn visit_child_block(&mut self, block: &Block, parent: ScopeId) {
        let scope = self.scopes.add_child(parent);
        self.visit_block(block, scope);
    }

    fn visit_expr(&mut self, expression: &Expr, scope: ScopeId) {
        match expression {
            Expr::Unknown => {}
            Expr::Variable(name) => self.record_use(name, scope),
            Expr::Binary { left, right, .. } => {
                self.visit_expr(left, scope);
                self.visit_expr(right, scope);
            }
            Expr::Unary { operand, .. } => self.visit_expr(operand, scope),
            Expr::Call { target, args } => {
                if matches!(
                    target,
                    SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Pickitem))
                ) {
                    if let Some(base) = args.first() {
                        self.record_index_base_symbols(base);
                    }
                }
                for argument in args {
                    self.visit_expr(argument, scope);
                }
            }
            Expr::Array(args) => {
                for argument in args {
                    self.visit_expr(argument, scope);
                }
            }
            Expr::Index { base, index } => {
                self.record_index_base_symbols(base);
                self.visit_expr(base, scope);
                self.visit_expr(index, scope);
            }
            Expr::Member { base, .. } => self.visit_expr(base, scope),
            Expr::Cast { expr, .. } => self.visit_expr(expr, scope),
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
            Expr::StackTemp(index) => {
                self.stack_placeholders.insert(*index);
            }
            Expr::Literal(_) => {}
        }
    }

    fn record_index_base_symbols(&mut self, expression: &Expr) {
        match expression {
            Expr::Variable(name) => {
                self.indexed_base_symbols.insert(name.clone());
            }
            Expr::Cast { expr, .. }
            | Expr::Convert { value: expr, .. }
            | Expr::IsType { value: expr, .. } => self.record_index_base_symbols(expr),
            Expr::Index { base, .. } => self.record_index_base_symbols(base),
            Expr::Ternary {
                then_expr,
                else_expr,
                ..
            } => {
                self.record_index_base_symbols(then_expr);
                self.record_index_base_symbols(else_expr);
            }
            Expr::Unknown
            | Expr::Literal(_)
            | Expr::Binary { .. }
            | Expr::Unary { .. }
            | Expr::Call { .. }
            | Expr::Member { .. }
            | Expr::NewArray { .. }
            | Expr::Array(_)
            | Expr::Struct(_)
            | Expr::Map(_)
            | Expr::StackTemp(_) => {}
        }
    }
}
