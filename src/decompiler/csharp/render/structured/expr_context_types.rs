//! Type and collection-provenance helpers for structured expression contexts.

use std::collections::BTreeMap;

use crate::decompiler::analysis::types::ValueType;
use crate::decompiler::ir::Expr;

use super::super::expr_inline::InlineCollector;

pub(super) fn collect_array_values(collector: &InlineCollector) -> BTreeMap<String, Vec<Expr>> {
    collector
        .definitions
        .iter()
        .filter_map(|(name, definitions)| {
            let [definition] = definitions.as_slice() else {
                return None;
            };
            let Expr::Array(elements) = &definition.value else {
                return None;
            };
            let [usage] = collector.uses.get(name)?.as_slice() else {
                return None;
            };
            (definition.scope == usage.scope && definition.order < usage.order)
                .then(|| (name.clone(), elements.clone()))
        })
        .collect()
}

pub(super) fn csharp_type_value_type(csharp_type: &str) -> Option<ValueType> {
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
        | "TriggerType" => Some(ValueType::Integer),
        "bool" => Some(ValueType::Boolean),
        "string" => Some(ValueType::ByteString),
        "ByteString" => Some(ValueType::ByteString),
        "byte[]" => Some(ValueType::Buffer),
        "object[]"
        | "ECPoint[]"
        | "Signer[]"
        | "Notification[]"
        | "ByteString[]"
        | "string[]"
        | "byte[][]"
        | "(ECPoint, BigInteger)[]" => Some(ValueType::Array),
        "Map<object, object>" => Some(ValueType::Map),
        "UInt160" | "UInt256" | "ECPoint" => Some(ValueType::ByteString),
        "Block"
        | "Contract"
        | "Iterator"
        | "Iterator<(ECPoint, BigInteger)>"
        | "Iterator<(int, UInt160)>"
        | "NeoAccountState"
        | "Notification"
        | "StorageContext"
        | "Transaction"
        | "object" => Some(ValueType::InteropInterface),
        _ => None,
    }
}

pub(super) fn exact_common_value_type(left: ValueType, right: ValueType) -> ValueType {
    if left == right && is_concrete_value_type(left) {
        left
    } else {
        ValueType::Unknown
    }
}

fn is_concrete_value_type(value_type: ValueType) -> bool {
    matches!(
        value_type,
        ValueType::Boolean
            | ValueType::Integer
            | ValueType::ByteString
            | ValueType::Buffer
            | ValueType::Array
            | ValueType::Struct
            | ValueType::Map
    )
}
