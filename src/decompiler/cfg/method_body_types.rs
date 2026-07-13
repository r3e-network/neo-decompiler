use crate::decompiler::analysis::types::ValueType;
use crate::instruction::OpCode;

pub(super) fn intrinsic_result_type(opcode: OpCode) -> ValueType {
    match opcode {
        OpCode::Newarray0
        | OpCode::Newarray
        | OpCode::NewarrayT
        | OpCode::Keys
        | OpCode::Values => ValueType::Array,
        OpCode::Newstruct0 | OpCode::Newstruct => ValueType::Struct,
        OpCode::Newmap => ValueType::Map,
        OpCode::Newbuffer => ValueType::Buffer,
        OpCode::Size | OpCode::Sqrt | OpCode::Min | OpCode::Max => ValueType::Integer,
        OpCode::Haskey | OpCode::Isnull | OpCode::Istype | OpCode::Nz => ValueType::Boolean,
        _ => ValueType::Unknown,
    }
}

pub(super) fn merge_value_types(left: ValueType, right: ValueType) -> ValueType {
    use ValueType::{Any, Null, Unknown};

    if left == right {
        return left;
    }
    match (left, right) {
        (Unknown, value) | (value, Unknown) => value,
        (Null, _) | (_, Null) => Any,
        _ => Any,
    }
}
