use std::collections::BTreeMap;

use crate::decompiler::analysis::types::ValueType;
use crate::decompiler::cfg::method_body::SymbolInfo;
use crate::decompiler::ir::{BinOp, Expr, Intrinsic, Literal, SemanticCallTarget, UnaryOp};
use crate::decompiler::native_method_types;
use crate::instruction::OpCode;

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

pub(in crate::decompiler::csharp::render) fn concrete_definition_type_with_symbols_and_known_types(
    expression: &Expr,
    symbols: &BTreeMap<String, SymbolInfo>,
    known_types: &BTreeMap<String, String>,
) -> Option<String> {
    concrete_call_type(expression).or_else(|| {
        concrete_expression_type_with_symbols_and_known(expression, Some(symbols), known_types)
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
    concrete_expression_type_with_symbols_and_known(expression, None, &BTreeMap::new())
}

fn concrete_expression_type_with_symbols_and_known(
    expression: &Expr,
    symbols: Option<&BTreeMap<String, SymbolInfo>>,
    known_types: &BTreeMap<String, String>,
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
            (concrete_expression_type_with_symbols_and_known(left, symbols, known_types).as_deref()
                == Some("BigInteger")
                && concrete_expression_type_with_symbols_and_known(right, symbols, known_types)
                    .as_deref()
                    == Some("BigInteger"))
            .then(|| "BigInteger".to_string())
        }
        Expr::Unary { op, operand } => {
            if *op == UnaryOp::LogicalNot {
                Some("bool".to_string())
            } else {
                concrete_expression_type_with_symbols_and_known(operand, symbols, known_types)
                    .filter(|type_name| type_name == "BigInteger")
            }
        }
        Expr::Call { target, args } => match target {
            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(opcode)) => match opcode {
                OpCode::Cat | OpCode::Substr | OpCode::Left | OpCode::Right => {
                    byte_container_result_type(args, symbols, known_types)
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
                OpCode::Pickitem => pickitem_result_type(args, symbols, known_types),
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
            let base_type =
                concrete_expression_type_with_symbols_and_known(base, symbols, known_types);
            array_element_type(base_type.as_deref())
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
            let then_type =
                concrete_expression_type_with_symbols_and_known(then_expr, symbols, known_types)?;
            (concrete_expression_type_with_symbols_and_known(else_expr, symbols, known_types)
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
) -> Option<String> {
    let source_type = args.first().and_then(|source| {
        concrete_expression_type_with_symbols_and_known(source, symbols, known_types)
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
) -> Option<String> {
    let base_type = args.first().and_then(|base| {
        concrete_expression_type_with_symbols_and_known(base, symbols, known_types)
    });
    array_element_type(base_type.as_deref())
}

fn array_element_type(base_type: Option<&str>) -> Option<String> {
    match base_type {
        Some("ByteString" | "byte[]" | "BigInteger[]") => Some("BigInteger".to_string()),
        Some("bool[]") => Some("bool".to_string()),
        Some("string[]") => Some("string".to_string()),
        Some("ByteString[]") => Some("ByteString".to_string()),
        Some("ECPoint[]") => Some("ECPoint".to_string()),
        Some("Signer[]") => Some("Signer".to_string()),
        Some("Notification[]") => Some("Notification".to_string()),
        Some("UInt160[]") => Some("UInt160".to_string()),
        Some("UInt256[]") => Some("UInt256".to_string()),
        Some("byte[][]") => Some("byte[]".to_string()),
        Some("object[][]") => Some("object[]".to_string()),
        Some("(ECPoint, BigInteger)[]") => Some("(ECPoint, BigInteger)".to_string()),
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

/// Map a concrete C# type spelling back to the conservative VM value kind it
/// represents. Native/syscall return tables and private-call propagation use
/// this same mapping so declaration and expression planning stay consistent.
pub(in crate::decompiler::csharp::render) fn csharp_type_value_type(
    csharp_type: &str,
) -> Option<ValueType> {
    match csharp_type {
        "BigInteger" => Some(ValueType::Integer),
        "byte"
        | "sbyte"
        | "short"
        | "ushort"
        | "int"
        | "uint"
        | "long"
        | "ulong"
        | "VMState"
        | "CallFlags"
        | "FindOptions"
        | "NamedCurve"
        | "NamedCurveHash"
        | "Role"
        | "TransactionAttributeType"
        | "TriggerType"
        | "WitnessScope" => Some(ValueType::Integer),
        "bool" => Some(ValueType::Boolean),
        "string" => Some(ValueType::ByteString),
        "ByteString" => Some(ValueType::ByteString),
        "byte[]" => Some(ValueType::Buffer),
        "BigInteger[]"
        | "bool[]"
        | "object[]"
        | "ECPoint[]"
        | "Signer[]"
        | "Notification[]"
        | "UInt160[]"
        | "UInt256[]"
        | "ByteString[]"
        | "WitnessRule[]"
        | "string[]"
        | "byte[][]"
        | "object[][]"
        | "Map<object, object>[]"
        | "(ECPoint, BigInteger)[]"
        | "(int, UInt160)[]" => Some(ValueType::Array),
        "Map<object, object>" => Some(ValueType::Map),
        "UInt160" | "UInt256" | "ECPoint" => Some(ValueType::ByteString),
        "(ECPoint, BigInteger)" | "(int, UInt160)" => Some(ValueType::Struct),
        "Block"
        | "Contract"
        | "Iterator"
        | "Iterator<(ECPoint, BigInteger)>"
        | "Iterator<(int, UInt160)>"
        | "NeoAccountState"
        | "Notification"
        | "ContractManifest"
        | "StorageContext"
        | "Signer"
        | "WitnessRule"
        | "Transaction"
        | "object" => Some(ValueType::InteropInterface),
        _ => None,
    }
}

pub(in crate::decompiler::csharp::render) fn csharp_array_element_value_type(
    csharp_type: &str,
) -> Option<ValueType> {
    csharp_array_element_type(csharp_type).and_then(csharp_type_value_type)
}

pub(in crate::decompiler::csharp::render) fn csharp_array_element_type(
    csharp_type: &str,
) -> Option<&'static str> {
    match csharp_type {
        "BigInteger[]" => Some("BigInteger"),
        "bool[]" => Some("bool"),
        "byte[][]" => Some("byte[]"),
        "ByteString[]" => Some("ByteString"),
        "ECPoint[]" => Some("ECPoint"),
        "Map<object, object>[]" => Some("Map<object, object>"),
        "Notification[]" => Some("Notification"),
        "object[][]" => Some("object[]"),
        "(ECPoint, BigInteger)[]" => Some("(ECPoint, BigInteger)"),
        "(int, UInt160)[]" => Some("(int, UInt160)"),
        "Signer[]" => Some("Signer"),
        "string[]" => Some("string"),
        "UInt160[]" => Some("UInt160"),
        "UInt256[]" => Some("UInt256"),
        "WitnessRule[]" => Some("WitnessRule"),
        _ => None,
    }
}

pub(in crate::decompiler::csharp::render) fn csharp_member_type(
    base_type: &str,
    member: &str,
) -> Option<&'static str> {
    match (base_type, member) {
        ("Transaction", "Hash") => Some("UInt256"),
        ("Transaction", "Version") => Some("byte"),
        ("Transaction", "Nonce") => Some("uint"),
        ("Transaction", "Sender") => Some("UInt160"),
        ("Transaction", "SystemFee" | "NetworkFee") => Some("long"),
        ("Transaction", "ValidUntilBlock") => Some("uint"),
        ("Transaction", "Script") => Some("ByteString"),
        ("Block", "Hash" | "PrevHash" | "MerkleRoot") => Some("UInt256"),
        ("Block", "Version" | "Index") => Some("uint"),
        ("Block", "Timestamp" | "Nonce") => Some("ulong"),
        ("Block", "PrimaryIndex") => Some("byte"),
        ("Block", "NextConsensus") => Some("UInt160"),
        ("Block", "TransactionsCount") => Some("int"),
        ("Notification", "ScriptHash") => Some("UInt160"),
        ("Notification", "EventName") => Some("string"),
        ("Notification", "State") => Some("object[]"),
        ("Contract", "Id") => Some("int"),
        ("Contract", "UpdateCounter") => Some("ushort"),
        ("Contract", "Hash") => Some("UInt160"),
        ("Contract", "Nef") => Some("ByteString"),
        ("Contract", "Manifest") => Some("ContractManifest"),
        ("Signer", "Account") => Some("UInt160"),
        ("Signer", "Scopes") => Some("WitnessScope"),
        ("Signer", "AllowedContracts") => Some("UInt160[]"),
        ("Signer", "AllowedGroups") => Some("ECPoint[]"),
        ("Signer", "Rules") => Some("WitnessRule[]"),
        ("NeoAccountState", "Balance" | "Height" | "LastGasPerVote") => Some("BigInteger"),
        ("NeoAccountState", "VoteTo") => Some("ECPoint"),
        ("Iterator<(ECPoint, BigInteger)>", "Value") => Some("(ECPoint, BigInteger)"),
        ("Iterator<(int, UInt160)>", "Value") => Some("(int, UInt160)"),
        _ => None,
    }
}

fn concrete_csharp_type_name(type_name: &str) -> Option<String> {
    matches!(
        type_name,
        "BigInteger"
            | "byte"
            | "sbyte"
            | "short"
            | "ushort"
            | "int"
            | "uint"
            | "long"
            | "ulong"
            | "VMState"
            | "CallFlags"
            | "FindOptions"
            | "NamedCurve"
            | "NamedCurveHash"
            | "Role"
            | "TransactionAttributeType"
            | "TriggerType"
            | "WitnessScope"
            | "bool"
            | "ByteString"
            | "BigInteger[]"
            | "byte[]"
            | "bool[]"
            | "object[]"
            | "Map<object, object>"
            | "string"
            | "string[]"
            | "UInt160"
            | "UInt256"
            | "ECPoint"
            | "ECPoint[]"
            | "Signer[]"
            | "Notification[]"
            | "UInt160[]"
            | "UInt256[]"
            | "ByteString[]"
            | "WitnessRule[]"
            | "byte[][]"
            | "object[][]"
            | "Map<object, object>[]"
            | "(ECPoint, BigInteger)[]"
            | "(int, UInt160)[]"
            | "Block"
            | "Contract"
            | "Iterator"
            | "Iterator<(ECPoint, BigInteger)>"
            | "Iterator<(int, UInt160)>"
            | "NeoAccountState"
            | "Notification"
            | "ContractManifest"
            | "StorageContext"
            | "Signer"
            | "WitnessRule"
            | "Transaction"
            | "object"
            | "(ECPoint, BigInteger)"
            | "(int, UInt160)"
    )
    .then(|| type_name.to_string())
}

pub(super) fn concrete_type_matches_value_type(type_name: &str, value_type: ValueType) -> bool {
    match value_type {
        ValueType::Unknown | ValueType::Any => true,
        ValueType::Null => false,
        ValueType::Boolean => type_name == "bool",
        ValueType::Integer => matches!(
            type_name,
            "BigInteger"
                | "byte"
                | "sbyte"
                | "short"
                | "ushort"
                | "int"
                | "uint"
                | "long"
                | "ulong"
                | "VMState"
                | "CallFlags"
                | "FindOptions"
                | "NamedCurve"
                | "NamedCurveHash"
                | "Role"
                | "TransactionAttributeType"
                | "TriggerType"
                | "WitnessScope"
        ),
        ValueType::ByteString => matches!(
            type_name,
            "ByteString" | "string" | "UInt160" | "UInt256" | "ECPoint"
        ),
        ValueType::Buffer => type_name == "byte[]",
        ValueType::Array => type_name.ends_with("[]"),
        ValueType::Struct => matches!(
            type_name,
            "object[]" | "(ECPoint, BigInteger)" | "(int, UInt160)"
        ),
        ValueType::Map => type_name == "Map<object, object>",
        ValueType::InteropInterface => matches!(
            type_name,
            "Block"
                | "Contract"
                | "Iterator"
                | "Iterator<(ECPoint, BigInteger)>"
                | "Iterator<(int, UInt160)>"
                | "NeoAccountState"
                | "Notification"
                | "ContractManifest"
                | "StorageContext"
                | "Signer"
                | "WitnessRule"
                | "Transaction"
                | "object"
        ),
        ValueType::Pointer => false,
    }
}
