//! Semantic call-target rendering for structured C# expressions.

use std::collections::BTreeSet;

use crate::decompiler::ir::{Expr, Intrinsic, SemanticCallTarget};

use super::expr::{
    escape_csharp_string, render_expr_list, ExprContext, RenderedExpr, PREC_PRIMARY,
};
use super::expr_intrinsics::render_intrinsic;
use super::expr_native::render_method_token_call;
use super::expr_syscalls::render_syscall;

pub(super) fn render_call(
    target: &SemanticCallTarget,
    args: &[Expr],
    context: &ExprContext,
    expanding: &mut BTreeSet<String>,
) -> RenderedExpr {
    match target {
        SemanticCallTarget::Internal { name, .. } => RenderedExpr::new(
            format!("{name}({})", render_expr_list(args, context, expanding)),
            PREC_PRIMARY,
        ),
        SemanticCallTarget::MethodToken {
            index,
            name,
            hash_le,
            call_flags,
        } => render_method_token_call(
            *index,
            name,
            hash_le.as_deref(),
            *call_flags,
            args,
            context,
            expanding,
        ),
        SemanticCallTarget::Unresolved { display_name } => RenderedExpr::new(
            format!(
                "__NeoDecompilerUnresolvedCall(\"{}\", new object[] {{ {} }})",
                escape_csharp_string(display_name),
                render_expr_list(args, context, expanding)
            ),
            PREC_PRIMARY,
        ),
        SemanticCallTarget::Syscall { hash, .. } => render_syscall(*hash, args, context, expanding),
        SemanticCallTarget::Intrinsic(Intrinsic::Opcode(opcode)) => {
            render_intrinsic(*opcode, args, context, expanding)
        }
        SemanticCallTarget::Intrinsic(Intrinsic::UnpackPackStruct) => {
            let helper = context
                .unpack_packstruct_helper_call
                .as_deref()
                .unwrap_or(super::super::UNPACK_PACKSTRUCT_HELPER);
            RenderedExpr::new(
                format!("{helper}({})", render_expr_list(args, context, expanding)),
                PREC_PRIMARY,
            )
        }
    }
}
