use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::decompiler::ir::{Block, ControlFlow, Expr, Stmt};

use super::plan::{concrete_definition_type, ScopeId, ScopeTree};

#[derive(Debug, Clone, Copy)]
#[cfg_attr(not(test), allow(dead_code))]
pub(super) struct Occurrence {
    pub(super) scope: ScopeId,
    pub(super) order: usize,
}

#[derive(Debug, Default)]
#[cfg_attr(not(test), allow(dead_code))]
pub(super) struct SymbolActivity {
    pub(super) definitions: Vec<Occurrence>,
    pub(super) uses: Vec<Occurrence>,
}

#[cfg_attr(not(test), allow(dead_code))]
pub(super) struct ActivityCollector {
    pub(super) scopes: ScopeTree,
    pub(super) activity: BTreeMap<String, SymbolActivity>,
    pub(super) concrete_definition_types: BTreeMap<String, String>,
    non_concrete_definitions: HashSet<String>,
    direct_index_definitions: HashSet<String>,
    copy_definitions: Vec<(String, String)>,
    pub(super) implicit_declarations: HashSet<String>,
    pub(super) stack_placeholders: BTreeSet<usize>,
    next_order: usize,
}

#[cfg_attr(not(test), allow(dead_code))]
impl ActivityCollector {
    pub(super) fn new() -> Self {
        Self {
            scopes: ScopeTree::new(),
            activity: BTreeMap::new(),
            concrete_definition_types: BTreeMap::new(),
            non_concrete_definitions: HashSet::new(),
            direct_index_definitions: HashSet::new(),
            copy_definitions: Vec::new(),
            implicit_declarations: HashSet::new(),
            stack_placeholders: BTreeSet::new(),
            next_order: 0,
        }
    }

    fn record_definition(&mut self, name: &str, value: &Expr, scope: ScopeId) {
        let occurrence = self.occurrence(scope);
        self.activity
            .entry(name.to_string())
            .or_default()
            .definitions
            .push(occurrence);

        if matches!(value, Expr::Index { .. }) {
            self.direct_index_definitions.insert(name.to_string());
        }
        if let Expr::Variable(source) = value {
            self.copy_definitions
                .push((name.to_string(), source.clone()));
        }

        let Some(candidate) = concrete_definition_type(value) else {
            self.concrete_definition_types.remove(name);
            self.non_concrete_definitions.insert(name.to_string());
            return;
        };
        if self.non_concrete_definitions.contains(name) {
            return;
        }
        if self
            .concrete_definition_types
            .get(name)
            .is_some_and(|existing| existing != &candidate)
        {
            self.concrete_definition_types.remove(name);
            self.non_concrete_definitions.insert(name.to_string());
            return;
        }
        self.concrete_definition_types
            .insert(name.to_string(), candidate);
    }

    fn record_use(&mut self, name: &str, scope: ScopeId) {
        let occurrence = self.occurrence(scope);
        self.activity
            .entry(name.to_string())
            .or_default()
            .uses
            .push(occurrence);
    }

    fn occurrence(&mut self, scope: ScopeId) -> Occurrence {
        let occurrence = Occurrence {
            scope,
            order: self.next_order,
        };
        self.next_order += 1;
        occurrence
    }

    pub(super) fn index_defined_symbols(&self) -> HashSet<String> {
        let mut symbols = self.direct_index_definitions.clone();
        self.propagate_copy_definitions(&mut symbols);
        symbols
    }

    fn propagate_copy_definitions(&self, symbols: &mut HashSet<String>) {
        loop {
            let mut changed = false;
            for (target, source) in &self.copy_definitions {
                if symbols.contains(source) {
                    changed |= symbols.insert(target.clone());
                }
            }
            if !changed {
                return;
            }
        }
    }

    pub(super) fn visit_block(&mut self, block: &Block, scope: ScopeId) {
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
            Expr::Call { args, .. } | Expr::Array(args) => {
                for argument in args {
                    self.visit_expr(argument, scope);
                }
            }
            Expr::Index { base, index } => {
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
}
