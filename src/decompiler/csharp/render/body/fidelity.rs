use crate::decompiler::cfg::method_body::{
    Fidelity, FidelityReport, LoweringIssue, LoweringIssueKind,
};
use crate::instruction::OpCode;

use super::{BodyRenderResult, CSharpMethodPlan};

pub(super) fn semantic_warnings(
    method_plan: &CSharpMethodPlan,
    fidelity: &FidelityReport,
) -> Vec<String> {
    fidelity
        .issues
        .iter()
        .filter(|issue| {
            issue.fidelity == Fidelity::Conservative
                && matches!(issue.opcode, OpCode::Abort | OpCode::Abortmsg)
        })
        .map(|issue| {
            format!(
                "csharp: {} at 0x{:04X}: {} at 0x{:04X}: {}",
                method_plan.emitted_name,
                method_plan.start,
                issue.opcode.mnemonic(),
                issue.offset,
                issue.detail
            )
        })
        .collect()
}

pub(super) fn requires_structured_stub(fidelity: &FidelityReport) -> bool {
    fidelity
        .issues
        .iter()
        .filter(|issue| issue.fidelity == Fidelity::Incomplete)
        .any(|issue| match issue.kind {
            LoweringIssueKind::LostStackValue | LoweringIssueKind::UnresolvedCall => false,
            LoweringIssueKind::MissingProvenance => {
                !(issue.detail.starts_with("collection packing requires")
                    || issue
                        .detail
                        .starts_with("collection packing has fewer values")
                    || issue
                        .detail
                        .starts_with("collection element count overflows")
                    || issue.detail.starts_with("UNPACK source")
                    || issue.detail.starts_with("UNPACK element count exceeds"))
            }
            LoweringIssueKind::MissingOperandMetadata => {
                !issue.detail.starts_with("StackItemType Any is invalid")
            }
            LoweringIssueKind::UnsupportedControl => !matches!(issue.opcode, OpCode::Jmp),
            _ => true,
        })
}

pub(super) fn recover_with_compatibility(fidelity: &FidelityReport) -> bool {
    fidelity.issues.iter().any(|issue| {
        issue.fidelity == Fidelity::Incomplete
            && ((issue.detail.starts_with("requires ")
                && matches!(issue.opcode, OpCode::Call | OpCode::CallA))
                || issue
                    .detail
                    .starts_with("structured output contains an unresolved control transfer")
                || matches!(issue.opcode, OpCode::Endtry | OpCode::EndtryL)
                || (issue.kind == LoweringIssueKind::LostStackValue
                    && matches!(issue.opcode, OpCode::Ret)))
    })
}

pub(super) fn throwing_stub(
    method_plan: &CSharpMethodPlan,
    issue: LoweringIssue,
) -> BodyRenderResult {
    let mut fidelity = FidelityReport::exact(0);
    fidelity.issues.push(issue);
    fidelity.finish();
    throwing_stub_with_fidelity(method_plan, fidelity)
}

pub(super) fn throwing_stub_with_fidelity(
    method_plan: &CSharpMethodPlan,
    fidelity: FidelityReport,
) -> BodyRenderResult {
    let warnings = fidelity
        .primary_issue()
        .map(|issue| {
            format!(
                "csharp: {} at 0x{:04X} used throwing stub: {} at 0x{:04X}: {}",
                method_plan.emitted_name,
                method_plan.start,
                issue.opcode.mnemonic(),
                issue.offset,
                issue_kind_label(issue.kind)
            )
        })
        .into_iter()
        .collect();
    BodyRenderResult {
        source: "            throw new NotImplementedException();\n".to_string(),
        backend: super::BodyBackend::ThrowingStub,
        fidelity,
        warnings,
    }
}

pub(super) fn fidelity_issue(
    offset: usize,
    kind: LoweringIssueKind,
    detail: &str,
) -> LoweringIssue {
    LoweringIssue {
        offset,
        opcode: OpCode::Unknown(0),
        kind,
        fidelity: Fidelity::Incomplete,
        detail: detail.to_string(),
    }
}

fn issue_kind_label(kind: LoweringIssueKind) -> &'static str {
    match kind {
        LoweringIssueKind::UnsupportedControl => "unsupported control",
        LoweringIssueKind::UnsupportedOpcode => "unsupported opcode",
        LoweringIssueKind::LostStackValue => "lost stack value",
        LoweringIssueKind::MissingOperandMetadata => "missing operand metadata",
        LoweringIssueKind::UnresolvedCall => "unresolved call",
        LoweringIssueKind::MissingProvenance => "missing provenance",
        LoweringIssueKind::BudgetExceeded => "budget exceeded",
    }
}
