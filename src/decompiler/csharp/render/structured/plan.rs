use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::ops::Index;

use crate::decompiler::analysis::call_graph::{CallGraph, CallTarget};
use crate::decompiler::analysis::method_contracts::{MethodContracts, ReturnBehavior};
use crate::decompiler::analysis::types::{TypeInfo, ValueType};
use crate::decompiler::cfg::method_body::{
    lower_method_body, Fidelity, LoweringIssue, LoweringIssueKind, MethodIrRequest,
    MethodSymbolTypes, SymbolInfo, SymbolOrigin,
};
use crate::decompiler::cfg::ssa::{CallContract, MethodContext};
use crate::decompiler::helpers::{
    build_method_arg_counts_by_offset, find_manifest_entry_method, initslot_argument_count_at,
    next_inferred_method_offset, offset_as_usize,
};
use crate::decompiler::ir::{Block, Expr, SemanticCallTarget};
use crate::instruction::{Instruction, OpCode, Operand};
use crate::manifest::{ContractManifest, ManifestMethod};

use super::super::super::helpers::{
    collect_csharp_parameters, format_manifest_type_csharp, sanitize_csharp_identifier,
    CSharpParameter,
};
use super::plan_activity::ActivityCollector;

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

fn collect_index_defined_symbols(body: &Block) -> HashSet<String> {
    let mut collector = ActivityCollector::new();
    let root = collector.scopes.root();
    collector.visit_block(body, root);
    collector.index_defined_symbols()
}

pub(super) fn concrete_definition_type(expression: &Expr) -> Option<String> {
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

#[allow(dead_code)]
pub(in crate::decompiler::csharp::render) struct CSharpMethodPlan {
    pub(in crate::decompiler::csharp::render) start: usize,
    pub(in crate::decompiler::csharp::render) end: usize,
    pub(in crate::decompiler::csharp::render) raw_name: String,
    pub(in crate::decompiler::csharp::render) emitted_name: String,
    pub(in crate::decompiler::csharp::render) parameters: Vec<CSharpParameter>,
    pub(in crate::decompiler::csharp::render) return_type: String,
    pub(in crate::decompiler::csharp::render) return_behavior: ReturnBehavior,
    pub(in crate::decompiler::csharp::render) method_context: MethodContext,
    pub(in crate::decompiler::csharp::render) symbol_types: MethodSymbolTypes,
    pub(in crate::decompiler::csharp::render) planning_issues: Vec<LoweringIssue>,
}

pub(in crate::decompiler::csharp::render) struct CSharpMethodPlans {
    plans: Vec<CSharpMethodPlan>,
    method_symbol_maps: Vec<BTreeMap<String, SymbolInfo>>,
    synthetic_entry: Option<usize>,
    fallback_entry: Option<usize>,
    manifest_methods: Vec<usize>,
    inferred_methods: BTreeMap<usize, usize>,
    method_labels_by_offset: BTreeMap<usize, String>,
    method_arg_counts_by_offset: BTreeMap<usize, usize>,
    method_return_types_by_offset: BTreeMap<usize, String>,
    index_defined_statics: BTreeSet<usize>,
}

impl CSharpMethodPlans {
    pub(in crate::decompiler::csharp::render) fn emitted_names(
        &self,
    ) -> impl Iterator<Item = &str> {
        self.plans.iter().map(|plan| plan.emitted_name.as_str())
    }

    pub(in crate::decompiler::csharp::render) fn synthetic_entry(
        &self,
    ) -> Option<&CSharpMethodPlan> {
        self.synthetic_entry.map(|index| &self.plans[index])
    }

    pub(in crate::decompiler::csharp::render) fn fallback_entry(
        &self,
    ) -> Option<&CSharpMethodPlan> {
        self.fallback_entry.map(|index| &self.plans[index])
    }

    pub(in crate::decompiler::csharp::render) fn manifest_method(
        &self,
        index: usize,
    ) -> &CSharpMethodPlan {
        &self.plans[self.manifest_methods[index]]
    }

    pub(in crate::decompiler::csharp::render) fn inferred_method(
        &self,
        start: usize,
    ) -> Option<&CSharpMethodPlan> {
        self.inferred_methods
            .get(&start)
            .map(|index| &self.plans[*index])
    }

    pub(in crate::decompiler::csharp::render) fn method_labels_by_offset(
        &self,
    ) -> &BTreeMap<usize, String> {
        &self.method_labels_by_offset
    }

    pub(in crate::decompiler::csharp::render) fn method_arg_counts_by_offset(
        &self,
    ) -> &BTreeMap<usize, usize> {
        &self.method_arg_counts_by_offset
    }

    pub(in crate::decompiler::csharp::render) fn method_return_types_by_offset(
        &self,
    ) -> &BTreeMap<usize, String> {
        &self.method_return_types_by_offset
    }

    pub(in crate::decompiler::csharp::render) fn method_symbol_maps(
        &self,
    ) -> &[BTreeMap<String, SymbolInfo>] {
        &self.method_symbol_maps
    }

    pub(in crate::decompiler::csharp::render) fn index_defined_statics(&self) -> &BTreeSet<usize> {
        &self.index_defined_statics
    }
}

impl Index<usize> for CSharpMethodPlans {
    type Output = CSharpMethodPlan;

    fn index(&self, index: usize) -> &Self::Output {
        &self.plans[index]
    }
}

struct MethodPlanDraft {
    start: usize,
    end: usize,
    raw_name: String,
    parameters: Vec<CSharpParameter>,
    return_type: String,
    return_behavior: ReturnBehavior,
    arguments_on_entry_stack: bool,
    addressable_offset: Option<usize>,
}

fn draft_method_context(
    draft: &MethodPlanDraft,
    method_contracts: &MethodContracts,
) -> MethodContext {
    let method_contract = method_contracts.get(draft.start);
    MethodContext {
        argument_names: draft
            .parameters
            .iter()
            .map(|parameter| parameter.name.clone())
            .collect(),
        arguments_on_entry_stack: draft.arguments_on_entry_stack,
        returns_value: return_value_option(draft.return_behavior),
        calls_by_offset: BTreeMap::new(),
        argument_collection_facts: method_contract
            .map(|contract| contract.argument_collection_facts.clone())
            .unwrap_or_default(),
        static_collection_facts: method_contracts.static_collection_facts.clone(),
    }
}

pub(in crate::decompiler::csharp::render) fn build_csharp_method_plans(
    instructions: &[Instruction],
    manifest: Option<&ContractManifest>,
    call_graph: &CallGraph,
    method_contracts: &MethodContracts,
    types: &TypeInfo,
    inferred_method_starts: &[usize],
) -> CSharpMethodPlans {
    let entry_offset = instructions
        .first()
        .map_or(0, |instruction| instruction.offset);
    let script_end = instructions
        .last()
        .map_or(entry_offset, |instruction| instruction.offset + 1);
    let inferred_argument_counts =
        build_method_arg_counts_by_offset(instructions, inferred_method_starts, manifest);
    let mut drafts = Vec::new();
    let mut synthetic_entry = None;
    let mut fallback_entry = None;
    let mut manifest_methods = Vec::new();
    let mut inferred_methods = BTreeMap::new();

    if let Some(manifest) = manifest {
        let entry_method = instructions
            .first()
            .and_then(|_| find_manifest_entry_method(manifest, entry_offset));
        if !instructions.is_empty() && entry_method.is_none() {
            let index = drafts.len();
            drafts.push(synthetic_entry_draft(
                instructions,
                inferred_method_starts,
                entry_offset,
                script_end,
            ));
            synthetic_entry = Some(index);
        }

        let mut sorted_methods: Vec<_> = manifest.abi.methods.iter().collect();
        sorted_methods.sort_by_key(|method| method.offset.unwrap_or(i32::MAX));
        let (with_offsets, without_offsets): (Vec<_>, Vec<_>) = sorted_methods
            .into_iter()
            .partition(|method| method.offset.is_some());

        for method in with_offsets.into_iter().chain(without_offsets) {
            let explicit_start = offset_as_usize(method.offset);
            let is_offsetless_entry = explicit_start.is_none()
                && entry_method
                    .as_ref()
                    .is_some_and(|(entry, _)| std::ptr::eq(*entry, method));
            let addressable_offset = explicit_start.or(is_offsetless_entry.then_some(entry_offset));
            let start = addressable_offset.unwrap_or(entry_offset);
            let end = addressable_offset.map_or(start, |start| {
                method_end(inferred_method_starts, start, script_end)
            });
            let index = drafts.len();
            drafts.push(manifest_method_draft(
                method,
                start,
                end,
                addressable_offset,
                instructions,
            ));
            manifest_methods.push(index);
        }
    } else {
        let index = drafts.len();
        drafts.push(synthetic_entry_draft(
            instructions,
            inferred_method_starts,
            entry_offset,
            script_end,
        ));
        fallback_entry = Some(index);
    }

    let manifest_offsets: HashSet<usize> = manifest
        .map(|manifest| {
            manifest
                .abi
                .methods
                .iter()
                .filter_map(|method| offset_as_usize(method.offset))
                .collect()
        })
        .unwrap_or_default();
    for start in inferred_method_starts {
        if *start == entry_offset || manifest_offsets.contains(start) {
            continue;
        }
        let end = method_end(inferred_method_starts, *start, script_end);
        let slice = instructions
            .iter()
            .filter(|instruction| instruction.offset >= *start && instruction.offset < end)
            .collect::<Vec<_>>();
        if slice.is_empty()
            || slice
                .iter()
                .all(|instruction| instruction.opcode == OpCode::Nop)
        {
            continue;
        }

        let method_contract = method_contracts.get(*start);
        let argument_count = method_contract.map_or_else(
            || inferred_argument_counts.get(start).copied().unwrap_or(0),
            |contract| contract.argument_count,
        );
        let return_behavior =
            method_contract.map_or(ReturnBehavior::Unknown, |contract| contract.return_behavior);
        let parameters = (0..argument_count)
            .map(|index| CSharpParameter {
                name: format!("arg{index}"),
                ty: "dynamic".to_string(),
            })
            .collect();
        let index = drafts.len();
        drafts.push(MethodPlanDraft {
            start: *start,
            end,
            raw_name: format!("sub_0x{start:04X}"),
            parameters,
            return_type: if return_behavior == ReturnBehavior::Void {
                "void".to_string()
            } else {
                "dynamic".to_string()
            },
            return_behavior,
            arguments_on_entry_stack: instructions
                .iter()
                .find(|instruction| instruction.offset == *start)
                .is_none_or(|instruction| instruction.opcode != OpCode::Initslot),
            addressable_offset: Some(*start),
        });
        inferred_methods.insert(*start, index);
    }

    for draft in &mut drafts {
        let null_checked = null_checked_argument_indices(instructions, draft.start, draft.end);
        for index in null_checked {
            let Some(parameter) = draft.parameters.get_mut(index) else {
                continue;
            };
            if matches!(parameter.ty.as_str(), "BigInteger" | "bool") {
                parameter.ty = "dynamic".to_string();
            }
        }
    }

    let method_symbol_maps = drafts
        .iter()
        .filter(|draft| draft.end > draft.start)
        .map(|draft| {
            lower_method_body(MethodIrRequest {
                start: draft.start,
                end: draft.end,
                instructions,
                context: draft_method_context(draft, method_contracts),
                symbol_types: method_symbol_types(types, draft.start, &draft.parameters),
            })
            .symbols
        })
        .collect::<Vec<_>>();
    let method_symbols = method_symbol_maps.iter().collect::<Vec<_>>();
    let reserved_member_names =
        plan_contract_symbols(types, &method_symbols, false, &BTreeSet::new())
            .static_fields
            .into_iter()
            .map(|field| field.name)
            .collect();
    let mut used_signatures = HashSet::new();
    let mut base_occurrences = HashMap::new();
    let mut plans = drafts
        .iter()
        .map(|draft| {
            let base_name = sanitize_csharp_identifier(&draft.raw_name);
            let emitted_name = make_unique_method_name(
                base_name,
                &parameter_type_signature(&draft.parameters),
                &mut used_signatures,
                &mut base_occurrences,
                &reserved_member_names,
            );
            CSharpMethodPlan {
                start: draft.start,
                end: draft.end,
                raw_name: draft.raw_name.clone(),
                emitted_name,
                parameters: draft.parameters.clone(),
                return_type: draft.return_type.clone(),
                return_behavior: draft.return_behavior,
                method_context: draft_method_context(draft, method_contracts),
                symbol_types: method_symbol_types(types, draft.start, &draft.parameters),
                planning_issues: Vec::new(),
            }
        })
        .collect::<Vec<_>>();

    let mut plans_by_offset: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (index, draft) in drafts.iter().enumerate() {
        if let Some(offset) = draft.addressable_offset {
            plans_by_offset.entry(offset).or_default().push(index);
        }
    }

    let mut calls_and_issues = Vec::with_capacity(plans.len());
    for plan in &plans {
        let mut calls_by_offset = BTreeMap::new();
        let mut planning_issues = Vec::new();
        for edge in call_graph
            .edges
            .iter()
            .filter(|edge| edge.call_offset >= plan.start && edge.call_offset < plan.end)
        {
            let Some(instruction) = instructions
                .iter()
                .find(|instruction| instruction.offset == edge.call_offset)
            else {
                continue;
            };
            let contract = match &edge.target {
                CallTarget::Internal { method } => match plans_by_offset.get(&method.offset) {
                    Some(candidates) if candidates.len() == 1 => {
                        let target = &plans[candidates[0]];
                        CallContract::new(
                            SemanticCallTarget::Internal {
                                offset: method.offset,
                                name: target.emitted_name.clone(),
                            },
                            target.parameters.len(),
                            target.return_behavior.returns_value(),
                        )
                        .with_may_return(
                            method_contracts
                                .get(method.offset)
                                .is_none_or(|contract| contract.may_return),
                        )
                        .with_return_shape(
                            method_contracts
                                .get(method.offset)
                                .and_then(|contract| contract.return_shape),
                        )
                        .with_argument_effects(
                            method_contracts
                                .get(method.offset)
                                .map(|contract| contract.argument_effects.clone())
                                .unwrap_or_default(),
                        )
                        .with_argument_field_writes(
                            method_contracts
                                .get(method.offset)
                                .map(|contract| contract.argument_field_writes.clone())
                                .unwrap_or_default(),
                        )
                    }
                    candidates => {
                        let declaration_count = candidates.map_or(0, Vec::len);
                        planning_issues.push(LoweringIssue {
                            offset: edge.call_offset,
                            opcode: instruction.opcode,
                            kind: LoweringIssueKind::UnresolvedCall,
                            fidelity: Fidelity::Incomplete,
                            detail: format!(
                                "internal call target 0x{:04X} matches {declaration_count} emitted C# declarations",
                                method.offset
                            ),
                        });
                        let method_contract = method_contracts.get(method.offset);
                        CallContract::new(
                            SemanticCallTarget::Unresolved {
                                display_name: format!("call_0x{:04X}", method.offset),
                            },
                            method_contract.map_or(0, |contract| contract.argument_count),
                            method_contract
                                .is_none_or(|contract| contract.return_behavior.returns_value()),
                        )
                        .with_may_return(method_contract.is_none_or(|contract| contract.may_return))
                    }
                },
                CallTarget::MethodToken {
                    index,
                    hash_le,
                    method,
                    parameters_count,
                    has_return_value,
                    call_flags,
                    ..
                } => CallContract::new(
                    SemanticCallTarget::MethodToken {
                        index: usize::from(*index),
                        name: method.clone(),
                        hash_le: Some(hash_le.clone()),
                        call_flags: Some(*call_flags),
                    },
                    usize::from(*parameters_count),
                    *has_return_value,
                ),
                _ => continue,
            };
            calls_by_offset.insert(edge.call_offset, contract);
        }
        for instruction in instructions
            .iter()
            .filter(|instruction| instruction.offset >= plan.start && instruction.offset < plan.end)
        {
            let Some(target_offset) = cross_range_tail_target(instruction, plan.start, plan.end)
            else {
                continue;
            };
            let Some(candidates) = plans_by_offset.get(&target_offset) else {
                continue;
            };
            if candidates.len() != 1 {
                continue;
            }
            let target = &plans[candidates[0]];
            let target_contract = method_contracts.get(target_offset);
            if target_contract.is_some_and(|contract| !contract.may_return) {
                continue;
            }
            calls_by_offset.insert(
                instruction.offset,
                CallContract::new(
                    SemanticCallTarget::Internal {
                        offset: target_offset,
                        name: target.emitted_name.clone(),
                    },
                    target.parameters.len(),
                    plan.return_behavior.returns_value(),
                )
                .with_may_return(true)
                .with_argument_effects(
                    target_contract
                        .map(|contract| contract.argument_effects.clone())
                        .unwrap_or_default(),
                )
                .with_argument_field_writes(
                    target_contract
                        .map(|contract| contract.argument_field_writes.clone())
                        .unwrap_or_default(),
                ),
            );
        }
        planning_issues.sort_by(|left, right| {
            (
                left.offset,
                left.opcode.byte(),
                left.kind,
                left.detail.as_str(),
            )
                .cmp(&(
                    right.offset,
                    right.opcode.byte(),
                    right.kind,
                    right.detail.as_str(),
                ))
        });
        planning_issues.dedup();
        calls_and_issues.push((calls_by_offset, planning_issues));
    }
    for (plan, (calls_by_offset, planning_issues)) in plans.iter_mut().zip(calls_and_issues) {
        plan.method_context.calls_by_offset = calls_by_offset;
        plan.planning_issues = planning_issues;
    }

    let mut parameter_index_definitions: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    let mut index_defined_statics = BTreeSet::new();
    for (plan_index, plan) in plans.iter().enumerate() {
        if plan.end <= plan.start {
            continue;
        }
        let lowered = lower_method_body(MethodIrRequest {
            start: plan.start,
            end: plan.end,
            instructions,
            context: plan.method_context.clone(),
            symbol_types: plan.symbol_types.clone(),
        });
        for name in collect_index_defined_symbols(&lowered.body) {
            match lowered.symbols.get(&name).map(|symbol| &symbol.origin) {
                Some(SymbolOrigin::Parameter(index)) => {
                    parameter_index_definitions
                        .entry(plan_index)
                        .or_default()
                        .insert(*index);
                }
                Some(SymbolOrigin::Static(index)) => {
                    index_defined_statics.insert(*index);
                }
                _ => {}
            }
        }
    }

    let mut parameter_types_changed = false;
    for (plan_index, indices) in parameter_index_definitions {
        let plan = &mut plans[plan_index];
        for index in indices {
            if let Some(parameter) = plan.parameters.get_mut(index) {
                parameter_types_changed |= parameter.ty != "dynamic";
                parameter.ty = "dynamic".to_string();
            }
            if let Some(value_type) = plan.symbol_types.parameters.get_mut(index) {
                *value_type = ValueType::Unknown;
            }
        }
    }
    for plan in &mut plans {
        if let Some(last_index) = index_defined_statics.last().copied() {
            plan.symbol_types
                .statics
                .resize(last_index + 1, ValueType::Unknown);
        }
        for index in &index_defined_statics {
            plan.symbol_types.statics[*index] = ValueType::Unknown;
        }
    }

    if parameter_types_changed {
        let mut used_signatures = HashSet::new();
        let mut base_occurrences = HashMap::new();
        for plan in &mut plans {
            plan.emitted_name = make_unique_method_name(
                sanitize_csharp_identifier(&plan.raw_name),
                &parameter_type_signature(&plan.parameters),
                &mut used_signatures,
                &mut base_occurrences,
                &reserved_member_names,
            );
        }
        let emitted_names_by_offset = plans_by_offset
            .iter()
            .filter(|(_, candidates)| candidates.len() == 1)
            .map(|(offset, candidates)| (*offset, plans[candidates[0]].emitted_name.clone()))
            .collect::<BTreeMap<_, _>>();
        for plan in &mut plans {
            for contract in plan.method_context.calls_by_offset.values_mut() {
                let SemanticCallTarget::Internal { offset, name } = &mut contract.target else {
                    continue;
                };
                if let Some(emitted_name) = emitted_names_by_offset.get(offset) {
                    *name = emitted_name.clone();
                }
            }
        }
    }

    let mut method_labels_by_offset = BTreeMap::new();
    let mut method_arg_counts_by_offset = BTreeMap::new();
    let mut method_return_types_by_offset = BTreeMap::new();
    for (offset, candidates) in &plans_by_offset {
        if candidates.len() != 1 {
            continue;
        }
        let plan = &plans[candidates[0]];
        method_labels_by_offset.insert(*offset, plan.emitted_name.clone());
        method_arg_counts_by_offset.insert(*offset, plan.parameters.len());
        method_return_types_by_offset.insert(*offset, plan.return_type.clone());
    }

    CSharpMethodPlans {
        plans,
        method_symbol_maps,
        synthetic_entry,
        fallback_entry,
        manifest_methods,
        inferred_methods,
        method_labels_by_offset,
        method_arg_counts_by_offset,
        method_return_types_by_offset,
        index_defined_statics,
    }
}

fn cross_range_tail_target(
    instruction: &Instruction,
    method_start: usize,
    method_end: usize,
) -> Option<usize> {
    if !matches!(instruction.opcode, OpCode::Jmp | OpCode::Jmp_L) {
        return None;
    }
    let target = match instruction.operand {
        Some(Operand::Jump(delta)) => instruction.offset.checked_add_signed(delta as isize),
        Some(Operand::Jump32(delta)) => instruction.offset.checked_add_signed(delta as isize),
        _ => None,
    }?;
    (!(method_start..method_end).contains(&target)).then_some(target)
}

fn synthetic_entry_draft(
    instructions: &[Instruction],
    inferred_method_starts: &[usize],
    entry_offset: usize,
    script_end: usize,
) -> MethodPlanDraft {
    let argument_count = initslot_argument_count_at(instructions, entry_offset).unwrap_or(0);
    MethodPlanDraft {
        start: entry_offset,
        end: method_end(inferred_method_starts, entry_offset, script_end),
        raw_name: "ScriptEntry".to_string(),
        parameters: (0..argument_count)
            .map(|index| CSharpParameter {
                name: format!("arg{index}"),
                ty: "object".to_string(),
            })
            .collect(),
        return_type: "object".to_string(),
        return_behavior: ReturnBehavior::Unknown,
        arguments_on_entry_stack: instructions
            .iter()
            .find(|instruction| instruction.offset == entry_offset)
            .is_none_or(|instruction| instruction.opcode != OpCode::Initslot),
        addressable_offset: Some(entry_offset),
    }
}

fn manifest_method_draft(
    method: &ManifestMethod,
    start: usize,
    end: usize,
    addressable_offset: Option<usize>,
    instructions: &[Instruction],
) -> MethodPlanDraft {
    let return_type = format_manifest_type_csharp(&method.return_type, true);
    MethodPlanDraft {
        start,
        end,
        raw_name: method.name.clone(),
        parameters: collect_csharp_parameters(&method.parameters),
        return_behavior: if return_type == "void" {
            ReturnBehavior::Void
        } else {
            ReturnBehavior::Value
        },
        return_type,
        arguments_on_entry_stack: instructions
            .iter()
            .find(|instruction| instruction.offset == start)
            .is_none_or(|instruction| instruction.opcode != OpCode::Initslot),
        addressable_offset,
    }
}

fn method_end(inferred_method_starts: &[usize], start: usize, script_end: usize) -> usize {
    next_inferred_method_offset(inferred_method_starts, start).unwrap_or(script_end)
}

fn null_checked_argument_indices(
    instructions: &[Instruction],
    start: usize,
    end: usize,
) -> BTreeSet<usize> {
    let method = instructions
        .iter()
        .filter(|instruction| instruction.offset >= start && instruction.offset < end)
        .collect::<Vec<_>>();
    let mut checked = BTreeSet::new();
    for (index, instruction) in method.iter().enumerate() {
        if instruction.opcode != OpCode::Isnull {
            continue;
        }
        let source = index
            .checked_sub(1)
            .and_then(|source| argument_load_index(method[source]))
            .or_else(|| {
                (index >= 2 && method[index - 1].opcode == OpCode::Dup)
                    .then(|| argument_load_index(method[index - 2]))
                    .flatten()
            });
        if let Some(source) = source {
            checked.insert(source);
        }
    }
    checked
}

fn argument_load_index(instruction: &Instruction) -> Option<usize> {
    match instruction.opcode {
        OpCode::Ldarg0 => Some(0),
        OpCode::Ldarg1 => Some(1),
        OpCode::Ldarg2 => Some(2),
        OpCode::Ldarg3 => Some(3),
        OpCode::Ldarg4 => Some(4),
        OpCode::Ldarg5 => Some(5),
        OpCode::Ldarg6 => Some(6),
        OpCode::Ldarg => match instruction.operand {
            Some(Operand::U8(index)) => Some(usize::from(index)),
            _ => None,
        },
        _ => None,
    }
}

fn parameter_type_signature(parameters: &[CSharpParameter]) -> String {
    parameters
        .iter()
        .map(|parameter| parameter.ty.as_str())
        .collect::<Vec<_>>()
        .join(",")
}

fn make_unique_method_name(
    base: String,
    signature: &str,
    used: &mut HashSet<(String, String)>,
    base_occurrences: &mut HashMap<String, usize>,
    reserved_member_names: &HashSet<String>,
) -> String {
    let occurrence = base_occurrences.entry(base.clone()).or_default();
    let mut suffix = *occurrence;
    *occurrence += 1;

    if !reserved_member_names.contains(&base) && used.insert((base.clone(), signature.to_string()))
    {
        return base;
    }

    suffix = suffix.max(1);
    loop {
        let candidate = format!("{base}_{suffix}");
        if !reserved_member_names.contains(&candidate)
            && used.insert((candidate.clone(), signature.to_string()))
        {
            return candidate;
        }
        suffix += 1;
    }
}

fn return_value_option(return_behavior: ReturnBehavior) -> Option<bool> {
    match return_behavior {
        ReturnBehavior::Value => Some(true),
        ReturnBehavior::Void => Some(false),
        ReturnBehavior::Unknown => None,
    }
}

fn method_symbol_types(
    types: &TypeInfo,
    start: usize,
    csharp_parameters: &[CSharpParameter],
) -> MethodSymbolTypes {
    let inferred = types
        .methods
        .iter()
        .find(|method| method.method.offset == start);
    let mut parameters = inferred
        .map(|method| method.arguments.clone())
        .unwrap_or_default();
    parameters.resize(csharp_parameters.len(), ValueType::Unknown);
    for (value_type, parameter) in parameters.iter_mut().zip(csharp_parameters) {
        if *value_type == ValueType::Unknown && parameter.ty == "object" {
            *value_type = ValueType::Any;
        }
    }
    MethodSymbolTypes {
        parameters,
        locals: inferred
            .map(|method| method.locals.clone())
            .unwrap_or_default(),
        statics: types.statics.clone(),
    }
}
