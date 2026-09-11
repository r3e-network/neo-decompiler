//! High-level method bodies lowered through the structured IR spine.
//!
//! This is the dual-pipeline convergence path: instead of the stack emitter
//! plus string-pattern postprocess, a method body is lifted via
//! [`lower_method_body`] and rendered with the high-level IR dialect
//! (`ir::render_high_level_block`).

use std::collections::BTreeMap;
use std::fmt::Write;

use crate::decompiler::cfg::method_body::{
    lower_method_body, Fidelity, MethodIrRequest, MethodSymbolTypes,
};
use crate::decompiler::cfg::ssa::{CallContract, MethodContext};
use crate::decompiler::ir::{render_high_level_block, SemanticCallTarget};
use crate::instruction::{Instruction, OpCode};

use super::body::MethodBodyContext;

/// Render a method body from the structured IR. Returns `false` when the
/// caller should fall back to the legacy stack emitter (budget exceeded).
pub(super) fn write_method_body_from_ir(
    output: &mut String,
    instructions: &[Instruction],
    argument_labels: Option<&[String]>,
    warnings: &mut Vec<String>,
    context: &MethodBodyContext<'_>,
    returns_void: bool,
) -> bool {
    if instructions.is_empty() {
        writeln!(output, "        // no instructions decoded").unwrap();
        return true;
    }
    if instructions.len() > super::super::emitter::MAX_HIGH_LEVEL_METHOD_INSTRUCTIONS {
        return false;
    }

    let start = instructions.first().map(|i| i.offset).unwrap_or(0);
    let end = instructions.last().map(|i| i.offset + 1).unwrap_or(start);
    let argument_names = argument_labels
        .map(|labels| labels.to_vec())
        .unwrap_or_default();
    let arguments_on_entry_stack = instructions
        .first()
        .is_none_or(|instruction| instruction.opcode != OpCode::Initslot);

    let mut calls_by_offset = BTreeMap::new();
    for instruction in instructions {
        if matches!(
            instruction.opcode,
            OpCode::Call | OpCode::Call_L | OpCode::CallA | OpCode::CallT
        ) {
            if let Some(&target) = context.call_targets_by_offset.get(&instruction.offset) {
                let label = context
                    .method_labels_by_offset
                    .get(&target)
                    .cloned()
                    .unwrap_or_else(|| format!("fn_0x{target:04X}"));
                let argument_count = context
                    .method_arg_counts_by_offset
                    .get(&target)
                    .copied()
                    .unwrap_or(0);
                let returns_value = context
                    .method_returns_value_by_offset
                    .get(&target)
                    .copied()
                    .unwrap_or(true);
                let mut contract = CallContract::new(
                    SemanticCallTarget::Internal {
                        offset: target,
                        name: label,
                    },
                    argument_count,
                    returns_value,
                );
                if context.noreturn_method_offsets.contains(&target) {
                    contract.may_return = false;
                }
                calls_by_offset.insert(instruction.offset, contract);
            } else if let Some(&target) = context.calla_targets_by_offset.get(&instruction.offset) {
                let label = context
                    .method_labels_by_offset
                    .get(&target)
                    .cloned()
                    .unwrap_or_else(|| format!("fn_0x{target:04X}"));
                let argument_count = context
                    .method_arg_counts_by_offset
                    .get(&target)
                    .copied()
                    .unwrap_or(0);
                let returns_value = context
                    .method_returns_value_by_offset
                    .get(&target)
                    .copied()
                    .unwrap_or(true);
                calls_by_offset.insert(
                    instruction.offset,
                    CallContract::new(
                        SemanticCallTarget::Internal {
                            offset: target,
                            name: label,
                        },
                        argument_count,
                        returns_value,
                    ),
                );
            }
        }
    }

    let method_context = MethodContext {
        argument_names,
        arguments_on_entry_stack,
        // Only assert void when the caller is certain. Synthetic script_entry
        // without a manifest is not necessarily void at the VM level — leave
        // the return contract unknown so RET consumes the top when present.
        returns_value: returns_void.then_some(false),
        calls_by_offset,
        call_return_types: BTreeMap::new(),
        argument_collection_facts: Vec::new(),
        static_collection_facts: BTreeMap::new(),
    };

    let lowered = lower_method_body(MethodIrRequest {
        start,
        end,
        instructions,
        context: method_context,
        symbol_types: MethodSymbolTypes::default(),
        // High-level is a readability surface — apply the same temp
        // reductions the C# renderer uses.
        reduce_temps: true,
    });

    for issue in &lowered.fidelity.issues {
        if issue.fidelity == Fidelity::Incomplete {
            let warning = format!(
                "high-level-ir: method at 0x{start:04X}: {} at 0x{:04X}: {}",
                issue.opcode.mnemonic(),
                issue.offset,
                issue.detail
            );
            if !warnings.contains(&warning) {
                warnings.push(warning);
            }
        }
    }

    // Oversized / empty lowering falls back so output never vanishes.
    if lowered.fidelity.status == Fidelity::Incomplete
        && lowered.fidelity.issues.iter().any(|issue| {
            matches!(
                issue.kind,
                crate::decompiler::cfg::method_body::LoweringIssueKind::BudgetExceeded
            )
        })
    {
        return false;
    }

    // Methods whose IR is only comments (trampoline/JMP-out shapes the
    // structurer cannot represent) fall back to the stack emitter so the
    // instruction lifting stays visible.
    if block_is_comments_only(&lowered.body) {
        return false;
    }

    let body = render_high_level_block(&lowered.body, 0);
    if body.trim().is_empty() {
        writeln!(output, "        // empty body").unwrap();
        return true;
    }
    for line in body.lines() {
        if line.is_empty() {
            writeln!(output).unwrap();
        } else {
            writeln!(output, "        {line}").unwrap();
        }
    }
    true
}

fn block_is_comments_only(block: &crate::decompiler::ir::Block) -> bool {
    use crate::decompiler::ir::{ControlFlow, Stmt};
    fn walk(block: &crate::decompiler::ir::Block) -> bool {
        block.stmts.iter().all(|statement| match statement {
            Stmt::Comment(_) => true,
            Stmt::ControlFlow(control) => match control.as_ref() {
                ControlFlow::If {
                    then_branch,
                    else_branch,
                    ..
                } => walk(then_branch) && else_branch.as_ref().is_none_or(walk),
                ControlFlow::While { body, .. }
                | ControlFlow::DoWhile { body, .. }
                | ControlFlow::For { body, .. } => walk(body),
                ControlFlow::TryCatch {
                    try_body,
                    catch_body,
                    finally_body,
                    ..
                } => {
                    walk(try_body)
                        && catch_body.as_ref().is_none_or(walk)
                        && finally_body.as_ref().is_none_or(walk)
                }
                ControlFlow::Switch { cases, default, .. } => {
                    cases.iter().all(|(_, body)| walk(body)) && default.as_ref().is_none_or(walk)
                }
            },
            _ => false,
        })
    }
    !block.stmts.is_empty() && walk(block)
}
