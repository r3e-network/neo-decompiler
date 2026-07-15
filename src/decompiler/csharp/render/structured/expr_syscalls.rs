use std::collections::BTreeSet;

use crate::decompiler::analysis::types::ValueType;
use crate::decompiler::ir::{Expr, Literal};
use crate::decompiler::syscall_types;
use crate::instruction::OpCode;

use super::expr::{
    render_expr_list, render_expr_prec, ExprContext, RenderedExpr, PREC_PRIMARY, PREC_UNARY,
};
#[path = "expr_syscalls_catalog.rs"]
mod catalog;
use catalog::{known_syscall_api, SyscallApi, SyscallArgument};

pub(super) fn render_syscall(
    hash: u32,
    args: &[Expr],
    context: &ExprContext,
    expanding: &mut BTreeSet<String>,
) -> RenderedExpr {
    let args = syscall_arguments(hash, args);
    if hash == 0x616F_0195 {
        if let [Expr::Literal(Literal::String(label)), state] = args {
            if label == "Debug" {
                if let Some(message) = context
                    .singleton_array_element(state)
                    .filter(|message| context.value_type(message) == ValueType::ByteString)
                {
                    return RenderedExpr::new(
                        format!(
                            "Runtime.Debug({})",
                            render_expr_prec(message, 0, context, expanding)
                        ),
                        PREC_PRIMARY,
                    );
                }
            }
            if let Some((event_name, parameter_types)) = context.event_signature(label) {
                if let Some(elements) = context.array_elements(state) {
                    if elements.len() == parameter_types.len() {
                        let rendered = elements
                            .iter()
                            .zip(parameter_types)
                            .map(|(expression, expected)| {
                                render_event_argument(expression, expected, context, expanding)
                            })
                            .collect::<Vec<_>>()
                            .join(", ");
                        return RenderedExpr::new(
                            format!("{event_name}({rendered})"),
                            PREC_PRIMARY,
                        );
                    }
                }
            }
        }
    }
    match known_syscall_api(hash) {
        Some(SyscallApi::StaticMethod { api, arguments }) => {
            if let Some(arguments) = render_syscall_arguments(args, arguments, context, expanding) {
                return RenderedExpr::new(format!("{api}({arguments})"), PREC_PRIMARY);
            }
        }
        Some(SyscallApi::StaticProperty(api)) => {
            if args.is_empty() {
                return RenderedExpr::new(api, PREC_PRIMARY);
            }
        }
        Some(SyscallApi::InstanceMethod {
            receiver_type,
            method,
            arguments,
        }) => {
            let Some((receiver, rest)) = args.split_first() else {
                return render_low_level_syscall(hash, args, context, expanding);
            };
            if let Some(arguments) = render_syscall_arguments(rest, arguments, context, expanding) {
                let receiver = render_typed_receiver(receiver, receiver_type, context, expanding);
                return RenderedExpr::new(
                    format!("{receiver}.{method}({arguments})"),
                    PREC_PRIMARY,
                );
            }
        }
        Some(SyscallApi::InstanceProperty {
            receiver_type,
            property,
        }) => {
            if let [receiver] = args {
                let receiver = render_typed_receiver(receiver, receiver_type, context, expanding);
                return RenderedExpr::new(format!("{receiver}.{property}"), PREC_PRIMARY);
            }
        }
        Some(SyscallApi::LowLevel) | None => {}
    }

    let rendered = render_low_level_syscall(hash, args, context, expanding);
    if let Some(return_type) = syscall_types::lookup(hash) {
        RenderedExpr::new(
            format!("({}){}", return_type.csharp_type, rendered.source),
            PREC_UNARY,
        )
    } else {
        rendered
    }
}

fn render_event_argument(
    expression: &Expr,
    expected_type: &str,
    context: &ExprContext,
    expanding: &mut BTreeSet<String>,
) -> String {
    let rendered = render_expr_prec(expression, 0, context, expanding);
    match (expected_type, context.value_type(expression)) {
        ("ByteString", ValueType::Buffer | ValueType::Integer) => {
            format!("(ByteString)({rendered})")
        }
        ("BigInteger", ValueType::ByteString) => format!("(BigInteger)({rendered})"),
        ("byte[]", ValueType::ByteString) => format!("(byte[])({rendered})"),
        ("UInt160" | "UInt256" | "ECPoint", ValueType::ByteString) => {
            format!("({expected_type})(byte[])({rendered})")
        }
        _ => rendered,
    }
}

fn render_syscall_arguments(
    expressions: &[Expr],
    arguments: &[SyscallArgument],
    context: &ExprContext,
    expanding: &mut BTreeSet<String>,
) -> Option<String> {
    if expressions.len() != arguments.len() {
        return None;
    }

    expressions
        .iter()
        .zip(arguments)
        .map(|(expression, argument)| {
            let rendered = render_expr_prec(expression, 0, context, expanding);
            Some(match argument {
                SyscallArgument::Cast(target_type) => {
                    format!("({target_type})({rendered})")
                }
                SyscallArgument::Int => format!("(int)({rendered})"),
                SyscallArgument::LongInteger => {
                    format!("(long)(BigInteger)({rendered})")
                }
                SyscallArgument::Enum(target_type) => {
                    format!("({target_type})(int)({rendered})")
                }
                SyscallArgument::StorageKey => match context.value_type(expression) {
                    ValueType::Buffer => format!("(byte[])({rendered})"),
                    ValueType::ByteString => format!("(ByteString)({rendered})"),
                    _ => return None,
                },
                SyscallArgument::StorageValue => match context.value_type(expression) {
                    ValueType::Integer => format!("(BigInteger)({rendered})"),
                    ValueType::Buffer => format!("(byte[])({rendered})"),
                    ValueType::ByteString => format!("(ByteString)({rendered})"),
                    _ => return None,
                },
                SyscallArgument::Witness => match expression {
                    Expr::Cast { target_type, .. }
                        if matches!(target_type.as_str(), "UInt160" | "ECPoint") =>
                    {
                        rendered
                    }
                    _ => return None,
                },
            })
        })
        .collect::<Option<Vec<_>>>()
        .map(|arguments| arguments.join(", "))
}

fn render_typed_receiver(
    expression: &Expr,
    target_type: &str,
    context: &ExprContext,
    expanding: &mut BTreeSet<String>,
) -> String {
    format!(
        "(({target_type}){})",
        render_expr_prec(expression, PREC_UNARY, context, expanding)
    )
}

fn render_low_level_syscall(
    hash: u32,
    args: &[Expr],
    context: &ExprContext,
    expanding: &mut BTreeSet<String>,
) -> RenderedExpr {
    let bytes = std::iter::once(OpCode::Syscall.byte())
        .chain(hash.to_le_bytes())
        .map(|byte| format!("0x{byte:02X}"))
        .collect::<Vec<_>>()
        .join(", ");
    RenderedExpr::new(
        format!(
            "Runtime.LoadScript((ByteString)new byte[] {{ {bytes} }}, CallFlags.All, new object[] {{ {} }})",
            render_expr_list(args, context, expanding)
        ),
        PREC_PRIMARY,
    )
}

fn syscall_arguments(hash: u32, args: &[Expr]) -> &[Expr] {
    let Some(Expr::Literal(Literal::String(metadata))) = args.first() else {
        return args;
    };
    let catalog = crate::syscalls::lookup(hash);
    let has_catalog_selector = catalog.is_some_and(|info| {
        args.len() == usize::from(info.param_count) + 1
            && (metadata == info.name || metadata == &format!("0x{hash:08X}"))
    });
    let has_unknown_selector = catalog.is_none() && metadata == &format!("0x{hash:08X}");
    if has_catalog_selector || has_unknown_selector {
        &args[1..]
    } else {
        args
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn known_syscall_is_classified(hash: u32) -> bool {
    known_syscall_api(hash).is_some()
}
