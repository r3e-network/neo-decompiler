use std::collections::{BTreeMap, HashMap, HashSet};

use crate::decompiler::analysis::method_contracts::{MethodContracts, ReturnBehavior};
use crate::decompiler::analysis::types::{TypeInfo, ValueType};
use crate::decompiler::cfg::method_body::MethodSymbolTypes;
use crate::decompiler::cfg::ssa::MethodContext;
use crate::decompiler::csharp::helpers::{
    collect_csharp_parameters, format_manifest_type_csharp, CSharpParameter,
};
use crate::decompiler::helpers::{initslot_argument_count_at, next_inferred_method_offset};
use crate::instruction::{Instruction, OpCode, Operand};
use crate::manifest::ManifestMethod;

use super::MethodPlanDraft;

pub(super) fn draft_method_context(
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
        call_return_types: BTreeMap::new(),
        argument_collection_facts: method_contract
            .map(|contract| contract.argument_collection_facts.clone())
            .unwrap_or_default(),
        static_collection_facts: method_contracts.static_collection_facts.clone(),
    }
}

pub(super) fn cross_range_tail_target(
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

pub(super) fn synthetic_entry_draft(
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

pub(super) fn manifest_method_draft(
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

pub(super) fn method_end(
    inferred_method_starts: &[usize],
    start: usize,
    script_end: usize,
) -> usize {
    next_inferred_method_offset(inferred_method_starts, start).unwrap_or(script_end)
}

pub(super) fn parameter_type_signature(parameters: &[CSharpParameter]) -> String {
    parameters
        .iter()
        .map(|parameter| parameter.ty.as_str())
        .collect::<Vec<_>>()
        .join(",")
}

pub(super) fn make_unique_method_name(
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

pub(super) fn return_value_option(return_behavior: ReturnBehavior) -> Option<bool> {
    match return_behavior {
        ReturnBehavior::Value => Some(true),
        ReturnBehavior::Void => Some(false),
        ReturnBehavior::Unknown => None,
    }
}

pub(super) fn method_symbol_types(
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
