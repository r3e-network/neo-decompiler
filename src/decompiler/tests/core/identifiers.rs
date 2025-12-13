use super::*;

#[test]
fn contract_name_is_sanitized_with_manifest() {
    let nef_bytes = sample_nef();
    let manifest = ContractManifest::from_json_str(
        r#"
            {
                "name": "  Weird Name! ",
                "abi": {
                    "methods": [
                        { "name": "main", "parameters": [], "returntype": "Void", "offset": 0 }
                    ],
                    "events": []
                },
                "permissions": [],
                "trusts": "*"
            }
            "#,
    )
    .expect("manifest parsed");

    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    assert!(
        decompilation
            .high_level
            .as_deref()
            .expect("high-level output")
            .contains("contract Weird_Name"),
        "contract name should be sanitized"
    );
}

#[test]
fn high_level_sanitizes_manifest_method_and_parameter_names() {
    let nef_bytes = sample_nef();
    let manifest = ContractManifest::from_json_str(
        r#"
            {
                "name": "Sanitized",
                "abi": {
                    "methods": [
                        {
                            "name": "deploy-contract",
                            "parameters": [{ "name": "owner-name", "type": "Hash160" }],
                            "returntype": "Void",
                            "offset": 0
                        }
                    ],
                    "events": []
                },
                "permissions": [],
                "trusts": "*"
            }
            "#,
    )
    .expect("manifest parsed");

    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    assert!(
        decompilation
            .high_level
            .as_deref()
            .expect("high-level output")
            .contains("fn deploy_contract(owner_name: hash160) {"),
        "high-level signature should sanitize identifiers"
    );
}

#[test]
fn sanitize_identifier_handles_edge_cases() {
    assert_eq!(
        sanitize_identifier(" 123 hello-world__"),
        "_123_hello_world"
    );
    assert_eq!(sanitize_identifier("9lives"), "_9lives");
    assert_eq!(sanitize_identifier("!!!"), "param");
}
