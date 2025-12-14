use super::super::super::*;

#[test]
fn high_level_lifts_rot_operation() {
    // Script: PUSH1, PUSH2, PUSH3, ROT, RET
    // ROT: [a, b, c] -> [b, c, a]
    let script = [0x11, 0x12, 0x13, 0x51, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    assert!(
        decompilation
            .high_level
            .as_deref()
            .expect("high-level output")
            .contains("rotate top three stack values"),
        "ROT should produce rotate comment: {}",
        decompilation
            .high_level
            .as_deref()
            .expect("high-level output")
    );
}

#[test]
fn high_level_lifts_tuck_operation() {
    // Script: PUSH1, PUSH2, TUCK, RET
    // TUCK: copies top to below second
    let script = [0x11, 0x12, 0x4E, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    assert!(
        decompilation
            .high_level
            .as_deref()
            .expect("high-level output")
            .contains("tuck top of stack"),
        "TUCK should produce tuck comment: {}",
        decompilation
            .high_level
            .as_deref()
            .expect("high-level output")
    );
}
