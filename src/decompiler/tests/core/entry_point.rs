use super::*;

#[test]
fn renames_script_entry_using_manifest_signature() {
    let nef_bytes = sample_nef();
    let manifest = ContractManifest::from_json_str(
        r#"
            {
                "name": "Parametrized",
                "abi": {
                    "methods": [
                        {
                            "name": "deploy",
                            "parameters": [
                                {"name": "owner", "type": "Hash160"},
                                {"name": "amount", "type": "Integer"}
                            ],
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
        .expect("decompile succeeds with manifest signature");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(high_level.contains("fn deploy(owner: hash160, amount: int) {"));
    assert!(
        !high_level.contains("fn other"),
        "additional manifest methods without offsets should not appear"
    );
}

#[test]
fn entry_point_falls_back_to_manifest_method_when_offset_mismatches() {
    let nef_bytes = sample_nef();
    let manifest = ContractManifest::from_json_str(
        r#"
            {
                "name": "OffsetMismatch",
                "abi": {
                    "methods": [
                        {
                            "name": "deploy",
                            "parameters": [{ "name": "owner", "type": "Hash160" }],
                            "returntype": "Void",
                            "offset": 42
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
            .contains("fn deploy(owner: hash160) {"),
        "entry point should use manifest method even when offsets do not align"
    );
}
