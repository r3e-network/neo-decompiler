//! C# rendering policy for calls whose VM argument stack underflows.

use std::collections::BTreeSet;

use crate::decompiler::cfg::method_body::FidelityReport;
use crate::decompiler::csharp::helpers::escape_csharp_string;
use crate::decompiler::csharp::render::structured::plan::CSharpMethodPlan;
use crate::instruction::{Instruction, OpCode, Operand};

use super::super::LiftedBodyContext;

pub(in crate::decompiler::csharp::render::body) fn underflow_call_targets(
    instructions: &[Instruction],
    context: &LiftedBodyContext<'_>,
    fidelity: &FidelityReport,
) -> BTreeSet<String> {
    fidelity
        .issues
        .iter()
        .filter(|issue| {
            issue.detail.starts_with("requires ")
                && matches!(
                    issue.opcode,
                    OpCode::Call | OpCode::Call_L | OpCode::CallA | OpCode::Jmp | OpCode::Jmp_L
                )
        })
        .filter_map(|issue| {
            let instruction = instructions
                .iter()
                .find(|instruction| instruction.offset == issue.offset)?;
            let target = relative_target(instruction)?;
            context.method_labels_by_offset.get(&target).cloned()
        })
        .collect()
}

pub(in crate::decompiler::csharp::render::body) fn underflow_placeholder(
    method_plan: &CSharpMethodPlan,
    fidelity: &FidelityReport,
    targets: &BTreeSet<String>,
) -> Option<String> {
    if targets.is_empty() {
        return None;
    }
    let issue = fidelity.issues.iter().find(|issue| {
        issue.detail.starts_with("requires ")
            && matches!(
                issue.opcode,
                OpCode::Call | OpCode::Call_L | OpCode::CallA | OpCode::Jmp | OpCode::Jmp_L
            )
    })?;
    Some(format!(
        "(dynamic)(((object)null) ?? throw new InvalidOperationException(\"VM argument underflow in {} at 0x{:04X}: {}\"))",
        escape_csharp_string(&method_plan.emitted_name),
        issue.offset,
        escape_csharp_string(&issue.detail),
    ))
}

fn relative_target(instruction: &Instruction) -> Option<usize> {
    let delta = match instruction.operand {
        Some(Operand::Jump(value)) => value as isize,
        Some(Operand::Jump32(value)) => value as isize,
        _ => return None,
    };
    instruction.offset.checked_add_signed(delta)
}
