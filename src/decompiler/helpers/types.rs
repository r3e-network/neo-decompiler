/// Convert a Neo manifest ABI type into the high-level pseudo-language type.
///
/// For unknown type names (anything outside the standard manifest spec
/// vocabulary), the original input string is returned verbatim so the
/// user's chosen casing/spelling survives. Matches the JS port's
/// `formatManifestType`.
pub(in super::super) fn format_manifest_type(kind: &str) -> String {
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
    use super::format_manifest_type;

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
}
