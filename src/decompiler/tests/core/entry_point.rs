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
fn mismatch_offset_emits_synthetic_entry_and_keeps_manifest_method() {
    // Script: PUSH1; RET; PUSH2; RET
    let nef_bytes = build_nef(&[0x11, 0x40, 0x12, 0x40]);
    let manifest = ContractManifest::from_json_str(
        r#"
            {
                "name": "OffsetMismatch",
                "abi": {
                    "methods": [
                        {
                            "name": "helper",
                            "parameters": [],
                            "returntype": "Integer",
                            "offset": 2
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

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");

    assert!(
        high_level.contains("fn script_entry() {"),
        "script entry should stay synthetic when ABI offsets do not include bytecode entry"
    );
    assert!(
        high_level.contains("fn helper() -> int {"),
        "manifest method should still be emitted"
    );

    let before_helper = high_level
        .split("fn helper() -> int {")
        .next()
        .expect("entry section present");
    assert!(
        before_helper.contains("0000: PUSH1"),
        "entry section should contain bytecode from script start"
    );
    assert!(
        !before_helper.contains("0002: PUSH2"),
        "entry section should stop before helper method offset"
    );
}
