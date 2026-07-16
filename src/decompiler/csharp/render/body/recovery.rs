use std::collections::BTreeMap;
use std::fmt::Write;

use crate::decompiler::analysis::method_contracts::ReturnBehavior;
use crate::decompiler::cfg::method_body::{FidelityReport, LoweringIssueKind, SymbolInfo};
use crate::decompiler::ir::{Block, Stmt};
use crate::instruction::{Instruction, OpCode, Operand};

use super::super::super::helpers::VM_ASSERT_MESSAGE_HELPER;
use super::super::structured::plan::CSharpMethodPlan;
use super::super::structured::stmt;
use super::fidelity::semantic_warnings;
use super::{plan_method_declarations, BodyBackend, BodyRenderResult, LiftedBodyContext};

const MAX_RENDERED_PLACEHOLDER_ARGUMENTS: usize = 256;

/// Preserve a useful C# call shape when structured lifting reports a recoverable
/// stack issue. The emitted placeholders are limited to the callee's declared
/// arity so compatibility recovery never invents an extra argument for a
/// zero-parameter method.
pub(super) fn recovered_result(
    instructions: &[Instruction],
    method_plan: &CSharpMethodPlan,
    context: &LiftedBodyContext<'_>,
    body: &Block,
    symbols: &BTreeMap<String, SymbolInfo>,
    fidelity: FidelityReport,
) -> BodyRenderResult {
    let mut source = String::new();
    if body
        .stmts
        .iter()
        .any(|statement| !matches!(statement, Stmt::Comment(_) | Stmt::Return(None)))
    {
        let declarations = plan_method_declarations(body, symbols, method_plan, context);
        let structured = stmt::render_block_with_trace(
            body,
            &declarations,
            symbols,
            method_plan.return_behavior,
            context.inline_single_use_temps,
            context
                .assert_message_helper_call
                .unwrap_or(VM_ASSERT_MESSAGE_HELPER),
            context.vm_exception_type,
            context.bare_throw_helper_call,
            context.unpack_packstruct_helper_call,
            context.tagged_opcode_helper_calls,
            context.method_return_types_by_offset,
            Some(&method_plan.return_type),
            None,
            instructions,
            context.event_signatures,
        );
        let structured = ensure_non_void_termination(structured, body, method_plan.return_behavior);
        let structured = prepend_argument_underflow_comment(structured, method_plan, &fidelity);
        if !structured.trim().is_empty() {
            return BodyRenderResult {
                source: indent_body(&structured),
                backend: BodyBackend::Structured,
                warnings: semantic_warnings(method_plan, &fidelity),
                fidelity,
            };
        }
    }
    let mut rendered = false;
    let argument_underflow = fidelity
        .issues
        .iter()
        .any(|issue| issue.detail.starts_with("requires "));
    let argument_underflow_detail = fidelity
        .issues
        .iter()
        .find(|issue| issue.detail.starts_with("requires "))
        .map(|issue| issue.detail.as_str())
        .unwrap_or("missing stack arguments");

    for instruction in instructions {
        let target = match instruction.operand {
            Some(Operand::Jump(delta)) => instruction.offset.checked_add_signed(delta as isize),
            Some(Operand::Jump32(delta)) => instruction.offset.checked_add_signed(delta as isize),
            _ => None,
        };
        let target = target.and_then(|offset| {
            context
                .method_labels_by_offset
                .get(&offset)
                .map(|name| (offset, name))
        });
        let (target_offset, target_name) = match target {
            Some(target) => target,
            None => continue,
        };
        let argument_count = context
            .method_arg_counts_by_offset
            .get(&target_offset)
            .copied()
            .unwrap_or(0);
        let args = render_placeholder_arguments(argument_count);
        let call = format!("{target_name}({args})");
        match instruction.opcode {
            OpCode::Call | OpCode::Call_L | OpCode::CallA | OpCode::Jmp | OpCode::Jmp_L => {
                if argument_underflow {
                    writeln!(
                        source,
                        "            // VM argument underflow: {target_name}: {argument_underflow_detail}; substituted (dynamic)null arguments."
                    )
                    .unwrap();
                }
                if method_plan.return_behavior == ReturnBehavior::Void {
                    writeln!(source, "            {call};").unwrap();
                } else {
                    writeln!(source, "            return {call};").unwrap();
                }
                rendered = true;
                break;
            }
            _ => {}
        }
    }

    if !rendered {
        if let Some(instruction) = instructions
            .iter()
            .find(|instruction| matches!(instruction.opcode, OpCode::Endtry | OpCode::EndtryL))
        {
            if let Some(target) = match instruction.operand {
                Some(Operand::Jump(delta)) => instruction.offset.checked_add_signed(delta as isize),
                Some(Operand::Jump32(delta)) => {
                    instruction.offset.checked_add_signed(delta as isize)
                }
                _ => None,
            } {
                writeln!(source, "            goto label_0x{target:04X};").unwrap();
                writeln!(source, "            label_0x{target:04X}:;").unwrap();
                rendered = true;
            }
        }
    }

    if !rendered && method_plan.return_behavior != ReturnBehavior::Void {
        source.push_str("            throw new NotImplementedException();\n");
    }
    BodyRenderResult {
        source,
        backend: BodyBackend::Structured,
        warnings: semantic_warnings(method_plan, &fidelity),
        fidelity,
    }
}

fn render_placeholder_arguments(argument_count: usize) -> String {
    let rendered_count = argument_count.min(MAX_RENDERED_PLACEHOLDER_ARGUMENTS);
    let mut arguments = std::iter::repeat_n("(dynamic)null", rendered_count)
        .map(str::to_string)
        .collect::<Vec<_>>();
    if argument_count > rendered_count {
        arguments.push(format!(
            "/* omitted {} additional VM arguments */ (dynamic)null",
            argument_count - rendered_count
        ));
    }
    arguments.join(", ")
}

pub(super) fn indent_body(source: &str) -> String {
    let mut indented = String::new();
    for line in source.lines() {
        writeln!(indented, "            {line}").unwrap();
    }
    indented
}

pub(super) fn ensure_non_void_termination(
    mut source: String,
    body: &Block,
    return_behavior: ReturnBehavior,
) -> String {
    if return_behavior != ReturnBehavior::Void && !stmt::terminates(body) {
        if !source.is_empty() && !source.ends_with('\n') {
            source.push('\n');
        }
        source
            .push_str("throw new InvalidOperationException(\"Unreachable Neo VM fallthrough.\");");
    }
    source
}

pub(super) fn prepend_argument_underflow_comment(
    source: String,
    method_plan: &CSharpMethodPlan,
    fidelity: &FidelityReport,
) -> String {
    let Some(issue) = fidelity.issues.iter().find(|issue| {
        issue.kind == LoweringIssueKind::LostStackValue
            && matches!(
                issue.opcode,
                OpCode::Call | OpCode::Call_L | OpCode::CallA | OpCode::Jmp | OpCode::Jmp_L
            )
    }) else {
        return source;
    };
    format!(
        "// VM argument underflow in {} at 0x{:04X}: {}; missing values are rendered as (dynamic)null.\n{}",
        method_plan.emitted_name, issue.offset, issue.detail, source
    )
}

#[cfg(test)]
mod tests {
    use super::render_placeholder_arguments;

    #[test]
    fn compatibility_recovery_does_not_invent_zero_argument_placeholders() {
        assert_eq!(render_placeholder_arguments(0), "");
        assert_eq!(
            render_placeholder_arguments(2),
            "(dynamic)null, (dynamic)null"
        );
    }

    #[test]
    fn compatibility_recovery_bounds_large_placeholder_argument_lists() {
        let rendered = render_placeholder_arguments(300);
        assert_eq!(rendered.matches("(dynamic)null").count(), 257);
        assert!(rendered.contains("omitted 44 additional VM arguments"));
    }
}
