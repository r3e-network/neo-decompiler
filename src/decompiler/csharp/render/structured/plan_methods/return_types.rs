use std::collections::BTreeMap;

use crate::decompiler::analysis::method_contracts::ReturnBehavior;
use crate::decompiler::analysis::types::ValueType;
use crate::decompiler::cfg::method_body::{
    lower_method_body, Fidelity, FidelityReport, LoweringIssueKind, MethodIrRequest, SymbolInfo,
};
use crate::decompiler::csharp::render::structured::expr::ExprContext;
use crate::decompiler::csharp::render::structured::stmt::terminates;
use crate::decompiler::ir::{Block, ControlFlow, Stmt};
use crate::instruction::Instruction;

use super::super::CSharpMethodPlan;

/// Resolve concrete C# return types for inferred private methods only.
///
/// The first pass starts with no inferred call types. Each subsequent pass
/// can therefore use a previously proven helper result while still rejecting
/// unresolved calls, mixed return types, bare returns, and partial bodies.
pub(super) fn infer_private_return_types(
    plans: &mut [CSharpMethodPlan],
    inferred_methods: &BTreeMap<usize, usize>,
    plans_by_offset: &BTreeMap<usize, Vec<usize>>,
    instructions: &[Instruction],
) {
    let iteration_limit = inferred_methods.len().saturating_add(1);
    for _ in 0..iteration_limit {
        let csharp_return_types = concrete_return_types_by_offset(plans, plans_by_offset);
        let call_return_types = csharp_return_types
            .iter()
            .filter_map(|(offset, return_type)| {
                csharp_return_value_type(return_type).map(|value_type| (*offset, value_type))
            })
            .collect::<BTreeMap<_, _>>();
        for plan in plans.iter_mut() {
            plan.method_context.call_return_types = call_return_types.clone();
        }

        let mut updates = Vec::new();
        for plan_index in inferred_methods.values() {
            let plan = &plans[*plan_index];
            if plan.return_behavior == ReturnBehavior::Void
                || plan.return_type != "dynamic"
                || plan.end <= plan.start
            {
                continue;
            }
            let lowered = lower_method_body(MethodIrRequest {
                start: plan.start,
                end: plan.end,
                instructions,
                context: plan.method_context.clone(),
                symbol_types: plan.symbol_types.clone(),
            });
            let Some(return_type) = inferred_return_type(
                &lowered.body,
                &lowered.symbols,
                &csharp_return_types,
                &lowered.fidelity,
            ) else {
                continue;
            };
            updates.push((*plan_index, return_type));
        }
        if updates.is_empty() {
            break;
        }
        for (plan_index, return_type) in updates {
            let plan = &mut plans[plan_index];
            plan.return_type = return_type.to_string();
            plan.return_behavior = ReturnBehavior::Value;
        }
    }

    let csharp_return_types = concrete_return_types_by_offset(plans, plans_by_offset);
    let call_return_types = csharp_return_types
        .iter()
        .filter_map(|(offset, return_type)| {
            csharp_return_value_type(return_type).map(|value_type| (*offset, value_type))
        })
        .collect::<BTreeMap<_, _>>();
    for plan in plans {
        plan.method_context.call_return_types = call_return_types.clone();
    }
}

fn concrete_return_types_by_offset(
    plans: &[CSharpMethodPlan],
    plans_by_offset: &BTreeMap<usize, Vec<usize>>,
) -> BTreeMap<usize, String> {
    plans_by_offset
        .iter()
        .filter_map(|(offset, candidates)| {
            let [plan_index] = candidates.as_slice() else {
                return None;
            };
            let return_type = plans[*plan_index].return_type.as_str();
            csharp_return_value_type(return_type).map(|_| (*offset, return_type.to_string()))
        })
        .collect()
}

fn csharp_return_value_type(return_type: &str) -> Option<ValueType> {
    match return_type {
        "BigInteger" => Some(ValueType::Integer),
        "bool" => Some(ValueType::Boolean),
        "string" => Some(ValueType::ByteString),
        "ByteString" => Some(ValueType::ByteString),
        "byte[]" => Some(ValueType::Buffer),
        "object[]" => Some(ValueType::Array),
        "Map<object, object>" => Some(ValueType::Map),
        "UInt160" | "UInt256" | "ECPoint" => Some(ValueType::ByteString),
        "StorageContext" | "Iterator" | "Transaction" => Some(ValueType::InteropInterface),
        _ => None,
    }
}

fn csharp_return_type(value_type: ValueType) -> Option<&'static str> {
    match value_type {
        ValueType::Integer => Some("BigInteger"),
        ValueType::Boolean => Some("bool"),
        ValueType::ByteString => Some("ByteString"),
        ValueType::Buffer => Some("byte[]"),
        ValueType::Array | ValueType::Struct => Some("object[]"),
        ValueType::Map => Some("Map<object, object>"),
        ValueType::Unknown
        | ValueType::Any
        | ValueType::Null
        | ValueType::InteropInterface
        | ValueType::Pointer => None,
    }
}

fn inferred_return_type(
    body: &Block,
    symbols: &BTreeMap<String, SymbolInfo>,
    internal_call_return_types: &BTreeMap<usize, String>,
    fidelity: &FidelityReport,
) -> Option<String> {
    if fidelity.status == Fidelity::Incomplete
        || fidelity
            .issues
            .iter()
            .any(|issue| issue.kind == LoweringIssueKind::UnresolvedCall)
        || !terminates(body)
    {
        return None;
    }
    let context = ExprContext::for_block(body, symbols, false)
        .with_internal_call_return_types(internal_call_return_types);
    collect_return_type(body, &context).ok().flatten()
}

fn collect_return_type(body: &Block, context: &ExprContext) -> Result<Option<String>, ()> {
    let mut candidate = None;
    for statement in &body.stmts {
        merge_return_type(
            &mut candidate,
            collect_statement_return_type(statement, context)?,
        )?;
    }
    Ok(candidate)
}

fn collect_statement_return_type(
    statement: &Stmt,
    context: &ExprContext,
) -> Result<Option<String>, ()> {
    match statement {
        Stmt::Return(Some(value)) => {
            let return_type = context
                .exact_csharp_type(value)
                .map(str::to_string)
                .or_else(|| csharp_return_type(context.value_type(value)).map(str::to_string))
                .ok_or(())?;
            Ok(Some(return_type))
        }
        Stmt::Return(None) => Err(()),
        Stmt::ControlFlow(control) => collect_control_return_type(control, context),
        _ => Ok(None),
    }
}

fn merge_return_type(
    candidate: &mut Option<String>,
    return_type: Option<String>,
) -> Result<(), ()> {
    let Some(return_type) = return_type else {
        return Ok(());
    };
    if candidate
        .as_ref()
        .is_some_and(|existing| existing != &return_type)
    {
        return Err(());
    }
    *candidate = Some(return_type);
    Ok(())
}

fn collect_control_return_type(
    control: &ControlFlow,
    context: &ExprContext,
) -> Result<Option<String>, ()> {
    let mut candidate = None;
    let collect = |candidate: &mut Option<String>, block: &Block| -> Result<(), ()> {
        merge_return_type(candidate, collect_return_type(block, context)?)
    };
    match control {
        ControlFlow::If {
            then_branch,
            else_branch,
            ..
        } => {
            collect(&mut candidate, then_branch)?;
            if let Some(else_branch) = else_branch {
                collect(&mut candidate, else_branch)?;
            }
        }
        ControlFlow::While { body, .. } | ControlFlow::DoWhile { body, .. } => {
            collect(&mut candidate, body)?;
        }
        ControlFlow::For { init, body, .. } => {
            if let Some(init) = init {
                merge_return_type(
                    &mut candidate,
                    collect_statement_return_type(init, context)?,
                )?;
            }
            collect(&mut candidate, body)?;
        }
        ControlFlow::TryCatch {
            try_body,
            catch_body,
            finally_body,
            ..
        } => {
            collect(&mut candidate, try_body)?;
            if let Some(catch_body) = catch_body {
                collect(&mut candidate, catch_body)?;
            }
            if let Some(finally_body) = finally_body {
                collect(&mut candidate, finally_body)?;
            }
        }
        ControlFlow::Switch { cases, default, .. } => {
            for (_, body) in cases {
                collect(&mut candidate, body)?;
            }
            if let Some(default) = default {
                collect(&mut candidate, default)?;
            }
        }
    }
    Ok(candidate)
}
