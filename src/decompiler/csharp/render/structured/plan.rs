use std::collections::{BTreeMap, BTreeSet};
use std::ops::Index;

use crate::decompiler::analysis::method_contracts::ReturnBehavior;
use crate::decompiler::cfg::method_body::{LoweringIssue, MethodSymbolTypes, SymbolInfo};
use crate::decompiler::cfg::ssa::MethodContext;

use super::super::super::helpers::CSharpParameter;
#[path = "declaration_types.rs"]
mod declaration_types;
pub(in crate::decompiler::csharp::render) use declaration_types::concrete_definition_type_with_symbols_and_known_types;
pub(in crate::decompiler::csharp::render) use declaration_types::csharp_array_element_type;
pub(in crate::decompiler::csharp::render) use declaration_types::csharp_array_element_value_type;
pub(in crate::decompiler::csharp::render) use declaration_types::csharp_member_type;
pub(in crate::decompiler::csharp::render) use declaration_types::csharp_type_value_type;
#[cfg(test)]
pub(in crate::decompiler::csharp::render) use declaration_types::{
    concrete_definition_type, concrete_definition_type_with_symbols,
};
#[path = "declarations.rs"]
mod declarations;
pub(in crate::decompiler::csharp::render) use declarations::{
    collect_index_defined_symbols, csharp_type, plan_contract_symbols, plan_declarations,
    CSharpContractSymbols, DeclarationKind, DeclarationPlan, ScopeId, ScopeTree,
};

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

pub(super) struct MethodPlanDraft {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) raw_name: String,
    pub(super) parameters: Vec<CSharpParameter>,
    pub(super) return_type: String,
    pub(super) return_behavior: ReturnBehavior,
    pub(super) arguments_on_entry_stack: bool,
    pub(super) addressable_offset: Option<usize>,
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

#[path = "plan_methods.rs"]
mod methods;
#[path = "plan_helpers.rs"]
mod plan_helpers;
pub(in crate::decompiler::csharp::render) use methods::build_csharp_method_plans;
