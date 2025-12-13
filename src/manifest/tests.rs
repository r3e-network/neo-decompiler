use super::*;

fn sample_manifest_json() -> &'static str {
    r#"
        {
            "name": "ExampleContract",
            "groups": [],
            "features": {
                "storage": true,
                "payable": false
            },
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
    assert!(manifest.has_storage());
    assert!(!manifest.is_payable());
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
