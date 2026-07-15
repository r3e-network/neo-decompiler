use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::decompiler::cfg::method_body::SymbolInfo;
use crate::decompiler::ir::{Block, ControlFlow, Expr, Intrinsic, SemanticCallTarget, Stmt};
use crate::instruction::OpCode;

use super::plan::{concrete_definition_type_with_symbols_and_known_types, ScopeId, ScopeTree};

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
pub(super) struct ActivityCollector<'a> {
    pub(super) scopes: ScopeTree,
    pub(super) activity: BTreeMap<String, SymbolActivity>,
    pub(super) concrete_definition_types: BTreeMap<String, String>,
    symbol_types: Option<&'a BTreeMap<String, SymbolInfo>>,
    definition_values: BTreeMap<String, Vec<Expr>>,
    non_concrete_definitions: HashSet<String>,
    direct_index_definitions: HashSet<String>,
    copy_definitions: Vec<(String, String)>,
    pub(super) implicit_declarations: HashSet<String>,
    pub(super) stack_placeholders: BTreeSet<usize>,
    next_order: usize,
}

#[cfg_attr(not(test), allow(dead_code))]
impl<'a> ActivityCollector<'a> {
    pub(super) fn new() -> Self {
        Self {
            scopes: ScopeTree::new(),
            activity: BTreeMap::new(),
            concrete_definition_types: BTreeMap::new(),
            symbol_types: None,
            definition_values: BTreeMap::new(),
            non_concrete_definitions: HashSet::new(),
            direct_index_definitions: HashSet::new(),
            copy_definitions: Vec::new(),
            implicit_declarations: HashSet::new(),
            stack_placeholders: BTreeSet::new(),
            next_order: 0,
        }
    }

    pub(super) fn with_symbol_types<'b>(
        self,
        symbol_types: &'b BTreeMap<String, SymbolInfo>,
    ) -> ActivityCollector<'b> {
        ActivityCollector {
            scopes: self.scopes,
            activity: self.activity,
            concrete_definition_types: self.concrete_definition_types,
            symbol_types: Some(symbol_types),
            definition_values: self.definition_values,
            non_concrete_definitions: self.non_concrete_definitions,
            direct_index_definitions: self.direct_index_definitions,
            copy_definitions: self.copy_definitions,
            implicit_declarations: self.implicit_declarations,
            stack_placeholders: self.stack_placeholders,
            next_order: self.next_order,
        }
    }

    fn record_definition(&mut self, name: &str, value: &Expr, scope: ScopeId) {
        let occurrence = self.occurrence(scope);
        self.activity
            .entry(name.to_string())
            .or_default()
            .definitions
            .push(occurrence);
        self.definition_values
            .entry(name.to_string())
            .or_default()
            .push(value.clone());

        if matches!(
            value,
            Expr::Index { .. }
                | Expr::Call {
                    target: SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Pickitem)),
                    ..
                }
        ) {
            self.direct_index_definitions.insert(name.to_string());
        }
        if let Expr::Variable(source) = value {
            self.copy_definitions
                .push((name.to_string(), source.clone()));
        }
    }

    pub(super) fn resolve_concrete_definition_types_with_known_types(
        &mut self,
        initial_known_types: &BTreeMap<String, String>,
    ) {
        let empty_symbols = BTreeMap::new();
        let mut known_types = initial_known_types.clone();
        let mut derived_types = BTreeMap::new();
        let iterations = self.definition_values.len().saturating_add(1);

        for _ in 0..iterations {
            let mut next_types = BTreeMap::new();
            let mut non_concrete = HashSet::new();
            for (name, definitions) in &self.definition_values {
                let mut candidate = None;
                let mut consistent = true;
                for definition in definitions {
                    let definition_type = concrete_definition_type_with_symbols_and_known_types(
                        definition,
                        self.symbol_types.unwrap_or(&empty_symbols),
                        &known_types,
                    );
                    let Some(definition_type) = definition_type else {
                        consistent = false;
                        break;
                    };
                    if candidate
                        .as_ref()
                        .is_some_and(|existing| existing != &definition_type)
                    {
                        consistent = false;
                        break;
                    }
                    candidate = Some(definition_type);
                }
                if consistent {
                    if let Some(candidate) = candidate {
                        next_types.insert(name.clone(), candidate);
                    }
                } else {
                    non_concrete.insert(name.clone());
                }
            }

            if next_types == derived_types {
                self.concrete_definition_types = next_types;
                self.non_concrete_definitions = non_concrete;
                return;
            }
            derived_types = next_types;
            known_types = initial_known_types.clone();
            known_types.extend(derived_types.clone());
        }

        self.concrete_definition_types = derived_types;
        self.non_concrete_definitions = self
            .definition_values
            .keys()
            .filter(|name| !self.concrete_definition_types.contains_key(*name))
            .cloned()
            .collect();
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
