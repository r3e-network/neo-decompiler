//! C# rendering for resolved native-contract method tokens.

use std::collections::BTreeSet;

use crate::decompiler::analysis::types::ValueType;
use crate::decompiler::ir::Expr;
use crate::native_contracts;

use super::expr::{
    escape_csharp_string, int_cast, render_expr_list, render_expr_prec, ExprContext, RenderedExpr,
    PREC_PRIMARY,
};
use super::native_framework;

/// Render a method-token call, using framework native APIs when the token is
/// fully resolved and unrestricted. Unknown or restricted tokens stay dynamic.
pub(super) fn render_method_token_call(
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
        .filter(|hint| {
            hint.has_exact_method()
                && call_flags == 0x0F
                && hint.canonical_method.is_some_and(|method| {
                    native_framework::method_name(hint.contract, method).is_some()
                })
        })
    {
        let method = hint
            .canonical_method
            .expect("exact native method hint has a canonical name");
        let framework_method = native_framework::method_name(hint.contract, method)
            .expect("supported native method has a framework spelling");
        let rendered_args = render_native_args(hint.contract, method, args, context, expanding);
        let call = if args.is_empty() && is_native_property(hint.contract, method) {
            format!("{}.{framework_method}", hint.contract)
        } else {
            format!("{}.{framework_method}({rendered_args})", hint.contract)
        };
        return RenderedExpr::new(call, PREC_PRIMARY);
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

fn render_native_args(
    contract: &str,
    method: &str,
    args: &[Expr],
    context: &ExprContext,
    expanding: &mut BTreeSet<String>,
) -> String {
    args.iter()
        .enumerate()
        .map(|(index, expression)| {
            if contract == "RoleManagement" && method == "GetDesignatedByRole" && index == 0 {
                let rendered = render_expr_prec(expression, 0, context, expanding);
                if context.exact_csharp_type(expression) == Some("Role") {
                    rendered
                } else {
                    format!("(Role)(int)({rendered})")
                }
            } else if contract == "StdLib"
                && method == "MemorySearch"
                && index == 1
                && context.value_type(expression) == ValueType::Integer
            {
                format!(
                    "(ByteString)({})",
                    render_expr_prec(expression, 0, context, expanding)
                )
            } else if contract == "StdLib" && method == "MemorySearch" && index == 2 {
                int_cast(expression, context, expanding)
            } else if contract == "PolicyContract"
                && matches!(method, "GetAttributeFee" | "getAttributeFee")
                && index == 0
            {
                let rendered = render_expr_prec(expression, 0, context, expanding);
                if context.exact_csharp_type(expression) == Some("TransactionAttributeType") {
                    rendered
                } else {
                    format!("(TransactionAttributeType)(int)({rendered})")
                }
            } else if contract == "CryptoLib"
                && matches!(method, "VerifyWithECDsa" | "verifyWithECDsa")
                && index == 3
            {
                let rendered = render_expr_prec(expression, 0, context, expanding);
                if context.exact_csharp_type(expression) == Some("NamedCurveHash") {
                    rendered
                } else {
                    format!("(NamedCurveHash)(int)({rendered})")
                }
            } else {
                render_expr_prec(expression, 0, context, expanding)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn is_native_property(contract: &str, method: &str) -> bool {
    matches!(
        (contract, method),
        ("GasToken" | "NeoToken", "Symbol" | "Decimals")
            | ("LedgerContract", "CurrentHash" | "CurrentIndex")
    )
}
