use std::collections::BTreeMap;
use std::fmt::Write;

use crate::decompiler::analysis::method_contracts::ReturnBehavior;
use crate::decompiler::cfg::method_body::{
    lower_method_body, Fidelity, FidelityReport, LoweringIssueKind, MethodIrRequest,
};
use crate::instruction::Instruction;
use crate::instruction::OpCode;
use crate::instruction::Operand;

use super::super::helpers::VM_ASSERT_MESSAGE_HELPER;
use super::structured::plan::{
    plan_declarations_with_known_types_and_calls, CSharpMethodPlan, DeclarationPlan,
};
use super::structured::stmt;

mod fidelity;
use fidelity::{
    fidelity_issue, recover_with_compatibility, requires_structured_stub, semantic_warnings,
    throwing_stub, throwing_stub_with_fidelity,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BodyBackend {
    Structured,
    ThrowingStub,
}

pub(super) struct BodyRenderResult {
    pub(super) source: String,
    pub(super) backend: BodyBackend,
    pub(super) fidelity: FidelityReport,
    pub(super) warnings: Vec<String>,
}

pub(super) struct LiftedBodyContext<'a> {
    pub(super) method_labels_by_offset: &'a BTreeMap<usize, String>,
    pub(super) method_arg_counts_by_offset: &'a BTreeMap<usize, usize>,
    pub(super) method_return_types_by_offset: &'a BTreeMap<usize, String>,
    pub(super) inline_single_use_temps: bool,
    pub(super) emit_trace_comments: bool,
    /// When true, body-local declarations use inferred C# types (`BigInteger
    /// loc0 = ...;`) instead of `var`. Off by default to preserve historical
    /// output.
    pub(super) typed_declarations: bool,
    pub(super) vm_exception_type: &'a str,
    /// Inferred C# slot types keyed by method start offset. Assertion rendering
    /// uses these even when typed local declarations are disabled.
    pub(super) assert_message_helper_call: Option<&'a str>,
    pub(super) bare_throw_helper_call: Option<&'a str>,
    pub(super) unpack_packstruct_helper_call: Option<&'a str>,
    pub(super) tagged_opcode_helper_calls: &'a BTreeMap<(u8, u8), String>,
    pub(super) static_field_types: &'a BTreeMap<String, String>,
    pub(super) event_signatures: &'a super::events::EventSignatures,
}

pub(super) fn render_method_body(
    instructions: &[Instruction],
    method_plan: &CSharpMethodPlan,
    context: &LiftedBodyContext<'_>,
) -> BodyRenderResult {
    if instructions.is_empty() {
        return throwing_stub(
            method_plan,
            fidelity_issue(
                method_plan.start,
                LoweringIssueKind::LostStackValue,
                "method has no decoded instructions",
            ),
        );
    }

    let lowered = lower_method_body(MethodIrRequest {
        start: method_plan.start,
        end: method_plan.end,
        instructions,
        context: method_plan.method_context.clone(),
        symbol_types: method_plan.symbol_types.clone(),
    });
    let mut fidelity = lowered.fidelity;
    fidelity
        .issues
        .extend(method_plan.planning_issues.iter().cloned());
    fidelity.finish();

    if fidelity
        .issues
        .iter()
        .any(|issue| issue.kind == LoweringIssueKind::BudgetExceeded)
    {
        return throwing_stub_with_fidelity(method_plan, fidelity);
    }

    if fidelity.status == Fidelity::Incomplete && recover_with_compatibility(&fidelity) {
        return recovered_result(
            instructions,
            method_plan,
            context,
            &lowered.body,
            &lowered.symbols,
            fidelity,
        );
    }
    if fidelity.status == Fidelity::Incomplete && requires_structured_stub(&fidelity) {
        return throwing_stub_with_fidelity(method_plan, fidelity);
    }

    let declarations =
        plan_method_declarations(&lowered.body, &lowered.symbols, method_plan, context);
    fidelity.issues.extend(declarations.issues.iter().cloned());
    fidelity.finish();
    if fidelity.status == Fidelity::Incomplete && requires_structured_stub(&fidelity) {
        return throwing_stub_with_fidelity(method_plan, fidelity);
    }

    let source = stmt::render_block_with_trace(
        &lowered.body,
        &declarations,
        &lowered.symbols,
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
        context.emit_trace_comments.then_some(&lowered.source_map),
        instructions,
        context.event_signatures,
    );
    let source = ensure_non_void_termination(source, &lowered.body, method_plan.return_behavior);
    let source = prepend_argument_underflow_comment(source, method_plan, &fidelity);
    if source.trim().is_empty() && method_plan.return_behavior != ReturnBehavior::Void {
        fidelity.issues.push(fidelity_issue(
            method_plan.start,
            LoweringIssueKind::LostStackValue,
            "structured non-void body produced no return",
        ));
        fidelity.finish();
        return throwing_stub_with_fidelity(method_plan, fidelity);
    }

    BodyRenderResult {
        source: indent_body(&source),
        backend: BodyBackend::Structured,
        warnings: semantic_warnings(method_plan, &fidelity),
        fidelity,
    }
}

fn recovered_result(
    instructions: &[Instruction],
    method_plan: &CSharpMethodPlan,
    context: &LiftedBodyContext<'_>,
    body: &crate::decompiler::ir::Block,
    symbols: &BTreeMap<String, crate::decompiler::cfg::method_body::SymbolInfo>,
    fidelity: FidelityReport,
) -> BodyRenderResult {
    let mut source = String::new();
    if body.stmts.iter().any(|statement| {
        !matches!(
            statement,
            crate::decompiler::ir::Stmt::Comment(_) | crate::decompiler::ir::Stmt::Return(None)
        )
    }) {
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
        let args = if argument_underflow || argument_count > 0 {
            std::iter::repeat_n("(dynamic)null", argument_count.max(1))
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            String::new()
        };
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

fn plan_method_declarations(
    body: &crate::decompiler::ir::Block,
    symbols: &BTreeMap<String, crate::decompiler::cfg::method_body::SymbolInfo>,
    method_plan: &CSharpMethodPlan,
    context: &LiftedBodyContext<'_>,
) -> DeclarationPlan {
    let parameter_types = method_plan
        .parameters
        .iter()
        .filter(|parameter| {
            !parameter.ty.eq_ignore_ascii_case("dynamic")
                && !parameter.ty.eq_ignore_ascii_case("object")
        })
        .map(|parameter| (parameter.name.clone(), parameter.ty.clone()))
        .collect::<BTreeMap<_, _>>();
    plan_declarations_with_known_types_and_calls(
        body,
        symbols,
        context.typed_declarations,
        &parameter_types,
        context.method_return_types_by_offset,
    )
    .with_static_field_types(context.static_field_types)
}

fn indent_body(source: &str) -> String {
    let mut indented = String::new();
    for line in source.lines() {
        writeln!(indented, "            {line}").unwrap();
    }
    indented
}

fn ensure_non_void_termination(
    mut source: String,
    body: &crate::decompiler::ir::Block,
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

fn prepend_argument_underflow_comment(
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
    use super::*;
    use crate::decompiler::ir::{Block, Expr, Stmt};

    #[test]
    fn non_void_partial_body_gets_fail_closed_fallthrough() {
        let body = Block::with_stmts(vec![Stmt::expr(Expr::int(1))]);

        assert_eq!(
            ensure_non_void_termination("_ = 1;".to_string(), &body, ReturnBehavior::Value),
            "_ = 1;\nthrow new InvalidOperationException(\"Unreachable Neo VM fallthrough.\");"
        );
        assert_eq!(
            ensure_non_void_termination("_ = 1;".to_string(), &body, ReturnBehavior::Void),
            "_ = 1;"
        );
    }
}
