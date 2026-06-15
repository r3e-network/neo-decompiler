use super::*;

fn sample_manifest_json() -> &'static str {
    r#"
        {
            "name": "ExampleContract",
            "groups": [],
            "features": {},
            "supportedstandards": [
                "NEP-17"
            ],
            "abi": {
                "methods": [
                    {
                        "name": "balanceOf",
                        "parameters": [
                            { "name": "account", "type": "Hash160" }
                        ],
                        "returntype": "Integer",
                        "offset": 0,
                        "safe": true
                    }
                ],
                "events": [
                    {
                        "name": "Transfer",
                        "parameters": [
                            { "name": "from", "type": "Hash160" },
                            { "name": "to", "type": "Hash160" },
                            { "name": "amount", "type": "Integer" }
                        ]
                    }
                ]
            },
            "permissions": [
                {
                    "contract": "*",
                    "methods": [
                        "balanceOf",
                        "transfer"
                    ]
                }
            ],
            "trusts": "*",
            "extra": null
        }
        "#
}

#[test]
fn parses_manifest_json() {
    let manifest =
        ContractManifest::from_json_str(sample_manifest_json()).expect("manifest parsed");
    assert_eq!(manifest.name, "ExampleContract");
    assert!(manifest.features.is_empty());
    assert_eq!(manifest.supported_standards, vec!["NEP-17"]);
    assert_eq!(manifest.abi.methods.len(), 1);
    let method = &manifest.abi.methods[0];
    assert_eq!(method.name, "balanceOf");
    assert_eq!(method.return_type, "Integer");
    assert_eq!(method.parameters.len(), 1);
}

#[test]
fn manifest_from_bytes_rejects_invalid_utf8() {
    let bytes = [0xF0, 0x28, 0x8C, 0x28];
    let err = ContractManifest::from_bytes(&bytes).unwrap_err();
    assert!(matches!(
        err,
        crate::error::Error::Manifest(crate::error::ManifestError::InvalidUtf8 { .. })
    ));
}

#[test]
fn manifest_from_bytes_rejects_invalid_json() {
    let bytes = br#"{ "name": "Bad", "abi": { "methods": [], "events": [] }"#;
    let err = ContractManifest::from_bytes(bytes).unwrap_err();
    assert!(matches!(
        err,
        crate::error::Error::Manifest(crate::error::ManifestError::Json(_))
    ));
}

#[test]
fn manifest_from_bytes_rejects_oversized_payloads() {
    let oversized = vec![b'a'; (MAX_MANIFEST_SIZE + 1) as usize];
    let err = ContractManifest::from_bytes(&oversized).unwrap_err();
    assert!(matches!(
        err,
        crate::error::Error::Manifest(crate::error::ManifestError::FileTooLarge {
            size,
            max
        }) if size == MAX_MANIFEST_SIZE + 1 && max == MAX_MANIFEST_SIZE
    ));
}

#[test]
fn parses_wildcard_permission_variants() {
    let json = r#"
        {
            "name": "WildcardContract",
            "abi": { "methods": [], "events": [] },
            "permissions": [
                { "contract": "*", "methods": "*" },
                { "contract": "*" }
            ],
            "trusts": "*"
        }
    "#;

    let manifest = ContractManifest::from_json_str(json).expect("manifest parsed");
    assert_eq!(manifest.permissions.len(), 2);

    assert!(matches!(
        manifest.permissions[0].contract,
        ManifestPermissionContract::Wildcard(ref value) if value == "*"
    ));
    assert!(matches!(
        manifest.permissions[0].methods,
        ManifestPermissionMethods::Wildcard(ref value) if value == "*"
    ));

    assert!(matches!(
        manifest.permissions[1].contract,
        ManifestPermissionContract::Wildcard(ref value) if value == "*"
    ));
    assert!(matches!(
        manifest.permissions[1].methods,
        ManifestPermissionMethods::Wildcard(ref value) if value == "*"
    ));

    assert!(matches!(
        manifest.trusts,
        Some(ManifestTrusts::Wildcard(ref value)) if value == "*"
    ));
}

#[test]
fn strict_manifest_parsing_accepts_valid_sample() {
    let manifest = ContractManifest::from_json_str_strict(sample_manifest_json())
        .expect("strict manifest parsed");
    assert_eq!(manifest.name, "ExampleContract");
}

#[test]
fn strict_manifest_parsing_rejects_non_wildcard_permission_methods() {
    let json = r#"
        {
            "name": "InvalidStrict",
            "abi": { "methods": [], "events": [] },
            "permissions": [
                { "contract": "*", "methods": "all" }
            ],
            "trusts": "*"
        }
    "#;

    let err = ContractManifest::from_json_str_strict(json).unwrap_err();
    assert!(matches!(
        err,
        crate::error::Error::Manifest(crate::error::ManifestError::Validation { .. })
    ));
}

#[test]
fn strict_manifest_parsing_rejects_non_wildcard_trusts_string() {
    let json = r#"
        {
            "name": "InvalidTrusts",
            "abi": { "methods": [], "events": [] },
            "trusts": "invalid"
        }
    "#;

    let err = ContractManifest::from_json_str_strict(json).unwrap_err();
    assert!(matches!(
        err,
        crate::error::Error::Manifest(crate::error::ManifestError::Validation { .. })
    ));
}

#[test]
fn classifies_official_string_permission_descriptors() {
    let json = r#"
        {
            "name": "DescriptorShapes",
            "abi": { "methods": [], "events": [] },
            "permissions": [
                { "contract": "*", "methods": "*" },
                { "contract": "0x0123456789abcdef0123456789abcdef01234567", "methods": "*" },
                { "contract": "03b209fd4f53a7170ea4444e0cb0a6bb6a53c2bd016926989cf85f9b0fba17a70c", "methods": "*" },
                { "contract": "not-a-descriptor", "methods": "*" }
            ]
        }
    "#;

    let manifest = ContractManifest::from_json_str(json).expect("manifest parsed");
    assert!(matches!(
        manifest.permissions[0].contract,
        ManifestPermissionContract::Wildcard(ref value) if value == "*"
    ));
    assert!(matches!(
        manifest.permissions[1].contract,
        ManifestPermissionContract::Hash { ref hash }
            if hash == "0x0123456789abcdef0123456789abcdef01234567"
    ));
    assert!(matches!(
        manifest.permissions[2].contract,
        ManifestPermissionContract::Group { ref group }
            if group == "03b209fd4f53a7170ea4444e0cb0a6bb6a53c2bd016926989cf85f9b0fba17a70c"
    ));
    assert!(matches!(
        manifest.permissions[3].contract,
        ManifestPermissionContract::Other(_)
    ));
}

#[test]
fn strict_manifest_parsing_rejects_malformed_permission_descriptor() {
    let json = r#"
        {
            "name": "InvalidDescriptor",
            "abi": { "methods": [], "events": [] },
            "permissions": [
                { "contract": { "hash": "0x0123456789abcdef0123456789abcdef01234567" }, "methods": "*" }
            ]
        }
    "#;

    let err = ContractManifest::from_json_str_strict(json).unwrap_err();
    assert!(matches!(
        err,
        crate::error::Error::Manifest(crate::error::ManifestError::Validation { .. })
    ));
}

#[test]
fn strict_manifest_parsing_rejects_non_empty_features() {
    let json = r#"
        {
            "name": "LegacyFeatures",
            "abi": { "methods": [], "events": [] },
            "features": { "storage": true }
        }
    "#;

    let err = ContractManifest::from_json_str_strict(json).unwrap_err();
    assert!(matches!(
        err,
        crate::error::Error::Manifest(crate::error::ManifestError::Validation { .. })
    ));

    // Tolerant parsing keeps the raw object for inspection.
    let manifest = ContractManifest::from_json_str(json).expect("tolerant parse");
    assert_eq!(manifest.features.len(), 1);
    assert_eq!(
        manifest.features.get("storage"),
        Some(&serde_json::Value::Bool(true))
    );
}
