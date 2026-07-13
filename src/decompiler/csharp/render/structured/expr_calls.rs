//! Semantic call-target rendering for structured C# expressions.

use std::collections::BTreeSet;

use crate::decompiler::ir::{Expr, Intrinsic, SemanticCallTarget};
use crate::native_contracts;

use super::expr::{
    escape_csharp_string, render_expr_list, ExprContext, RenderedExpr, PREC_PRIMARY,
};
use super::expr_intrinsics::render_intrinsic;
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

fn render_method_token_call(
    index: usize,
    name: &str,
    hash_le: Option<&str>,
    call_flags: Option<u8>,
    args: &[Expr],
    context: &ExprContext,
    expanding: &mut BTreeSet<String>,
) -> RenderedExpr {
    let bytes = hash_le.and_then(|hash| {
        (hash.len() == 40)
            .then(|| {
                hash.as_bytes()
                    .chunks_exact(2)
                    .map(|pair| {
                        std::str::from_utf8(pair)
                            .ok()
                            .and_then(|pair| u8::from_str_radix(pair, 16).ok())
                    })
                    .collect::<Option<Vec<_>>>()
            })
            .flatten()
    });
    let (Some(bytes), Some(call_flags)) = (bytes, call_flags) else {
        return RenderedExpr::new(
            format!(
                "__NeoDecompilerUnresolvedCall(\"method token {index}: {}\", new object[] {{ {} }})",
                escape_csharp_string(name),
                render_expr_list(args, context, expanding)
            ),
            PREC_PRIMARY,
        );
    };
    let native_hash = (bytes.len() == 20).then(|| {
        let mut hash = [0u8; 20];
        hash.copy_from_slice(&bytes);
        hash
    });
    if let Some(hint) = native_hash
        .as_ref()
        .and_then(|hash| native_contracts::describe_method_token(hash, name))
        .filter(|hint| hint.has_exact_method() && call_flags == 0x0F)
    {
        let method = hint
            .canonical_method
            .expect("exact native method hint has a canonical name");
        return RenderedExpr::new(
            format!(
                "{}.{method}({})",
                hint.contract,
                render_expr_list(args, context, expanding)
            ),
            PREC_PRIMARY,
        );
    }
    let bytes = bytes
        .iter()
        .map(|byte| format!("0x{byte:02X}"))
        .collect::<Vec<_>>()
        .join(", ");
    RenderedExpr::new(
        format!(
            "(dynamic)Contract.Call((UInt160)new byte[] {{ {bytes} }}, \"{}\", (CallFlags)0x{call_flags:02X}, new object[] {{ {} }})",
            escape_csharp_string(name),
            render_expr_list(args, context, expanding)
        ),
        PREC_PRIMARY,
    )
}
