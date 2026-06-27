use crate::decompiler::analysis::types::ValueType;

/// Render an inferred [`ValueType`] as a high-level pseudo-language type.
///
/// Returns an empty string for `Unknown` so callers can fall back to the
/// existing untyped `loc0`/`arg0` rendering when no type was inferred
/// (keeps the annotation purely additive).
///
/// Currently consumed by the C# renderer via [`inferred_type_to_csharp`]; this
/// pseudo variant is wired up in the Phase-4 AST-based high-level renderer and
/// is unit-tested here so the mapping stays correct in the meantime.
#[allow(dead_code)]
pub(in super::super) fn inferred_type_to_pseudo(ty: ValueType) -> &'static str {
    match ty {
        ValueType::Unknown => "",
        ValueType::Any => "any",
        ValueType::Null => "null",
        ValueType::Boolean => "bool",
        ValueType::Integer => "int",
        ValueType::ByteString => "byte[]",
        ValueType::Buffer => "byte[]",
        ValueType::Array => "array",
        ValueType::Struct => "struct",
        ValueType::Map => "map",
        ValueType::InteropInterface => "interop",
        ValueType::Pointer => "pointer",
    }
}

/// Render an inferred [`ValueType`] as a C# type name.
///
/// Returns an empty string for `Unknown` (see [`inferred_type_to_pseudo`]).
pub(in super::super) fn inferred_type_to_csharp(ty: ValueType) -> &'static str {
    match ty {
        ValueType::Unknown => "",
        ValueType::Any => "object",
        ValueType::Null => "object",
        ValueType::Boolean => "bool",
        ValueType::Integer => "BigInteger",
        ValueType::ByteString => "ByteString",
        ValueType::Buffer => "byte[]",
        ValueType::Array => "object[]",
        ValueType::Struct => "object[]",
        ValueType::Map => "Map",
        ValueType::InteropInterface => "object",
        ValueType::Pointer => "object",
    }
}

/// Convert a Neo manifest ABI type into the high-level pseudo-language type.
///
/// For unknown type names (anything outside the standard manifest spec
/// vocabulary), the original input string is returned verbatim so the
/// user's chosen casing/spelling survives. Matches the JS port's
/// `formatManifestType`.
pub(crate) fn format_manifest_type(kind: &str) -> String {
    match kind.to_ascii_lowercase().as_str() {
        "void" => "void".into(),
        "boolean" => "bool".into(),
        "integer" => "int".into(),
        "string" => "string".into(),
        "hash160" => "hash160".into(),
        "hash256" => "hash256".into(),
        "publickey" => "publickey".into(),
        "bytearray" => "bytes".into(),
        "signature" => "signature".into(),
        "array" => "array".into(),
        "map" => "map".into(),
        "interopinterface" => "interop".into(),
        "any" => "any".into(),
        _ => kind.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{format_manifest_type, inferred_type_to_csharp, inferred_type_to_pseudo};
    use crate::decompiler::analysis::types::ValueType;

    #[test]
    fn known_kinds_normalise_regardless_of_case() {
        assert_eq!(format_manifest_type("Integer"), "int");
        assert_eq!(format_manifest_type("integer"), "int");
        assert_eq!(format_manifest_type("INTEGER"), "int");
        assert_eq!(format_manifest_type("Boolean"), "bool");
        assert_eq!(format_manifest_type("Hash160"), "hash160");
        assert_eq!(format_manifest_type("PublicKey"), "publickey");
        assert_eq!(format_manifest_type("InteropInterface"), "interop");
    }

    #[test]
    fn unknown_kinds_preserve_original_case() {
        // Parity with the JS port's `formatManifestType`: unknown type
        // names round-trip verbatim instead of being lowercased.
        assert_eq!(format_manifest_type("MyCustomType"), "MyCustomType");
        assert_eq!(format_manifest_type("Foo_Bar"), "Foo_Bar");
        assert_eq!(format_manifest_type(""), "");
    }

    #[test]
    fn inferred_pseudo_types_map_correctly() {
        assert_eq!(inferred_type_to_pseudo(ValueType::Integer), "int");
        assert_eq!(inferred_type_to_pseudo(ValueType::Boolean), "bool");
        assert_eq!(inferred_type_to_pseudo(ValueType::ByteString), "byte[]");
        assert_eq!(inferred_type_to_pseudo(ValueType::Buffer), "byte[]");
        assert_eq!(inferred_type_to_pseudo(ValueType::Array), "array");
        assert_eq!(inferred_type_to_pseudo(ValueType::Map), "map");
        assert_eq!(inferred_type_to_pseudo(ValueType::Any), "any");
        // Unknown must be empty so callers fall back to the untyped rendering.
        assert_eq!(inferred_type_to_pseudo(ValueType::Unknown), "");
    }

    #[test]
    fn inferred_csharp_types_map_correctly() {
        assert_eq!(inferred_type_to_csharp(ValueType::Integer), "BigInteger");
        assert_eq!(inferred_type_to_csharp(ValueType::Boolean), "bool");
        assert_eq!(inferred_type_to_csharp(ValueType::ByteString), "ByteString");
        assert_eq!(inferred_type_to_csharp(ValueType::Buffer), "byte[]");
        assert_eq!(inferred_type_to_csharp(ValueType::Array), "object[]");
        assert_eq!(inferred_type_to_csharp(ValueType::Map), "Map");
        assert_eq!(inferred_type_to_csharp(ValueType::Any), "object");
        assert_eq!(inferred_type_to_csharp(ValueType::Unknown), "");
    }
}
