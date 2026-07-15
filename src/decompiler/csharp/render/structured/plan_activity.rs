use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::decompiler::cfg::method_body::SymbolInfo;
use crate::decompiler::ir::{Expr, Intrinsic, Literal, SemanticCallTarget};
use crate::instruction::OpCode;

use super::plan::{
    concrete_definition_type_with_symbols_and_known_types_and_calls, ScopeId, ScopeTree,
};

#[path = "plan_activity/visitor.rs"]
mod visitor;

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
    indexed_base_symbols: HashSet<String>,
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
            indexed_base_symbols: HashSet::new(),
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
            indexed_base_symbols: self.indexed_base_symbols,
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

    pub(super) fn resolve_concrete_definition_types_with_known_types_and_calls(
        &mut self,
        initial_known_types: &BTreeMap<String, String>,
        known_call_types: &BTreeMap<usize, String>,
    ) {
        let empty_symbols = BTreeMap::new();
        let mut known_types = initial_known_types.clone();
        let mut derived_types = BTreeMap::new();
        let mut known_nulls = BTreeSet::new();
        let mut derived_nulls = BTreeSet::new();
        let iterations = self.definition_values.len().saturating_add(1);

        for _ in 0..iterations {
            let mut next_types = BTreeMap::new();
            let mut next_nulls = BTreeSet::new();
            let mut non_concrete = HashSet::new();
            for (name, definitions) in &self.definition_values {
                let mut candidate = None;
                let mut consistent = true;
                let mut saw_null = false;
                for definition in definitions {
                    if is_null_definition(definition, &known_nulls) {
                        saw_null = true;
                        continue;
                    }
                    let definition_type =
                        concrete_definition_type_with_symbols_and_known_types_and_calls(
                            definition,
                            self.symbol_types.unwrap_or(&empty_symbols),
                            &known_types,
                            known_call_types,
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
                        if !saw_null || is_nullable_csharp_type(&candidate) {
                            next_types.insert(name.clone(), candidate);
                        } else {
                            non_concrete.insert(name.clone());
                        }
                    } else if saw_null {
                        next_nulls.insert(name.clone());
                    }
                } else {
                    non_concrete.insert(name.clone());
                }
            }

            if next_types == derived_types && next_nulls == derived_nulls {
                self.concrete_definition_types = next_types;
                self.non_concrete_definitions = non_concrete;
                return;
            }
            derived_types = next_types;
            derived_nulls = next_nulls.clone();
            known_types = initial_known_types.clone();
            known_types.extend(derived_types.clone());
            known_nulls = next_nulls;
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

    pub(super) fn indexed_base_symbols(&self) -> HashSet<String> {
        let mut symbols = self.indexed_base_symbols.clone();
        self.propagate_indexed_base_symbols(&mut symbols);
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

    fn propagate_indexed_base_symbols(&self, symbols: &mut HashSet<String>) {
        loop {
            let mut changed = false;
            for (target, source) in &self.copy_definitions {
                if symbols.contains(source) {
                    changed |= symbols.insert(target.clone());
                }
                if symbols.contains(target) {
                    changed |= symbols.insert(source.clone());
                }
            }
            if !changed {
                return;
            }
        }
    }
}

fn is_nullable_csharp_type(type_name: &str) -> bool {
    type_name.ends_with("[]")
        || matches!(
            type_name,
            "ByteString" | "Map<object, object>" | "string" | "object"
        )
}

fn is_null_definition(expression: &Expr, known_nulls: &BTreeSet<String>) -> bool {
    matches!(expression, Expr::Literal(Literal::Null))
        || matches!(expression, Expr::Variable(name) if known_nulls.contains(name))
}
