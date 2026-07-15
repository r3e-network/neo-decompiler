use std::collections::BTreeSet;

use crate::decompiler::analysis::types::ValueType;
use crate::decompiler::ir::Expr;
use crate::instruction::OpCode;

use super::super::expr::{
    int_cast, render_expr_prec, ExprContext, RenderedExpr, PREC_PRIMARY, PREC_UNARY,
};
use super::super::expr_low_level::render_low_level_opcode;

pub(super) fn render_byte_concat(
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
        ValueType::ByteString
            if context.is_statically_exact_csharp_type(&args[0], "ByteString") =>
        {
            left
        }
        ValueType::ByteString => format!("(ByteString)({left})"),
        ValueType::Buffer if context.is_statically_exact_csharp_type(&args[0], "byte[]") => left,
        ValueType::Buffer => format!("(byte[])({left})"),
        _ => format!("(ByteString)(dynamic)({left})"),
    };
    let right_type = context.value_type(right);
    let right = render_expr_prec(right, 0, context, expanding);
    let right = match right_type {
        ValueType::ByteString
            if context.is_statically_exact_csharp_type(&args[1], "ByteString") =>
        {
            right
        }
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

pub(super) fn render_byte_slice(
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

pub(super) fn render_memcpy(
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
