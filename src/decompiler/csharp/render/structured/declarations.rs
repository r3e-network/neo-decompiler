use super::super::super::super::helpers::sanitize_csharp_identifier;
use super::super::plan_activity::ActivityCollector;
use crate::decompiler::analysis::types::{TypeInfo, ValueType};
use crate::decompiler::cfg::method_body::{
    Fidelity, LoweringIssue, LoweringIssueKind, SymbolInfo, SymbolOrigin,
};
use crate::decompiler::ir::{Block, Expr};
use crate::decompiler::native_method_types;
use crate::instruction::OpCode;
use std::collections::{BTreeMap, BTreeSet, HashSet};
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(in crate::decompiler::csharp::render) struct DeclarationPlan {
    pub(in crate::decompiler::csharp::render) scopes: ScopeTree,
    pub(in crate::decompiler::csharp::render) declarations: BTreeMap<String, PlannedDeclaration>,
    pub(in crate::decompiler::csharp::render) issues: Vec<LoweringIssue>,
    pub(in crate::decompiler::csharp::render) typed: bool,
    pub(in crate::decompiler::csharp::render) index_defined_symbols: HashSet<String>,
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
    let mut collector = ActivityCollector::new();
    let root = collector.scopes.root();
    collector.visit_block(body, root);
    let index_defined_symbols = collector.index_defined_symbols();

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
                csharp_type: if typed && index_defined_symbols.contains(name) {
                    "dynamic".to_string()
                } else {
                    collector
                        .concrete_definition_types
                        .get(name)
                        .filter(|_| typed && activity.definitions.len() == 1)
                        .cloned()
                        .unwrap_or_else(|| csharp_type(symbol.value_type, typed).to_string())
                },
                initialize_to_default: !inline && symbol.origin == SymbolOrigin::Phi,
            },
        );
    }
    issues.sort_by(|left, right| {
        (left.kind, left.detail.as_str()).cmp(&(right.kind, right.detail.as_str()))
    });

    DeclarationPlan {
        scopes: collector.scopes,
        declarations,
        issues,
        typed,
        index_defined_symbols,
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

pub(in crate::decompiler::csharp::render) fn concrete_definition_type(
    expression: &Expr,
) -> Option<String> {
    if let Expr::Call {
        target:
            crate::decompiler::ir::SemanticCallTarget::MethodToken {
                name,
                hash_le,
                call_flags,
                ..
            },
        ..
    } = expression
    {
        if let Some(return_type) =
            native_method_types::lookup(hash_le.as_deref(), name, *call_flags)
        {
            return Some(return_type.csharp_type.to_string());
        }
    }
    if let Expr::Call {
        target: crate::decompiler::ir::SemanticCallTarget::Syscall { hash, .. },
        ..
    } = expression
    {
        if let Some(return_type) = crate::decompiler::syscall_types::lookup(*hash) {
            return Some(return_type.csharp_type.to_string());
        }
    }
    let Expr::NewArray {
        element_type: Some(element_type),
        ..
    } = expression
    else {
        return None;
    };
    Some(match element_type {
        ValueType::Boolean => "bool[]".to_string(),
        ValueType::Integer => "BigInteger[]".to_string(),
        ValueType::ByteString => "ByteString[]".to_string(),
        ValueType::Buffer => "byte[][]".to_string(),
        ValueType::Array | ValueType::Struct => "object[][]".to_string(),
        ValueType::Map => "Map<object, object>[]".to_string(),
        ValueType::Any | ValueType::Null | ValueType::InteropInterface | ValueType::Pointer => {
            "object[]".to_string()
        }
        ValueType::Unknown => return None,
    })
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
