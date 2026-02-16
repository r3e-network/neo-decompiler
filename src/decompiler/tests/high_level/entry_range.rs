use super::*;

#[test]
fn high_level_limits_instructions_to_entry_range() {
    // Script: PUSH1; RET; PUSH2; RET
    let script = [0x11, 0x40, 0x12, 0x40];
    let nef_bytes = build_nef(&script);
    let manifest = ContractManifest::from_json_str(
        r#"
            {
                "name": "Multi",
                "abi": {
                    "methods": [
                        { "name": "entry", "parameters": [], "returntype": "Integer", "offset": 0 },
                        { "name": "other", "parameters": [], "returntype": "Integer", "offset": 2 }
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
    assert!(high_level.contains("let t0 = 1;"), "entry body missing");
    assert!(
        high_level.contains("fn other() -> int {"),
        "additional manifest methods should be emitted in high-level view"
    );
    assert!(
        high_level.contains("return t0;"),
        "additional method body should be decompiled"
    );
    let before_other = high_level
        .split("fn other")
        .next()
        .expect("entry section present");
    assert!(
        !before_other.contains("0002"),
        "entry section should not contain helper instructions"
    );
}

#[test]
fn high_level_trims_initslot_boundaries() {
    let nef_bytes = load_testing_nef("Contract_Delegate.nef");
    let manifest = load_testing_manifest("Contract_Delegate.manifest.json");

    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("// 0000: INITSLOT"),
        "entry block should still be rendered"
    );
    let sum_block = high_level
        .split("\n    fn testDelegate(")
        .next()
        .expect("sumFunc section");
    assert!(
        !sum_block.contains("// 000C: INITSLOT"),
        "should stop at the first INITSLOT boundary for sumFunc"
    );
    assert!(
        !sum_block.contains("return t23;"),
        "duplicate return from appended block should not appear"
    );
    assert!(
        high_level.contains("fn sub_0x000C(arg0, arg1)"),
        "inferred private helper should be rendered as a separate method"
    );
}
