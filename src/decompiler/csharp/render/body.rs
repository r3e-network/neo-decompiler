use std::collections::BTreeMap;

use crate::decompiler::analysis::method_contracts::ReturnBehavior;
use crate::decompiler::cfg::method_body::{
    lower_method_body, Fidelity, FidelityReport, LoweringIssueKind, MethodIrRequest,
};
use crate::instruction::Instruction;

use super::super::helpers::VM_ASSERT_MESSAGE_HELPER;
use super::structured::plan::{
    plan_declarations_with_known_types_and_calls, CSharpMethodPlan, DeclarationPlan,
};
use super::structured::stmt;

mod fidelity;
mod recovery;
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
        reduce_temps: true,
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
        return recovery::recovered_result(
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

    let underflow_targets = recovery::underflow_call_targets(instructions, context, &fidelity);
    let underflow_placeholder =
        recovery::underflow_placeholder(method_plan, &fidelity, &underflow_targets);
    let source = stmt::render_block_with_trace_and_underflow(
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
        &underflow_targets,
        underflow_placeholder.as_deref(),
    );
    let source =
        recovery::ensure_non_void_termination(source, &lowered.body, method_plan.return_behavior);
    let source = recovery::prepend_argument_underflow_comment(source, method_plan, &fidelity);
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
        source: recovery::indent_body(&source),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decompiler::cfg::method_body::{LoweringIssue, LoweringIssueKind};
    use crate::decompiler::ir::{Block, Expr, Stmt};
    use crate::instruction::OpCode;

    #[test]
    fn long_relative_call_underflow_uses_compatibility_recovery() {
        let mut fidelity = FidelityReport::exact(1);
        fidelity.issues.push(LoweringIssue {
            offset: 0x20,
            opcode: OpCode::Call_L,
            kind: LoweringIssueKind::LostStackValue,
            fidelity: Fidelity::Incomplete,
            detail: "requires 2 stack values, but only 0 are available".to_string(),
        });
        fidelity.finish();

        assert!(recover_with_compatibility(&fidelity));
    }

    #[test]
    fn non_void_partial_body_gets_fail_closed_fallthrough() {
        let body = Block::with_stmts(vec![Stmt::expr(Expr::int(1))]);

        assert_eq!(
            recovery::ensure_non_void_termination(
                "_ = 1;".to_string(),
                &body,
                ReturnBehavior::Value
            ),
            "_ = 1;\nthrow new InvalidOperationException(\"Unreachable Neo VM fallthrough.\");"
        );
        assert_eq!(
            recovery::ensure_non_void_termination(
                "_ = 1;".to_string(),
                &body,
                ReturnBehavior::Void
            ),
            "_ = 1;"
        );
    }
}
