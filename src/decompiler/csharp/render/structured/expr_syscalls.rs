use std::collections::BTreeSet;

use crate::decompiler::analysis::types::ValueType;
use crate::decompiler::ir::{Expr, Literal};
use crate::decompiler::syscall_types;
use crate::instruction::OpCode;

use super::expr::{
    render_expr_list, render_expr_prec, ExprContext, RenderedExpr, PREC_PRIMARY, PREC_UNARY,
};

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

#[derive(Clone, Copy)]
enum SyscallApi {
    StaticMethod {
        api: &'static str,
        arguments: &'static [SyscallArgument],
    },
    StaticProperty(&'static str),
    InstanceMethod {
        receiver_type: &'static str,
        method: &'static str,
        arguments: &'static [SyscallArgument],
    },
    InstanceProperty {
        receiver_type: &'static str,
        property: &'static str,
    },
    LowLevel,
}

#[derive(Clone, Copy)]
enum SyscallArgument {
    Cast(&'static str),
    Int,
    LongInteger,
    Enum(&'static str),
    StorageKey,
    StorageValue,
    Witness,
}

fn known_syscall_api(hash: u32) -> Option<SyscallApi> {
    Some(match hash {
        0x0287_99CF => SyscallApi::StaticMethod {
            api: "Contract.CreateStandardAccount",
            arguments: &[SyscallArgument::Cast("ECPoint")],
        },
        0x0388_C3B7 => SyscallApi::StaticProperty("Runtime.Time"),
        0x09E9_336A => SyscallApi::StaticMethod {
            api: "Contract.CreateMultisigAccount",
            arguments: &[SyscallArgument::Int, SyscallArgument::Cast("ECPoint[]")],
        },
        0x0AE3_0C39 => SyscallApi::StaticMethod {
            api: "Storage.Put",
            arguments: &[SyscallArgument::StorageKey, SyscallArgument::StorageValue],
        },
        0x165D_A144 => SyscallApi::LowLevel,
        0x1DBF_54F3 => SyscallApi::InstanceProperty {
            receiver_type: "Iterator",
            property: "Value",
        },
        0x27B3_E756 => SyscallApi::StaticMethod {
            api: "Crypto.CheckSig",
            arguments: &[
                SyscallArgument::Cast("ECPoint"),
                SyscallArgument::Cast("ByteString"),
            ],
        },
        0x28A9_DE6B => SyscallApi::StaticMethod {
            api: "Runtime.GetRandom",
            arguments: &[],
        },
        0x3008_512D => SyscallApi::StaticProperty("Runtime.Transaction"),
        0x31E8_5D92 => SyscallApi::StaticMethod {
            api: "Storage.Get",
            arguments: &[
                SyscallArgument::Cast("StorageContext"),
                SyscallArgument::StorageKey,
            ],
        },
        0x38E2_B4F9 => SyscallApi::StaticProperty("Runtime.EntryScriptHash"),
        0x3ADC_D09E => SyscallApi::StaticMethod {
            api: "Crypto.CheckMultisig",
            arguments: &[
                SyscallArgument::Cast("ECPoint[]"),
                SyscallArgument::Cast("ByteString[]"),
            ],
        },
        0x3C6E_5339 => SyscallApi::StaticProperty("Runtime.CallingScriptHash"),
        0x4311_2784 => SyscallApi::StaticProperty("Runtime.InvocationCounter"),
        0x525B_7D62 => SyscallApi::StaticMethod {
            api: "Contract.Call",
            arguments: &[
                SyscallArgument::Cast("UInt160"),
                SyscallArgument::Cast("string"),
                SyscallArgument::Enum("CallFlags"),
                SyscallArgument::Cast("object[]"),
            ],
        },
        0x616F_0195 => SyscallApi::LowLevel,
        0x677B_F71A => SyscallApi::LowLevel,
        0x74A8_FEDB => SyscallApi::StaticProperty("Runtime.ExecutingScriptHash"),
        0x813A_DA95 => SyscallApi::StaticMethod {
            api: "Contract.GetCallFlags",
            arguments: &[],
        },
        0x8418_3FE6 => SyscallApi::StaticMethod {
            api: "Storage.Put",
            arguments: &[
                SyscallArgument::Cast("StorageContext"),
                SyscallArgument::StorageKey,
                SyscallArgument::StorageValue,
            ],
        },
        0x8B18_F1AC => SyscallApi::StaticMethod {
            api: "Runtime.CurrentSigners",
            arguments: &[],
        },
        0x8CEC_27F8 => SyscallApi::StaticMethod {
            api: "Runtime.CheckWitness",
            arguments: &[SyscallArgument::Witness],
        },
        0x8F80_0CB3 => SyscallApi::StaticMethod {
            api: "Runtime.LoadScript",
            arguments: &[
                SyscallArgument::Cast("ByteString"),
                SyscallArgument::Enum("CallFlags"),
                SyscallArgument::Cast("object[]"),
            ],
        },
        0x93BC_DB2E => SyscallApi::LowLevel,
        0x94F5_5475 => SyscallApi::StaticMethod {
            api: "Storage.Delete",
            arguments: &[SyscallArgument::StorageKey],
        },
        0x9647_E7CF => SyscallApi::StaticMethod {
            api: "Runtime.Log",
            arguments: &[SyscallArgument::Cast("string")],
        },
        0x9AB8_30DF => SyscallApi::StaticMethod {
            api: "Storage.Find",
            arguments: &[
                SyscallArgument::Cast("StorageContext"),
                SyscallArgument::StorageKey,
                SyscallArgument::Enum("FindOptions"),
            ],
        },
        0x9CED_089C => SyscallApi::InstanceMethod {
            receiver_type: "Iterator",
            method: "Next",
            arguments: &[],
        },
        0xA038_7DE9 => SyscallApi::StaticProperty("Runtime.Trigger"),
        0xBC8C_5AC3 => SyscallApi::StaticMethod {
            api: "Runtime.BurnGas",
            arguments: &[SyscallArgument::LongInteger],
        },
        0xCE67_F69B => SyscallApi::StaticProperty("Storage.CurrentContext"),
        0xCED8_8814 => SyscallApi::StaticProperty("Runtime.GasLeft"),
        0xDC92_494C => SyscallApi::StaticProperty("Runtime.AddressVersion"),
        0xE0A0_FBC5 => SyscallApi::StaticMethod {
            api: "Runtime.GetNetwork",
            arguments: &[],
        },
        0xE26B_B4F6 => SyscallApi::StaticProperty("Storage.CurrentReadOnlyContext"),
        0xE85E_8DD5 => SyscallApi::StaticMethod {
            api: "Storage.Get",
            arguments: &[SyscallArgument::StorageKey],
        },
        0xE9BF_4C76 => SyscallApi::InstanceProperty {
            receiver_type: "StorageContext",
            property: "AsReadOnly",
        },
        0xEDC5_582F => SyscallApi::StaticMethod {
            api: "Storage.Delete",
            arguments: &[
                SyscallArgument::Cast("StorageContext"),
                SyscallArgument::StorageKey,
            ],
        },
        0xF135_4327 => SyscallApi::StaticMethod {
            api: "Runtime.GetNotifications",
            arguments: &[SyscallArgument::Cast("UInt160")],
        },
        0xF352_7607 => SyscallApi::StaticMethod {
            api: "Storage.Find",
            arguments: &[
                SyscallArgument::StorageKey,
                SyscallArgument::Enum("FindOptions"),
            ],
        },
        0xF6FC_79B2 => SyscallApi::StaticProperty("Runtime.Platform"),
        _ => return None,
    })
}

#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn known_syscall_is_classified(hash: u32) -> bool {
    known_syscall_api(hash).is_some()
}
