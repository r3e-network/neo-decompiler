use super::*;

#[test]
fn loop_condition_temp_is_inlined() {
    // Script models: for (loc0 = 0; loc0 < 3; loc0++) {}
    let script = [
        0x57, 0x01, 0x00, 0x10, 0x70, 0x68, 0x13, 0xB5, 0x26, 0x07, 0x21, 0x68, 0x11, 0x9E, 0x70,
        0x22, 0xF4, 0x40,
    ];
    let nef_bytes = build_nef(&script);
    let manifest = ContractManifest::from_json_str(
        r#"
            {
                "name": "LoopInline",
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

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("for (let loc0 = t0; loc0 < t1; loc0 += 1) {"),
        "loop header should inline condition and increment expressions: {high_level}"
    );
    assert!(
        !high_level.contains("let t3 = 1;"),
        "increment temp should be inlined and removed: {high_level}"
    );
}
