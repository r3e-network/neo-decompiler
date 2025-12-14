use super::super::super::*;

#[test]
fn high_level_lifts_reverse3_operation() {
    // Script: PUSH1, PUSH2, PUSH3, REVERSE3, RET
    let script = [0x11, 0x12, 0x13, 0x53, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    assert!(
        decompilation
            .high_level
            .as_deref()
            .expect("high-level output")
            .contains("reverse top 3 stack values"),
        "REVERSE3 should produce reverse comment: {}",
        decompilation
            .high_level
            .as_deref()
            .expect("high-level output")
    );
}

#[test]
fn high_level_lifts_reverse4_operation() {
    // Script: PUSH1, PUSH2, PUSH3, PUSH4, REVERSE4, RET
    let script = [0x11, 0x12, 0x13, 0x14, 0x54, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    assert!(
        decompilation
            .high_level
            .as_deref()
            .expect("high-level output")
            .contains("reverse top 4 stack values"),
        "REVERSE4 should produce reverse comment: {}",
        decompilation
            .high_level
            .as_deref()
            .expect("high-level output")
    );
}

#[test]
fn high_level_lifts_reversen_operation() {
    // Script: PUSH1, PUSH2, PUSH3, PUSH3 (count=3), REVERSEN, RET
    let script = [0x11, 0x12, 0x13, 0x13, 0x55, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    assert!(
        decompilation
            .high_level
            .as_deref()
            .expect("high-level output")
            .contains("reverse top 3 stack values"),
        "REVERSEN should produce reverse comment: {}",
        decompilation
            .high_level
            .as_deref()
            .expect("high-level output")
    );
}
