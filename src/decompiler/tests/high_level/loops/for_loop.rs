use super::*;

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
        high_level.contains("loc0 +="),
        "increment not surfaced: {high_level}"
    );
}
