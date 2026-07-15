//! VM intrinsic result typing for [`ExprContext`].

use crate::decompiler::analysis::types::ValueType;
use crate::decompiler::ir::Expr;
use crate::instruction::OpCode;

use super::types;
use super::ExprContext;

/// Infer the VM value type produced by a known intrinsic opcode.
pub(super) fn value_type(context: &ExprContext, opcode: OpCode, args: &[Expr]) -> ValueType {
    match opcode {
        OpCode::Newarray0 | OpCode::Newarray | OpCode::NewarrayT => ValueType::Array,
        OpCode::Keys | OpCode::Values => ValueType::Array,
        OpCode::Newstruct0 | OpCode::Newstruct => ValueType::Struct,
        OpCode::Newmap => ValueType::Map,
        OpCode::Newbuffer => ValueType::Buffer,
        OpCode::Within => ValueType::Boolean,
        OpCode::Depth | OpCode::Size | OpCode::Sqrt | OpCode::Min | OpCode::Max => {
            ValueType::Integer
        }
        OpCode::Modmul | OpCode::Modpow => ValueType::Integer,
        OpCode::Haskey | OpCode::Isnull | OpCode::Istype | OpCode::Nz => ValueType::Boolean,
        OpCode::Pickitem => args.first().map_or(ValueType::Unknown, |base| {
            if let Expr::NewArray {
                element_type: Some(element_type),
                ..
            } = base
            {
                return *element_type;
            }
            if let Some(element_type) = context
                .exact_csharp_type(base)
                .and_then(types::csharp_array_element_value_type)
            {
                return element_type;
            }
            match context.value_type(base) {
                ValueType::ByteString | ValueType::Buffer => ValueType::Integer,
                _ => ValueType::Unknown,
            }
        }),
        OpCode::Substr | OpCode::Left | OpCode::Right => {
            args.first().map_or(ValueType::Unknown, |source| {
                match context.value_type(source) {
                    ValueType::ByteString => ValueType::ByteString,
                    ValueType::Buffer => ValueType::Buffer,
                    _ => ValueType::Unknown,
                }
            })
        }
        OpCode::Cat => {
            args.first()
                .map_or(ValueType::Unknown, |left| match context.value_type(left) {
                    ValueType::ByteString => ValueType::ByteString,
                    ValueType::Buffer => ValueType::Buffer,
                    _ => ValueType::Unknown,
                })
        }
        _ => ValueType::Unknown,
    }
}
