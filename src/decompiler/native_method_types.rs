//! Return types for native method tokens with stable Neo C# framework APIs.

use crate::decompiler::analysis::types::ValueType;
use crate::native_contracts;

/// A native method return type in both the VM-oriented and C#-oriented forms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativeMethodReturnType {
    pub(crate) value_type: ValueType,
    pub(crate) csharp_type: &'static str,
}

/// Resolve a known native method token to its stable C# return type.
///
/// The hash and method name are both required. A method name alone is not
/// enough to establish a native API contract because arbitrary external
/// contracts may expose the same name. Restricted calls remain dynamic so
/// their `Contract.Call` fallback does not acquire an unchecked static type.
pub(crate) fn lookup(
    hash_le: Option<&str>,
    method: &str,
    call_flags: Option<u8>,
) -> Option<NativeMethodReturnType> {
    if call_flags != Some(0x0F) {
        return None;
    }
    let hash = parse_hash(hash_le?)?;
    let hint = native_contracts::describe_method_token(&hash, method)?;
    let method = hint.canonical_method?;

    match (hint.contract, method) {
        // The VM represents strings as ByteStrings, while generated C# keeps
        // the framework's string spelling for direct helper return types.
        ("StdLib", "Atoi") => Some(return_type(ValueType::Integer, "BigInteger")),
        ("StdLib", "Itoa") => Some(return_type(ValueType::ByteString, "string")),
        (
            "StdLib",
            "Base64Encode" | "Base64UrlEncode" | "Base58Encode" | "Base58CheckEncode" | "HexEncode",
        ) => Some(return_type(ValueType::ByteString, "string")),
        ("StdLib", "Base64Decode" | "Base58Decode" | "Base58CheckDecode" | "HexDecode") => {
            Some(return_type(ValueType::ByteString, "ByteString"))
        }
        ("StdLib", "Serialize") => Some(return_type(ValueType::ByteString, "ByteString")),
        ("StdLib", "JsonSerialize") => Some(return_type(ValueType::ByteString, "string")),
        ("StdLib", "MemoryCompare" | "MemorySearch" | "StrLen") => {
            Some(return_type(ValueType::Integer, "BigInteger"))
        }
        ("StdLib", "StringSplit") => Some(return_type(ValueType::Array, "object[]")),
        _ => None,
    }
}

fn return_type(value_type: ValueType, csharp_type: &'static str) -> NativeMethodReturnType {
    NativeMethodReturnType {
        value_type,
        csharp_type,
    }
}

fn parse_hash(hash_le: &str) -> Option<[u8; 20]> {
    if hash_le.len() != 40 {
        return None;
    }
    let mut hash = [0u8; 20];
    for (index, pair) in hash_le.as_bytes().chunks_exact(2).enumerate() {
        let pair = std::str::from_utf8(pair).ok()?;
        hash[index] = u8::from_str_radix(pair, 16).ok()?;
    }
    Some(hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    const STDLIB: &str = "C0EF39CEE0E4E925C6C2A06A79E1440DD86FCEAC";

    #[test]
    fn resolves_only_hash_bound_native_signatures() {
        let result = lookup(Some(STDLIB), "strLen", Some(0x0F)).expect("StdLib method");
        assert_eq!(result.value_type, ValueType::Integer);
        assert_eq!(result.csharp_type, "BigInteger");

        assert!(lookup(Some(STDLIB), "strLen", Some(0x01)).is_none());
        assert!(lookup(Some("00"), "strLen", Some(0x0F)).is_none());
        assert!(lookup(Some(STDLIB), "notAStdLibMethod", Some(0x0F)).is_none());
    }

    #[test]
    fn maps_framework_string_and_collection_returns() {
        let string = lookup(Some(STDLIB), "base58CheckEncode", Some(0x0F)).unwrap();
        assert_eq!(string.value_type, ValueType::ByteString);
        assert_eq!(string.csharp_type, "string");

        let array = lookup(Some(STDLIB), "stringSplit", Some(0x0F)).unwrap();
        assert_eq!(array.value_type, ValueType::Array);
        assert_eq!(array.csharp_type, "object[]");

        let bytes = lookup(Some(STDLIB), "base58CheckDecode", Some(0x0F)).unwrap();
        assert_eq!(bytes.value_type, ValueType::ByteString);
        assert_eq!(bytes.csharp_type, "ByteString");

        let json = lookup(Some(STDLIB), "jsonSerialize", Some(0x0F)).unwrap();
        assert_eq!(json.value_type, ValueType::ByteString);
        assert_eq!(json.csharp_type, "string");
    }
}
