use super::super::super::super::helpers::sanitize_csharp_identifier;
use super::super::plan_activity::{is_nullable_csharp_type, ActivityCollector};
use crate::decompiler::analysis::types::{TypeInfo, ValueType};
use crate::decompiler::cfg::method_body::{
    Fidelity, LoweringIssue, LoweringIssueKind, SymbolInfo, SymbolOrigin,
};
use crate::decompiler::ir::Block;
use crate::instruction::OpCode;
use std::collections::{BTreeMap, BTreeSet, HashSet};

use super::declaration_type_catalog::concrete_type_matches_value_type;
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(not(test), allow(dead_code))]
pub(in crate::decompiler::csharp::render) struct ScopeId(u32);

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
struct ScopeNode {
    parent: Option<ScopeId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(in crate::decompiler::csharp::render) struct ScopeTree {
    scopes: Vec<ScopeNode>,
}

#[cfg_attr(not(test), allow(dead_code))]
impl ScopeTree {
    pub(in crate::decompiler::csharp::render) fn new() -> Self {
        Self {
            scopes: vec![ScopeNode { parent: None }],
        }
    }

    pub(in crate::decompiler::csharp::render) fn root(&self) -> ScopeId {
        ScopeId(0)
    }

    pub(in crate::decompiler::csharp::render) fn scope_at(&self, index: usize) -> Option<ScopeId> {
        (index < self.scopes.len()).then_some(ScopeId(
            u32::try_from(index).expect("structured scope count must fit in u32"),
        ))
    }

    pub(in crate::decompiler::csharp::render) fn parent_of(
        &self,
        scope: ScopeId,
    ) -> Option<ScopeId> {
        self.parent(scope)
    }

    /// True when `ancestor` is `scope` itself or one of its enclosing scopes.
    ///
    /// A declaration emitted in `ancestor` is visible from `scope` exactly when
    /// this holds, which is what gates moving a hoisted declaration down to its
    /// first assignment site.
    pub(in crate::decompiler::csharp::render) fn encloses(
        &self,
        ancestor: ScopeId,
        scope: ScopeId,
    ) -> bool {
        let mut current = Some(scope);
        while let Some(candidate) = current {
            if candidate == ancestor {
                return true;
            }
            current = self.parent(candidate);
        }
        false
    }

    pub(in crate::decompiler::csharp::render) fn add_child(&mut self, parent: ScopeId) -> ScopeId {
        let id = ScopeId(
            u32::try_from(self.scopes.len()).expect("structured scope count must fit in u32"),
        );
        self.scopes.push(ScopeNode {
            parent: Some(parent),
        });
        id
    }

    pub(in crate::decompiler::csharp::render) fn nearest_common_ancestor(
        &self,
        scopes: impl IntoIterator<Item = ScopeId>,
    ) -> ScopeId {
        scopes
            .into_iter()
            .reduce(|left, right| self.common_ancestor(left, right))
            .unwrap_or_else(|| self.root())
    }

    fn common_ancestor(&self, left: ScopeId, mut right: ScopeId) -> ScopeId {
        let mut left_ancestors = HashSet::new();
        let mut current = Some(left);
        while let Some(scope) = current {
            left_ancestors.insert(scope);
            current = self.parent(scope);
        }
        while !left_ancestors.contains(&right) {
            right = self
                .parent(right)
                .expect("all structured scopes descend from the root");
        }
        right
    }

    fn parent(&self, scope: ScopeId) -> Option<ScopeId> {
        self.scopes
            .get(scope.0 as usize)
            .and_then(|node| node.parent)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(in crate::decompiler::csharp::render) enum DeclarationKind {
    Inline,
    HoistedAssignment,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(in crate::decompiler::csharp::render) struct PlannedDeclaration {
    pub(in crate::decompiler::csharp::render) scope: ScopeId,
    pub(in crate::decompiler::csharp::render) kind: DeclarationKind,
    pub(in crate::decompiler::csharp::render) emitted_name: String,
    pub(in crate::decompiler::csharp::render) csharp_type: String,
    pub(in crate::decompiler::csharp::render) initialize_to_default: bool,
    /// Emit `T name = value` at the first in-scope assignment instead of a
    /// bare hoisted `T name;` followed by `name = value`.
    pub(in crate::decompiler::csharp::render) merge_first_assignment: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(in crate::decompiler::csharp::render) struct DeclarationPlan {
    pub(in crate::decompiler::csharp::render) scopes: ScopeTree,
    pub(in crate::decompiler::csharp::render) declarations: BTreeMap<String, PlannedDeclaration>,
    /// Concrete method parameter types used while resolving member/index
    /// expressions. Parameters are emitted by the method signature, so they
    /// must not become local declarations here.
    pub(in crate::decompiler::csharp::render) parameter_types: BTreeMap<String, String>,
    pub(in crate::decompiler::csharp::render) static_field_types: BTreeMap<String, String>,
    pub(in crate::decompiler::csharp::render) issues: Vec<LoweringIssue>,
    pub(in crate::decompiler::csharp::render) typed: bool,
    pub(in crate::decompiler::csharp::render) index_defined_symbols: HashSet<String>,
    pub(in crate::decompiler::csharp::render) unused_copy_symbols: HashSet<String>,
}

impl DeclarationPlan {
    pub(in crate::decompiler::csharp::render) fn with_static_field_types(
        mut self,
        static_field_types: &BTreeMap<String, String>,
    ) -> Self {
        self.static_field_types.clone_from(static_field_types);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::decompiler::csharp::render) struct CSharpStaticField {
    pub(in crate::decompiler::csharp::render) name: String,
    pub(in crate::decompiler::csharp::render) csharp_type: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::decompiler::csharp::render) struct CSharpContractSymbols {
    pub(in crate::decompiler::csharp::render) static_fields: Vec<CSharpStaticField>,
}
#[cfg_attr(not(test), allow(dead_code))]
pub(in crate::decompiler::csharp::render) fn plan_declarations(
    body: &Block,
    symbols: &BTreeMap<String, SymbolInfo>,
    typed: bool,
) -> DeclarationPlan {
    plan_declarations_with_known_types(body, symbols, typed, &BTreeMap::new())
}

#[cfg_attr(not(test), allow(dead_code))]
pub(in crate::decompiler::csharp::render) fn plan_declarations_with_known_types(
    body: &Block,
    symbols: &BTreeMap<String, SymbolInfo>,
    typed: bool,
    known_types: &BTreeMap<String, String>,
) -> DeclarationPlan {
    plan_declarations_with_known_types_and_calls(
        body,
        symbols,
        typed,
        known_types,
        &BTreeMap::new(),
    )
}

#[cfg_attr(not(test), allow(dead_code))]
pub(in crate::decompiler::csharp::render) fn plan_declarations_with_known_types_and_calls(
    body: &Block,
    symbols: &BTreeMap<String, SymbolInfo>,
    typed: bool,
    known_types: &BTreeMap<String, String>,
    known_call_types: &BTreeMap<usize, String>,
) -> DeclarationPlan {
    let mut collector = ActivityCollector::new().with_symbol_types(symbols);
    let root = collector.scopes.root();
    collector.visit_block(body, root);
    collector.resolve_concrete_definition_types_with_known_types_and_calls(
        known_types,
        known_call_types,
    );
    let index_defined_symbols = collector.index_defined_symbols();
    let unused_copy_symbols = collector.unused_copy_symbols(symbols);

    let mut declarations = BTreeMap::new();
    let mut issues = collector
        .stack_placeholders
        .iter()
        .map(|index| {
            declaration_issue(
                LoweringIssueKind::LostStackValue,
                format!("structured stack placeholder {index} has no recovered value"),
            )
        })
        .collect::<Vec<_>>();
    for (name, activity) in &collector.activity {
        if collector.implicit_declarations.contains(name) {
            continue;
        }
        let symbol = symbols.get(name);
        if symbol.is_some_and(|symbol| {
            matches!(
                symbol.origin,
                SymbolOrigin::Parameter(_) | SymbolOrigin::Static(_)
            )
        }) {
            continue;
        }
        if activity.definitions.is_empty() {
            issues.push(declaration_issue(
                LoweringIssueKind::LostStackValue,
                format!("structured symbol {name} is used without an assignment"),
            ));
            // A phi can survive into the structured IR when its defining edge
            // was intentionally omitted (for example, an exceptional or
            // malformed VM path). Keep the source compile-safe with a
            // conservative default instead of emitting an undeclared name.
            if let Some(symbol) = symbol {
                if symbol.origin == SymbolOrigin::Phi {
                    let scope = collector.scopes.nearest_common_ancestor(
                        activity.uses.iter().map(|occurrence| occurrence.scope),
                    );
                    declarations.insert(
                        name.clone(),
                        PlannedDeclaration {
                            scope,
                            kind: DeclarationKind::HoistedAssignment,
                            emitted_name: sanitize_csharp_identifier(name),
                            csharp_type: csharp_type(symbol.value_type, typed).to_string(),
                            initialize_to_default: true,
                            merge_first_assignment: false,
                        },
                    );
                }
            }
            continue;
        }
        let Some(symbol) = symbol else {
            issues.push(declaration_issue(
                LoweringIssueKind::MissingProvenance,
                format!("structured symbol {name} has no neutral metadata"),
            ));
            continue;
        };

        let definition = activity.definitions.first().copied();
        let inline = definition.is_some_and(|definition| {
            activity.definitions.len() == 1
                && activity
                    .uses
                    .iter()
                    .all(|usage| usage.scope == definition.scope && usage.order > definition.order)
        });
        let scope = if inline {
            definition
                .expect("an inline declaration has one definition")
                .scope
        } else {
            collector.scopes.nearest_common_ancestor(
                activity
                    .definitions
                    .iter()
                    .chain(&activity.uses)
                    .map(|occurrence| occurrence.scope),
            )
        };
        let concrete_type = collector
            .concrete_definition_types
            .get(name)
            .filter(|candidate| {
                typed
                    && (concrete_type_matches_value_type(candidate, symbol.value_type)
                        || (symbol.value_type == ValueType::Null
                            && collector.nullable_concrete_definitions.contains(name)
                            && is_nullable_csharp_type(candidate)
                            && *candidate == "object[]"))
            })
            .cloned();
        let initialize_to_default = !inline && symbol.origin == SymbolOrigin::Phi;
        // A hoisted local may merge its first assignment into the declaration
        // (`T name = value`) only when the merge cannot change C# visibility
        // or duplicate the name:
        //
        // * every definition and use must live in the very same scope —
        //   otherwise the declaration would move into an inner block and
        //   leave outer/sibling references unresolved (CS0103), as happened
        //   when a `for` initializer adopted `loc3` that the loop body's
        //   successor still read;
        // * the first definition must precede every use in that scope;
        // * the declaration must not already be emitted as `T name = default`
        //   (phi locals), which would declare the name twice (CS0128).
        let merge_first_assignment = if inline || initialize_to_default {
            false
        } else {
            activity
                .definitions
                .iter()
                .min_by_key(|definition| definition.order)
                .is_some_and(|first| {
                    // The declaration lands in `first.scope`, so every other
                    // occurrence must sit inside that block (or a nested one)
                    // to stay visible, and every read must follow it. A `for`
                    // initializer that owns the first store would otherwise
                    // hide the name from statements after the loop (CS0103).
                    let visible_from = |scope| collector.scopes.encloses(first.scope, scope);
                    activity.definitions.iter().all(|definition| {
                        definition.order == first.order || visible_from(definition.scope)
                    }) && activity
                        .uses
                        .iter()
                        .all(|usage| usage.order > first.order && visible_from(usage.scope))
                })
        };
        declarations.insert(
            name.clone(),
            PlannedDeclaration {
                scope,
                kind: if inline {
                    DeclarationKind::Inline
                } else {
                    DeclarationKind::HoistedAssignment
                },
                emitted_name: sanitize_csharp_identifier(name),
                csharp_type: if typed
                    && index_defined_symbols.contains(name)
                    && concrete_type.is_none()
                {
                    "dynamic".to_string()
                } else {
                    concrete_type
                        .unwrap_or_else(|| csharp_type(symbol.value_type, typed).to_string())
                },
                initialize_to_default,
                merge_first_assignment,
            },
        );
    }
    issues.sort_by(|left, right| {
        (left.kind, left.detail.as_str()).cmp(&(right.kind, right.detail.as_str()))
    });

    DeclarationPlan {
        scopes: collector.scopes,
        declarations,
        parameter_types: known_types.clone(),
        static_field_types: BTreeMap::new(),
        issues,
        typed,
        index_defined_symbols,
        unused_copy_symbols,
    }
}

pub(in crate::decompiler::csharp::render) fn collect_index_defined_symbols(
    body: &Block,
) -> HashSet<String> {
    let mut collector = ActivityCollector::new();
    let root = collector.scopes.root();
    collector.visit_block(body, root);
    collector.index_defined_symbols()
}

pub(in crate::decompiler::csharp::render) fn collect_indexed_base_symbols(
    body: &Block,
) -> HashSet<String> {
    let mut collector = ActivityCollector::new();
    let root = collector.scopes.root();
    collector.visit_block(body, root);
    collector.indexed_base_symbols()
}

pub(in crate::decompiler::csharp::render) fn plan_contract_symbols(
    types: &TypeInfo,
    method_symbols: &[&BTreeMap<String, SymbolInfo>],
    typed: bool,
    index_defined_statics: &BTreeSet<usize>,
) -> CSharpContractSymbols {
    let mut statics: BTreeMap<usize, ValueType> =
        types.statics.iter().copied().enumerate().collect();
    for symbols in method_symbols {
        for symbol in symbols.values() {
            let SymbolOrigin::Static(index) = symbol.origin else {
                continue;
            };
            statics
                .entry(index)
                .and_modify(|current| *current = merge_value_types(*current, symbol.value_type))
                .or_insert(symbol.value_type);
        }
    }
    for index in index_defined_statics {
        statics.entry(*index).or_insert(ValueType::Unknown);
    }

    CSharpContractSymbols {
        static_fields: statics
            .into_iter()
            .map(|(index, value_type)| CSharpStaticField {
                name: format!("static{index}"),
                csharp_type: if typed && index_defined_statics.contains(&index) {
                    "dynamic".to_string()
                } else {
                    csharp_type(value_type, typed).to_string()
                },
            })
            .collect(),
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn declaration_issue(kind: LoweringIssueKind, detail: String) -> LoweringIssue {
    LoweringIssue {
        offset: 0,
        opcode: OpCode::Unknown(0),
        kind,
        fidelity: Fidelity::Incomplete,
        detail,
    }
}

fn merge_value_types(left: ValueType, right: ValueType) -> ValueType {
    use ValueType::{Any, Null, Unknown};

    if left == right {
        return left;
    }
    match (left, right) {
        (Unknown, value) | (value, Unknown) => value,
        (Null, _) | (_, Null) => Any,
        _ => Any,
    }
}

pub(in crate::decompiler::csharp::render) fn csharp_type(
    value_type: ValueType,
    typed: bool,
) -> &'static str {
    match (typed, value_type) {
        (true, ValueType::Integer) => "BigInteger",
        (true, ValueType::Boolean) => "bool",
        (true, ValueType::ByteString) => "ByteString",
        (true, ValueType::Buffer) => "byte[]",
        (true, ValueType::Array | ValueType::Struct) => "object[]",
        (true, ValueType::Map) => "Map<object, object>",
        (
            _,
            ValueType::Unknown
            | ValueType::Any
            | ValueType::Null
            | ValueType::InteropInterface
            | ValueType::Pointer,
        ) => "dynamic",
        (false, _) => "dynamic",
    }
}
