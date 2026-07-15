use std::collections::BTreeMap;

use crate::decompiler::cfg::method_body::SymbolInfo;
use crate::decompiler::ir::{BinOp, Expr, Intrinsic, Literal, SemanticCallTarget, UnaryOp};
use crate::decompiler::native_method_types;
use crate::instruction::OpCode;

use super::declaration_type_catalog::{
    array_element_type, concrete_csharp_type_name, csharp_member_type, csharp_type_for_value_type,
    homogeneous_csharp_array_type,
};

#[cfg_attr(not(test), allow(dead_code))]
pub(in crate::decompiler::csharp::render) fn concrete_definition_type(
    expression: &Expr,
) -> Option<String> {
    concrete_call_type(expression).or_else(|| concrete_expression_type(expression))
}

#[cfg_attr(not(test), allow(dead_code))]
pub(in crate::decompiler::csharp::render) fn concrete_definition_type_with_symbols(
    expression: &Expr,
    symbols: &BTreeMap<String, SymbolInfo>,
) -> Option<String> {
    concrete_definition_type_with_symbols_and_known_types(expression, symbols, &BTreeMap::new())
}

#[cfg_attr(not(test), allow(dead_code))]
pub(in crate::decompiler::csharp::render) fn concrete_definition_type_with_symbols_and_known_types(
    expression: &Expr,
    symbols: &BTreeMap<String, SymbolInfo>,
    known_types: &BTreeMap<String, String>,
) -> Option<String> {
    concrete_definition_type_with_symbols_and_known_types_and_calls(
        expression,
        symbols,
        known_types,
        &BTreeMap::new(),
    )
}

pub(in crate::decompiler::csharp::render) fn concrete_definition_type_with_symbols_and_known_types_and_calls(
    expression: &Expr,
    symbols: &BTreeMap<String, SymbolInfo>,
    known_types: &BTreeMap<String, String>,
    known_call_types: &BTreeMap<usize, String>,
) -> Option<String> {
    concrete_call_type(expression).or_else(|| {
        concrete_expression_type_with_symbols_and_known(
            expression,
            Some(symbols),
            known_types,
            known_call_types,
        )
    })
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

#[cfg_attr(not(test), allow(dead_code))]
fn concrete_expression_type(expression: &Expr) -> Option<String> {
    concrete_expression_type_with_symbols_and_known(
        expression,
        None,
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
}

fn concrete_expression_type_with_symbols_and_known(
    expression: &Expr,
    symbols: Option<&BTreeMap<String, SymbolInfo>>,
    known_types: &BTreeMap<String, String>,
    known_call_types: &BTreeMap<usize, String>,
) -> Option<String> {
    if let Expr::Call {
        target: SemanticCallTarget::Internal { offset, .. },
        ..
    } = expression
    {
        if let Some(call_type) = known_call_types.get(offset) {
            return Some(call_type.clone());
        }
    }
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
        Expr::Variable(name) => known_types.get(name).cloned().or_else(|| {
            symbols
                .and_then(|symbols| symbols.get(name))
                .and_then(|symbol| {
                    csharp_type_for_value_type(symbol.value_type).map(str::to_string)
                })
        }),
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
            (concrete_expression_type_with_symbols_and_known(
                left,
                symbols,
                known_types,
                known_call_types,
            )
            .as_deref()
                == Some("BigInteger")
                && concrete_expression_type_with_symbols_and_known(
                    right,
                    symbols,
                    known_types,
                    known_call_types,
                )
                .as_deref()
                    == Some("BigInteger"))
            .then(|| "BigInteger".to_string())
        }
        Expr::Unary { op, operand } => {
            if *op == UnaryOp::LogicalNot {
                Some("bool".to_string())
            } else {
                concrete_expression_type_with_symbols_and_known(
                    operand,
                    symbols,
                    known_types,
                    known_call_types,
                )
                .filter(|type_name| type_name == "BigInteger")
            }
        }
        Expr::Call { target, args } => match target {
            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(opcode)) => match opcode {
                OpCode::Cat | OpCode::Substr | OpCode::Left | OpCode::Right => {
                    byte_container_result_type(args, symbols, known_types, known_call_types)
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
                OpCode::Pickitem => {
                    pickitem_result_type(args, symbols, known_types, known_call_types)
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
        Expr::Index { base, .. } => {
            if let Expr::NewArray {
                element_type: Some(element_type),
                ..
            } = base.as_ref()
            {
                return csharp_type_for_value_type(*element_type).map(str::to_string);
            }
            let base_type = concrete_expression_type_with_symbols_and_known(
                base,
                symbols,
                known_types,
                known_call_types,
            );
            array_element_type(base_type.as_deref())
        }
        Expr::Member { name, .. } if name.eq_ignore_ascii_case("Length") => {
            Some("BigInteger".to_string())
        }
        Expr::Member { base, name } => concrete_expression_type_with_symbols_and_known(
            base,
            symbols,
            known_types,
            known_call_types,
        )
        .as_deref()
        .and_then(|base_type| csharp_member_type(base_type, name).map(str::to_string)),
        Expr::Cast { target_type, .. } => concrete_csharp_type_name(target_type),
        Expr::Convert { target, .. } => csharp_type_for_value_type(*target).map(str::to_string),
        Expr::IsType { .. } => Some("bool".to_string()),
        Expr::NewArray { element_type, .. } => element_type.and_then(|element_type| {
            csharp_type_for_value_type(element_type).map(|element_type| format!("{element_type}[]"))
        }),
        Expr::Array(elements) => homogeneous_csharp_array_type(elements.iter().map(|element| {
            concrete_expression_type_with_symbols_and_known(
                element,
                symbols,
                known_types,
                known_call_types,
            )
        }))
        .map(str::to_string)
        .or_else(|| Some("object[]".to_string())),
        Expr::Struct(_) => Some("object[]".to_string()),
        Expr::Map(_) => Some("Map<object, object>".to_string()),
        Expr::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            let then_type = concrete_expression_type_with_symbols_and_known(
                then_expr,
                symbols,
                known_types,
                known_call_types,
            )?;
            (concrete_expression_type_with_symbols_and_known(
                else_expr,
                symbols,
                known_types,
                known_call_types,
            )
            .as_deref()
                == Some(then_type.as_str()))
            .then_some(then_type)
        }
    }
}

fn byte_container_result_type(
    args: &[Expr],
    symbols: Option<&BTreeMap<String, SymbolInfo>>,
    known_types: &BTreeMap<String, String>,
    known_call_types: &BTreeMap<usize, String>,
) -> Option<String> {
    let source_type = args.first().and_then(|source| {
        concrete_expression_type_with_symbols_and_known(
            source,
            symbols,
            known_types,
            known_call_types,
        )
    });
    if source_type.as_deref() == Some("byte[]") {
        Some("byte[]".to_string())
    } else {
        Some("ByteString".to_string())
    }
}

fn pickitem_result_type(
    args: &[Expr],
    symbols: Option<&BTreeMap<String, SymbolInfo>>,
    known_types: &BTreeMap<String, String>,
    known_call_types: &BTreeMap<usize, String>,
) -> Option<String> {
    let base_type = args.first().and_then(|base| {
        concrete_expression_type_with_symbols_and_known(
            base,
            symbols,
            known_types,
            known_call_types,
        )
    });
    array_element_type(base_type.as_deref())
}
