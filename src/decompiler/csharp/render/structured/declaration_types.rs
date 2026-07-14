use crate::decompiler::analysis::types::ValueType;
use crate::decompiler::ir::{BinOp, Expr, Intrinsic, Literal, SemanticCallTarget, UnaryOp};
use crate::decompiler::native_method_types;
use crate::instruction::OpCode;

pub(in crate::decompiler::csharp::render) fn concrete_definition_type(
    expression: &Expr,
) -> Option<String> {
    if let Expr::Call { target, .. } = expression {
        match target {
            SemanticCallTarget::MethodToken {
                name,
                hash_le,
                call_flags,
                ..
            } => {
                if let Some(return_type) =
                    native_method_types::lookup(hash_le.as_deref(), name, *call_flags)
                {
                    return Some(return_type.csharp_type.to_string());
                }
            }
            SemanticCallTarget::Syscall { hash, .. } => {
                if let Some(return_type) = crate::decompiler::syscall_types::lookup(*hash) {
                    return Some(return_type.csharp_type.to_string());
                }
            }
            _ => {}
        }
    }
    concrete_expression_type(expression)
}

fn concrete_expression_type(expression: &Expr) -> Option<String> {
    match expression {
        Expr::Literal(Literal::Int(_) | Literal::BigInt(_)) => Some("BigInteger".to_string()),
        // Neo treats compiler string literals as byte strings. The generated
        // C# framework accepts the source spelling directly as ByteString.
        Expr::Literal(Literal::String(_) | Literal::Bytes(_)) => Some("ByteString".to_string()),
        Expr::Literal(Literal::Bool(_)) => Some("bool".to_string()),
        Expr::Literal(Literal::Null) | Expr::Unknown | Expr::Variable(_) | Expr::StackTemp(_) => {
            None
        }
        Expr::Binary { op, left, right } => {
            if matches!(
                op,
                BinOp::Eq
                    | BinOp::Ne
                    | BinOp::Lt
                    | BinOp::Le
                    | BinOp::Gt
                    | BinOp::Ge
                    | BinOp::LogicalAnd
                    | BinOp::LogicalOr
            ) {
                return Some("bool".to_string());
            }
            (concrete_expression_type(left).as_deref() == Some("BigInteger")
                && concrete_expression_type(right).as_deref() == Some("BigInteger"))
            .then(|| "BigInteger".to_string())
        }
        Expr::Unary { op, operand } => {
            if *op == UnaryOp::LogicalNot {
                Some("bool".to_string())
            } else {
                concrete_expression_type(operand).filter(|type_name| type_name == "BigInteger")
            }
        }
        Expr::Call { target, .. } => match target {
            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(opcode)) => match opcode {
                OpCode::Cat | OpCode::Substr | OpCode::Left | OpCode::Right => {
                    Some("ByteString".to_string())
                }
                OpCode::Depth | OpCode::Size | OpCode::Sqrt | OpCode::Min | OpCode::Max => {
                    Some("BigInteger".to_string())
                }
                OpCode::Haskey | OpCode::Isnull | OpCode::Istype | OpCode::Nz => {
                    Some("bool".to_string())
                }
                OpCode::Newbuffer => Some("byte[]".to_string()),
                OpCode::Newarray0
                | OpCode::Newarray
                | OpCode::NewarrayT
                | OpCode::Newstruct0
                | OpCode::Newstruct
                | OpCode::Keys
                | OpCode::Values
                | OpCode::Unpack => Some("object[]".to_string()),
                OpCode::Newmap => Some("Map<object, object>".to_string()),
                _ => None,
            },
            SemanticCallTarget::Intrinsic(Intrinsic::UnpackPackStruct) => {
                Some("object[]".to_string())
            }
            SemanticCallTarget::Internal { .. }
            | SemanticCallTarget::MethodToken { .. }
            | SemanticCallTarget::Syscall { .. }
            | SemanticCallTarget::Unresolved { .. } => None,
        },
        Expr::Index { base, .. } => match base.as_ref() {
            Expr::NewArray {
                element_type: Some(element_type),
                ..
            } => csharp_type_for_value_type(*element_type).map(str::to_string),
            _ => None,
        },
        Expr::Member { name, .. } if name.eq_ignore_ascii_case("Length") => {
            Some("BigInteger".to_string())
        }
        Expr::Member { .. } => None,
        Expr::Cast { target_type, .. } => concrete_csharp_type_name(target_type),
        Expr::Convert { target, .. } => csharp_type_for_value_type(*target).map(str::to_string),
        Expr::IsType { .. } => Some("bool".to_string()),
        Expr::NewArray { element_type, .. } => element_type.and_then(|element_type| {
            csharp_type_for_value_type(element_type).map(|element_type| format!("{element_type}[]"))
        }),
        Expr::Array(_) | Expr::Struct(_) => Some("object[]".to_string()),
        Expr::Map(_) => Some("Map<object, object>".to_string()),
        Expr::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            let then_type = concrete_expression_type(then_expr)?;
            (concrete_expression_type(else_expr).as_deref() == Some(then_type.as_str()))
                .then_some(then_type)
        }
    }
}

fn csharp_type_for_value_type(value_type: ValueType) -> Option<&'static str> {
    match value_type {
        ValueType::Boolean => Some("bool"),
        ValueType::Integer => Some("BigInteger"),
        ValueType::ByteString => Some("ByteString"),
        ValueType::Buffer => Some("byte[]"),
        ValueType::Array | ValueType::Struct => Some("object[]"),
        ValueType::Map => Some("Map<object, object>"),
        ValueType::Unknown
        | ValueType::Any
        | ValueType::Null
        | ValueType::InteropInterface
        | ValueType::Pointer => None,
    }
}

fn concrete_csharp_type_name(type_name: &str) -> Option<String> {
    matches!(
        type_name,
        "BigInteger"
            | "bool"
            | "ByteString"
            | "byte[]"
            | "object[]"
            | "Map<object, object>"
            | "string"
            | "UInt160"
            | "UInt256"
            | "ECPoint"
    )
    .then(|| type_name.to_string())
}

pub(super) fn concrete_type_matches_value_type(type_name: &str, value_type: ValueType) -> bool {
    match value_type {
        ValueType::Unknown | ValueType::Any => true,
        ValueType::Null => false,
        ValueType::Boolean => type_name == "bool",
        ValueType::Integer => type_name == "BigInteger",
        ValueType::ByteString => matches!(
            type_name,
            "ByteString" | "string" | "UInt160" | "UInt256" | "ECPoint"
        ),
        ValueType::Buffer => type_name == "byte[]",
        ValueType::Array => type_name.ends_with("[]"),
        ValueType::Struct => type_name == "object[]",
        ValueType::Map => type_name == "Map<object, object>",
        ValueType::InteropInterface | ValueType::Pointer => false,
    }
}
