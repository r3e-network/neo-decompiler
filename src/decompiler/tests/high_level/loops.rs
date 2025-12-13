use super::*;

#[test]
fn high_level_lifts_simple_while_loop() {
    // Script: PUSH1, JMPIFNOT +3 (to RET), NOP, JMP -6 (to PUSH1), RET
    let script = [0x11, 0x26, 0x03, 0x21, 0x22, 0xFA, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("while t0 {"),
        "missing while block: {high_level}"
    );
    assert!(
        !high_level.contains("jump ->"),
        "loop back-edge should be lifted: {high_level}"
    );
}

#[test]
fn high_level_lifts_do_while_loop() {
    // Script: body; PUSH1; JMPIF -5; RET
    let script = [0x11, 0x21, 0x11, 0x24, 0xFB, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("do {"),
        "missing do/while header: {high_level}"
    );
    assert!(
        high_level.contains("} while ("),
        "missing do/while tail: {high_level}"
    );
}

#[test]
fn high_level_lifts_for_loop() {
    // Script models: for (loc0 = 0; loc0 < 3; loc0++) {}
    let script = [
        0x57, 0x01, 0x00, 0x10, 0x70, 0x68, 0x13, 0xB5, 0x26, 0x07, 0x21, 0x68, 0x11, 0x9E, 0x70,
        0x22, 0xF4, 0x40,
    ];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("for (let loc0 = t0;"),
        "missing for-loop header: {high_level}"
    );
    assert!(
        high_level.contains("loc0 = loc0 +"),
        "increment not surfaced: {high_level}"
    );
}

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
        high_level.contains("for (let loc0 = t0; loc0 < t1; loc0 = loc0 + 1) {"),
        "loop header should inline condition and increment expressions: {high_level}"
    );
    assert!(
        !high_level.contains("let t3 = 1;"),
        "increment temp should be inlined and removed: {high_level}"
    );
}

#[test]
fn high_level_emits_break_and_continue() {
    // Script demonstrating break/continue inside a while loop
    let script = [
        0x57, 0x01, 0x00, 0x10, 0x70, 0x68, 0x13, 0xB5, 0x26, 0x18, 0x68, 0x11, 0xB3, 0x26, 0x06,
        0x68, 0x11, 0x9E, 0x70, 0x22, 0xF0, 0x68, 0x12, 0xB3, 0x26, 0x02, 0x22, 0x06, 0x68, 0x11,
        0x9E, 0x70, 0x22, 0xE3, 0x40,
    ];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("break;"),
        "missing break statement: {high_level}"
    );
    assert!(
        high_level.contains("continue;"),
        "missing continue statement: {high_level}"
    );
}
