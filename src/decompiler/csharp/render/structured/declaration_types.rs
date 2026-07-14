use std::collections::BTreeMap;

use crate::decompiler::analysis::types::ValueType;
use crate::decompiler::cfg::method_body::SymbolInfo;
use crate::decompiler::ir::{BinOp, Expr, Intrinsic, Literal, SemanticCallTarget, UnaryOp};
use crate::decompiler::native_method_types;
use crate::instruction::OpCode;

pub(in crate::decompiler::csharp::render) fn concrete_definition_type(
    expression: &Expr,
) -> Option<String> {
    concrete_call_type(expression).or_else(|| concrete_expression_type(expression))
}

pub(in crate::decompiler::csharp::render) fn concrete_definition_type_with_symbols(
    expression: &Expr,
    symbols: &BTreeMap<String, SymbolInfo>,
) -> Option<String> {
    concrete_call_type(expression)
        .or_else(|| concrete_expression_type_with_symbols(expression, Some(symbols)))
}

fn concrete_call_type(expression: &Expr) -> Option<String> {
    let Expr::Call { target, .. } = expression else {
        return None;
    };
    match target {
        SemanticCallTarget::MethodToken {
            name,
            hash_le,
            call_flags,
            ..
        } => native_method_types::lookup(hash_le.as_deref(), name, *call_flags)
            .map(|return_type| return_type.csharp_type.to_string()),
        SemanticCallTarget::Syscall { hash, .. } => crate::decompiler::syscall_types::lookup(*hash)
            .map(|return_type| return_type.csharp_type.to_string()),
        _ => None,
    }
}

fn concrete_expression_type(expression: &Expr) -> Option<String> {
    concrete_expression_type_with_symbols(expression, None)
}

fn concrete_expression_type_with_symbols(
    expression: &Expr,
    symbols: Option<&BTreeMap<String, SymbolInfo>>,
) -> Option<String> {
    if let Some(call_type) = concrete_call_type(expression) {
        return Some(call_type);
    }
    match expression {
        Expr::Literal(Literal::Int(_) | Literal::BigInt(_)) => Some("BigInteger".to_string()),
        // Neo treats compiler string literals as byte strings. The generated
        // C# framework accepts the source spelling directly as ByteString.
        Expr::Literal(Literal::String(_) | Literal::Bytes(_)) => Some("ByteString".to_string()),
        Expr::Literal(Literal::Bool(_)) => Some("bool".to_string()),
        Expr::Literal(Literal::Null) | Expr::Unknown | Expr::StackTemp(_) => None,
        Expr::Variable(name) => symbols
            .and_then(|symbols| symbols.get(name))
            .and_then(|symbol| csharp_type_for_value_type(symbol.value_type).map(str::to_string)),
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
            (concrete_expression_type_with_symbols(left, symbols).as_deref() == Some("BigInteger")
                && concrete_expression_type_with_symbols(right, symbols).as_deref()
                    == Some("BigInteger"))
            .then(|| "BigInteger".to_string())
        }
        Expr::Unary { op, operand } => {
            if *op == UnaryOp::LogicalNot {
                Some("bool".to_string())
            } else {
                concrete_expression_type_with_symbols(operand, symbols)
                    .filter(|type_name| type_name == "BigInteger")
            }
        }
        Expr::Call { target, args } => match target {
            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(opcode)) => match opcode {
                OpCode::Cat | OpCode::Substr | OpCode::Left | OpCode::Right => {
                    byte_container_result_type(args, symbols)
                }
                OpCode::Within | OpCode::Haskey | OpCode::Isnull | OpCode::Istype | OpCode::Nz => {
                    Some("bool".to_string())
                }
                OpCode::Depth
                | OpCode::Size
                | OpCode::Sqrt
                | OpCode::Min
                | OpCode::Max
                | OpCode::Modmul
                | OpCode::Modpow => Some("BigInteger".to_string()),
                OpCode::Pickitem => pickitem_result_type(args, symbols),
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
        Expr::Index { base, .. } => {
            if let Expr::NewArray {
                element_type: Some(element_type),
                ..
            } = base.as_ref()
            {
                return csharp_type_for_value_type(*element_type).map(str::to_string);
            }
            let base_type = concrete_expression_type_with_symbols(base, symbols);
            matches!(base_type.as_deref(), Some("ByteString" | "byte[]"))
                .then(|| "BigInteger".to_string())
        }
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
            let then_type = concrete_expression_type_with_symbols(then_expr, symbols)?;
            (concrete_expression_type_with_symbols(else_expr, symbols).as_deref()
                == Some(then_type.as_str()))
            .then_some(then_type)
        }
    }
}

fn byte_container_result_type(
    args: &[Expr],
    symbols: Option<&BTreeMap<String, SymbolInfo>>,
) -> Option<String> {
    let source_type = args
        .first()
        .and_then(|source| concrete_expression_type_with_symbols(source, symbols));
    if source_type.as_deref() == Some("byte[]") {
        Some("byte[]".to_string())
    } else {
        Some("ByteString".to_string())
    }
}

fn pickitem_result_type(
    args: &[Expr],
    symbols: Option<&BTreeMap<String, SymbolInfo>>,
) -> Option<String> {
    let base_type = args
        .first()
        .and_then(|base| concrete_expression_type_with_symbols(base, symbols));
    match base_type.as_deref() {
        Some("ByteString" | "byte[]") => Some("BigInteger".to_string()),
        Some("BigInteger[]") => Some("BigInteger".to_string()),
        Some("bool[]") => Some("bool".to_string()),
        Some("ByteString[]") => Some("ByteString".to_string()),
        Some("byte[][]") => Some("byte[]".to_string()),
        Some("object[][]") => Some("object[]".to_string()),
        Some("Map<object, object>[]") => Some("Map<object, object>".to_string()),
        _ => None,
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
