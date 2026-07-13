//! Low-level opcode and explicit type-tag rendering for structured C#.

use std::collections::BTreeSet;

use crate::decompiler::analysis::types::ValueType;
use crate::decompiler::helpers::stack_item_type_tag;
use crate::decompiler::ir::Expr;
use crate::instruction::OpCode;

use super::expr::{
    render_expr_list, render_expr_prec, ExprContext, RenderedExpr, PREC_PRIMARY, PREC_UNARY,
};

pub(super) fn render_low_level_opcode(
    opcode: OpCode,
    args: &[Expr],
    context: &ExprContext,
    expanding: &mut BTreeSet<String>,
) -> RenderedExpr {
    RenderedExpr::new(
        format!(
            "Runtime.LoadScript((ByteString)new byte[] {{ 0x{:02X} }}, CallFlags.All, new object[] {{ {} }})",
            opcode.byte(),
            render_expr_list(args, context, expanding)
        ),
        PREC_PRIMARY,
    )
}

pub(super) fn render_tagged_type_opcode(
    opcode: OpCode,
    target: ValueType,
    value: &Expr,
    context: &ExprContext,
    expanding: &mut BTreeSet<String>,
) -> RenderedExpr {
    let value = render_expr_prec(value, 0, context, expanding);
    render_tagged_type_opcode_source(opcode, target, &value, context)
}

pub(super) fn render_tagged_type_opcode_source(
    opcode: OpCode,
    target: ValueType,
    value: &str,
    context: &ExprContext,
) -> RenderedExpr {
    let helper = tagged_opcode_helper_key(opcode, target)
        .and_then(|key| context.tagged_opcode_helper_calls.get(&key))
        .cloned()
        .unwrap_or_else(|| default_tagged_opcode_helper_name(opcode, target));
    RenderedExpr::new(format!("{helper}({value})"), PREC_PRIMARY)
}

pub(in crate::decompiler::csharp::render) fn tagged_opcode_helper_key(
    opcode: OpCode,
    target: ValueType,
) -> Option<(u8, u8)> {
    Some((opcode.byte(), stack_item_type_tag(target)?))
}

pub(in crate::decompiler::csharp::render) fn default_tagged_opcode_helper_name(
    opcode: OpCode,
    target: ValueType,
) -> String {
    let operation = match opcode {
        OpCode::Convert => "Convert",
        OpCode::Istype => "IsType",
        _ => "Opcode",
    };
    let target = match target {
        ValueType::Unknown => "Unknown",
        ValueType::Any => "Any",
        ValueType::Null => "Null",
        ValueType::Boolean => "Boolean",
        ValueType::Integer => "Integer",
        ValueType::ByteString => "ByteString",
        ValueType::Buffer => "Buffer",
        ValueType::Array => "Array",
        ValueType::Struct => "Struct",
        ValueType::Map => "Map",
        ValueType::InteropInterface => "InteropInterface",
        ValueType::Pointer => "Pointer",
    };
    format!("__NeoDecompiler{operation}{target}")
}

pub(super) fn render_low_level_boolean_binary_opcode(
    opcode: OpCode,
    left: &Expr,
    right: &Expr,
    context: &ExprContext,
    expanding: &mut BTreeSet<String>,
) -> RenderedExpr {
    let args = [left.clone(), right.clone()];
    let rendered = render_low_level_opcode(opcode, &args, context, expanding);
    RenderedExpr::new(format!("(bool){}", rendered.source), PREC_UNARY)
}
