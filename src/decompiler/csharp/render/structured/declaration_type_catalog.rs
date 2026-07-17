use crate::decompiler::analysis::types::ValueType;

/// Convert the VM-level value category to the concrete C# type used by the
/// declaration and expression planners.
pub(super) fn csharp_type_for_value_type(value_type: ValueType) -> Option<&'static str> {
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

/// Return a concrete array type when every element has the same known C# type.
/// Mixed or unknown elements intentionally fall back to the VM's object-array
/// representation at the call site.
pub(in crate::decompiler::csharp::render) fn homogeneous_csharp_array_type(
    element_types: impl IntoIterator<Item = Option<String>>,
) -> Option<&'static str> {
    let mut candidate = None;
    for element_type in element_types {
        let element_type = element_type?;
        if candidate
            .as_ref()
            .is_some_and(|candidate| candidate != &element_type)
        {
            return None;
        }
        candidate = Some(element_type);
    }
    let candidate = candidate?;
    match candidate.as_str() {
        "BigInteger" => Some("BigInteger[]"),
        "bool" => Some("bool[]"),
        "byte" => Some("byte[]"),
        "byte[]" => Some("byte[][]"),
        "ByteString" => Some("ByteString[]"),
        "ECPoint" => Some("ECPoint[]"),
        "Map<object, object>" => Some("Map<object, object>[]"),
        "Notification" => Some("Notification[]"),
        "object" => Some("object[]"),
        "object[]" => Some("object[][]"),
        "Signer" => Some("Signer[]"),
        "string" => Some("string[]"),
        "UInt160" => Some("UInt160[]"),
        "UInt256" => Some("UInt256[]"),
        "WitnessRule" => Some("WitnessRule[]"),
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

pub(super) fn concrete_csharp_type_name(type_name: &str) -> Option<String> {
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

pub(super) fn array_element_type(base_type: Option<&str>) -> Option<String> {
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
