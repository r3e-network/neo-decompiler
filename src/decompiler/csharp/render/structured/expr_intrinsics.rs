use std::collections::BTreeSet;

use crate::decompiler::analysis::types::ValueType;
use crate::decompiler::ir::Expr;
use crate::instruction::OpCode;

use super::expr::{
    int_cast, render_expr_list, render_expr_prec, ExprContext, RenderedExpr, PREC_ASSIGNMENT,
    PREC_EQUALITY, PREC_PRIMARY, PREC_RELATIONAL, PREC_UNARY,
};
use super::expr_low_level::render_low_level_opcode;

pub(super) fn render_intrinsic(
    opcode: OpCode,
    args: &[Expr],
    context: &ExprContext,
    expanding: &mut BTreeSet<String>,
) -> RenderedExpr {
    let arg_at = |index: usize, precedence: u8, expanding: &mut BTreeSet<String>| {
        args.get(index)
            .map(|value| render_expr_prec(value, precedence, context, expanding))
            .unwrap_or_else(|| "default".to_string())
    };
    let arg = |index: usize, expanding: &mut BTreeSet<String>| arg_at(index, 0, expanding);
    let receiver = |index: usize, expanding: &mut BTreeSet<String>| {
        args.get(index)
            .map(|value| render_expr_prec(value, PREC_PRIMARY, context, expanding))
            .unwrap_or_else(|| "default".to_string())
    };
    let call = |name: &str, expanding: &mut BTreeSet<String>| {
        RenderedExpr::new(
            format!("{name}({})", render_expr_list(args, context, expanding)),
            PREC_PRIMARY,
        )
    };

    match opcode {
        OpCode::Within => call("Helper.Within", expanding),
        OpCode::Substr => render_byte_slice(opcode, args, context, expanding, "Helper.Range", true),
        OpCode::Modmul => call("Helper.ModMultiply", expanding),
        OpCode::Modpow => call("BigInteger.ModPow", expanding),
        OpCode::Sqrt => call("Helper.Sqrt", expanding),
        OpCode::Nz => {
            let value = args
                .first()
                .map(|expression| render_expr_prec(expression, 0, context, expanding))
                .unwrap_or_else(|| "default".to_string());
            let source = if args
                .first()
                .and_then(|expression| context.exact_csharp_type(expression))
                == Some("BigInteger")
            {
                value
            } else {
                format!("(BigInteger)(dynamic)({value})")
            };
            RenderedExpr::new(format!("{source} != 0"), PREC_EQUALITY)
        }
        OpCode::Size => {
            match args
                .first()
                .map_or(ValueType::Unknown, |value| context.value_type(value))
            {
                ValueType::Map => {
                    RenderedExpr::new(format!("{}.Count", receiver(0, expanding)), PREC_PRIMARY)
                }
                ValueType::Array
                | ValueType::Struct
                | ValueType::Buffer
                | ValueType::ByteString => {
                    RenderedExpr::new(format!("{}.Length", receiver(0, expanding)), PREC_PRIMARY)
                }
                _ => {
                    let rendered = render_low_level_opcode(opcode, args, context, expanding);
                    RenderedExpr::new(format!("(BigInteger){}", rendered.source), PREC_UNARY)
                }
            }
        }
        OpCode::Keys | OpCode::Values => {
            let receiver_type = args
                .first()
                .map_or(ValueType::Unknown, |value| context.value_type(value));
            let known_non_map = matches!(
                receiver_type,
                ValueType::Boolean
                    | ValueType::Integer
                    | ValueType::ByteString
                    | ValueType::Buffer
                    | ValueType::Array
                    | ValueType::Struct
                    | ValueType::Null
            );
            if known_non_map {
                return render_low_level_opcode(opcode, args, context, expanding);
            }
            let property = if opcode == OpCode::Keys {
                "Keys"
            } else {
                "Values"
            };
            RenderedExpr::new(
                format!("{}.{}", receiver(0, expanding), property),
                PREC_PRIMARY,
            )
        }
        OpCode::Isnull => {
            if args.first().is_some_and(|value| {
                matches!(
                    context.value_type(value),
                    ValueType::Boolean | ValueType::Integer
                )
            }) {
                RenderedExpr::new("false", PREC_PRIMARY)
            } else {
                RenderedExpr::new(
                    format!("{} is null", arg_at(0, PREC_RELATIONAL, expanding)),
                    PREC_RELATIONAL,
                )
            }
        }
        OpCode::Newbuffer => RenderedExpr::new(
            format!(
                "new byte[{}]",
                args.first()
                    .map(|value| int_cast(value, context, expanding))
                    .unwrap_or_else(|| "default".to_string())
            ),
            PREC_PRIMARY,
        ),
        OpCode::Cat => render_byte_concat(args, context, expanding),
        OpCode::Left | OpCode::Right => render_byte_slice(
            opcode,
            args,
            context,
            expanding,
            if opcode == OpCode::Left {
                "Helper.Take"
            } else {
                "Helper.Last"
            },
            false,
        ),
        OpCode::Min | OpCode::Max => {
            let name = if opcode == OpCode::Min {
                "BigInteger.Min"
            } else {
                "BigInteger.Max"
            };
            call(name, expanding)
        }
        OpCode::Newarray0 => RenderedExpr::new("new object[0]", PREC_PRIMARY),
        OpCode::Newarray | OpCode::NewarrayT | OpCode::Newstruct => RenderedExpr::new(
            format!(
                "new object[{}]",
                args.first()
                    .map(|value| int_cast(value, context, expanding))
                    .unwrap_or_else(|| "default".to_string())
            ),
            PREC_PRIMARY,
        ),
        OpCode::Newstruct0 => RenderedExpr::new("new object[] { }", PREC_PRIMARY),
        OpCode::Newmap => RenderedExpr::new("new Map<object, object>()", PREC_PRIMARY),
        OpCode::Haskey => {
            let receiver_type = args
                .first()
                .map_or(ValueType::Unknown, |value| context.value_type(value));
            if matches!(
                receiver_type,
                ValueType::Boolean
                    | ValueType::Integer
                    | ValueType::ByteString
                    | ValueType::Buffer
                    | ValueType::Array
                    | ValueType::Struct
                    | ValueType::Null
            ) {
                return render_low_level_opcode(opcode, args, context, expanding);
            }
            RenderedExpr::new(
                format!("{}.HasKey({})", receiver(0, expanding), arg(1, expanding)),
                PREC_PRIMARY,
            )
        }
        OpCode::Pickitem => {
            let receiver_type = args
                .first()
                .map_or(ValueType::Unknown, |value| context.value_type(value));
            let index = if matches!(
                receiver_type,
                ValueType::Array | ValueType::Struct | ValueType::Buffer | ValueType::ByteString
            ) {
                args.get(1)
                    .map(|value| int_cast(value, context, expanding))
                    .unwrap_or_else(|| "default".to_string())
            } else {
                arg(1, expanding)
            };
            let receiver = if matches!(
                receiver_type,
                ValueType::Array
                    | ValueType::Struct
                    | ValueType::Buffer
                    | ValueType::ByteString
                    | ValueType::Map
            ) {
                receiver(0, expanding)
            } else {
                format!("((dynamic)({}))", arg(0, expanding))
            };
            RenderedExpr::new(format!("{receiver}[{index}]"), PREC_PRIMARY)
        }
        OpCode::Setitem => {
            let receiver_type = args
                .first()
                .map_or(ValueType::Unknown, |value| context.value_type(value));
            if matches!(receiver_type, ValueType::Buffer | ValueType::ByteString) {
                let buffer = args.first().map_or_else(
                    || "default".to_string(),
                    |value| {
                        let rendered = render_expr_prec(value, PREC_PRIMARY, context, expanding);
                        if receiver_type == ValueType::ByteString {
                            format!("((byte[])({rendered}))")
                        } else {
                            rendered
                        }
                    },
                );
                let index = args
                    .get(1)
                    .map(|value| int_cast(value, context, expanding))
                    .unwrap_or_else(|| "default".to_string());
                let value = args.get(2).map_or_else(
                    || "default".to_string(),
                    |value| {
                        format!(
                            "(byte)(dynamic)({})",
                            render_expr_prec(value, 0, context, expanding)
                        )
                    },
                );
                RenderedExpr::new(format!("{buffer}[{index}] = {value}"), PREC_ASSIGNMENT)
            } else if matches!(receiver_type, ValueType::Array | ValueType::Struct) {
                let index = args
                    .get(1)
                    .map(|value| int_cast(value, context, expanding))
                    .unwrap_or_else(|| "default".to_string());
                RenderedExpr::new(
                    format!(
                        "{}[{index}] = {}",
                        receiver(0, expanding),
                        arg(2, expanding)
                    ),
                    PREC_ASSIGNMENT,
                )
            } else if receiver_type == ValueType::Map {
                RenderedExpr::new(
                    format!(
                        "{}[{}] = {}",
                        receiver(0, expanding),
                        arg(1, expanding),
                        arg(2, expanding)
                    ),
                    PREC_ASSIGNMENT,
                )
            } else {
                RenderedExpr::new(
                    format!(
                        "((dynamic)({}))[{}] = {}",
                        receiver(0, expanding),
                        arg(1, expanding),
                        arg(2, expanding)
                    ),
                    PREC_ASSIGNMENT,
                )
            }
        }
        OpCode::Append => {
            let receiver_type = args
                .first()
                .map_or(ValueType::Unknown, |value| context.value_type(value));
            if matches!(
                receiver_type,
                ValueType::Boolean
                    | ValueType::Integer
                    | ValueType::ByteString
                    | ValueType::Buffer
                    | ValueType::Map
                    | ValueType::Null
            ) {
                return render_low_level_opcode(opcode, args, context, expanding);
            }
            RenderedExpr::new(
                format!(
                    "((Neo.SmartContract.Framework.List<object>){}).Add({})",
                    arg(0, expanding),
                    arg(1, expanding)
                ),
                PREC_PRIMARY,
            )
        }
        OpCode::Remove => {
            let receiver_type = args
                .first()
                .map_or(ValueType::Unknown, |value| context.value_type(value));
            if receiver_type == ValueType::Map {
                RenderedExpr::new(
                    format!("{}.Remove({})", receiver(0, expanding), arg(1, expanding)),
                    PREC_PRIMARY,
                )
            } else if matches!(receiver_type, ValueType::Array | ValueType::Struct) {
                let index = args
                    .get(1)
                    .map(|value| int_cast(value, context, expanding))
                    .unwrap_or_else(|| "default".to_string());
                RenderedExpr::new(
                    format!(
                        "((Neo.SmartContract.Framework.List<object>){}).RemoveAt({index})",
                        arg(0, expanding)
                    ),
                    PREC_PRIMARY,
                )
            } else {
                render_low_level_opcode(opcode, args, context, expanding)
            }
        }
        OpCode::Clearitems => {
            let receiver_type = args
                .first()
                .map_or(ValueType::Unknown, |value| context.value_type(value));
            if matches!(receiver_type, ValueType::Array | ValueType::Struct) {
                RenderedExpr::new(
                    format!(
                        "((Neo.SmartContract.Framework.List<object>){}).Clear()",
                        arg(0, expanding)
                    ),
                    PREC_PRIMARY,
                )
            } else if receiver_type == ValueType::Map {
                RenderedExpr::new(format!("{}.Clear()", receiver(0, expanding)), PREC_PRIMARY)
            } else {
                render_low_level_opcode(opcode, args, context, expanding)
            }
        }
        OpCode::Reverseitems => {
            let receiver_type = args
                .first()
                .map_or(ValueType::Unknown, |value| context.value_type(value));
            if matches!(
                receiver_type,
                ValueType::Boolean
                    | ValueType::Integer
                    | ValueType::ByteString
                    | ValueType::Buffer
                    | ValueType::Map
                    | ValueType::Null
            ) {
                return render_low_level_opcode(opcode, args, context, expanding);
            }
            RenderedExpr::new(
                format!("Helper.Reverse({})", arg(0, expanding)),
                PREC_PRIMARY,
            )
        }
        OpCode::Popitem => {
            let receiver_type = args
                .first()
                .map_or(ValueType::Unknown, |value| context.value_type(value));
            if matches!(
                receiver_type,
                ValueType::Boolean
                    | ValueType::Integer
                    | ValueType::ByteString
                    | ValueType::Buffer
                    | ValueType::Map
                    | ValueType::Null
            ) {
                return render_low_level_opcode(opcode, args, context, expanding);
            }
            RenderedExpr::new(
                format!(
                    "((Neo.SmartContract.Framework.List<object>){}).PopItem()",
                    arg(0, expanding)
                ),
                PREC_PRIMARY,
            )
        }
        OpCode::Memcpy => render_memcpy(args, context, expanding),
        OpCode::Convert => {
            RenderedExpr::new(format!("(object)({})", arg(0, expanding)), PREC_UNARY)
        }
        OpCode::Istype => RenderedExpr::new(
            format!("{} is object", arg_at(0, PREC_RELATIONAL, expanding)),
            PREC_RELATIONAL,
        ),
        _ => render_low_level_opcode(opcode, args, context, expanding),
    }
}

fn render_byte_concat(
    args: &[Expr],
    context: &ExprContext,
    expanding: &mut BTreeSet<String>,
) -> RenderedExpr {
    let Some(left) = args.first() else {
        return render_low_level_opcode(OpCode::Cat, args, context, expanding);
    };
    let Some(right) = args.get(1) else {
        return render_low_level_opcode(OpCode::Cat, args, context, expanding);
    };
    let left_type = context.value_type(left);
    if matches!(left_type, ValueType::Unknown | ValueType::Any) {
        return render_low_level_opcode(OpCode::Cat, args, context, expanding);
    }

    let left = render_expr_prec(left, 0, context, expanding);
    let left = match left_type {
        ValueType::ByteString if context.exact_csharp_type(&args[0]) == Some("ByteString") => left,
        ValueType::ByteString => format!("(ByteString)({left})"),
        ValueType::Buffer if context.exact_csharp_type(&args[0]) == Some("byte[]") => left,
        ValueType::Buffer => format!("(byte[])({left})"),
        _ => format!("(ByteString)(dynamic)({left})"),
    };
    let right_type = context.value_type(right);
    let right = render_expr_prec(right, 0, context, expanding);
    let right = match right_type {
        ValueType::ByteString if context.exact_csharp_type(&args[1]) == Some("ByteString") => right,
        ValueType::Boolean
        | ValueType::Array
        | ValueType::Struct
        | ValueType::Map
        | ValueType::InteropInterface
        | ValueType::Pointer
        | ValueType::Null => format!("(ByteString)(dynamic)({right})"),
        _ => format!("(ByteString)({right})"),
    };
    RenderedExpr::new(format!("Helper.Concat({left}, {right})"), PREC_PRIMARY)
}

fn render_byte_slice(
    opcode: OpCode,
    args: &[Expr],
    context: &ExprContext,
    expanding: &mut BTreeSet<String>,
    api: &str,
    has_length: bool,
) -> RenderedExpr {
    let Some(source) = args.first() else {
        return render_low_level_opcode(opcode, args, context, expanding);
    };
    let source_type = context.value_type(source);
    if !matches!(source_type, ValueType::ByteString | ValueType::Buffer) {
        return render_low_level_opcode(opcode, args, context, expanding);
    }

    let source = render_expr_prec(source, 0, context, expanding);
    let source = if source_type == ValueType::ByteString {
        format!("(byte[])(ByteString)({source})")
    } else {
        format!("(byte[])({source})")
    };
    let index = args
        .get(1)
        .map(|value| int_cast(value, context, expanding))
        .unwrap_or_else(|| "default".to_string());
    let rendered = if has_length {
        let length = args
            .get(2)
            .map(|value| int_cast(value, context, expanding))
            .unwrap_or_else(|| "default".to_string());
        format!("{api}({source}, {index}, {length})")
    } else {
        format!("{api}({source}, {index})")
    };

    if source_type == ValueType::ByteString {
        RenderedExpr::new(format!("(ByteString)({rendered})"), PREC_UNARY)
    } else {
        RenderedExpr::new(rendered, PREC_PRIMARY)
    }
}

fn render_memcpy(
    args: &[Expr],
    context: &ExprContext,
    expanding: &mut BTreeSet<String>,
) -> RenderedExpr {
    let [destination, destination_index, source, source_index, count] = args else {
        return render_low_level_opcode(OpCode::Memcpy, args, context, expanding);
    };
    if context.value_type(destination) != ValueType::Buffer
        || !matches!(
            context.value_type(source),
            ValueType::ByteString | ValueType::Buffer
        )
    {
        return render_low_level_opcode(OpCode::Memcpy, args, context, expanding);
    }

    RenderedExpr::new(
        format!(
            "Array.Copy((byte[])({}), {}, (byte[])({}), {}, {})",
            render_expr_prec(source, 0, context, expanding),
            int_cast(source_index, context, expanding),
            render_expr_prec(destination, 0, context, expanding),
            int_cast(destination_index, context, expanding),
            int_cast(count, context, expanding)
        ),
        PREC_PRIMARY,
    )
}
