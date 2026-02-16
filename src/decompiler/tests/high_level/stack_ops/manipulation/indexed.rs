use super::super::super::*;

#[test]
fn high_level_lifts_pick_with_literal_index() {
    // Script: PUSH1, PUSH2, PUSH1 (index), PICK, RET
    let script = [0x11, 0x12, 0x11, 0x4D, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("pick stack[1]"),
        "literal PICK should duplicate a stack value: {high_level}"
    );
    assert!(
        high_level.contains("return t3;"),
        "picked value should be returned: {high_level}"
    );
}

#[test]
fn high_level_lifts_xdrop_with_literal_index() {
    // Script: PUSH1, PUSH2, PUSH3, PUSH1 (index), XDROP, RET
    // XDROP uses the literal index (1) to remove the second item from the top.
    let script = [0x11, 0x12, 0x13, 0x11, 0x48, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("xdrop stack[1]"),
        "literal XDROP should remove a stack value: {high_level}"
    );
    assert!(
        high_level.contains("return t2;"),
        "XDROP should preserve the top value (PUSH3): {high_level}"
    );
}

#[test]
fn high_level_pick_preserves_packed_shape_for_unpack_reverse4() {
    // Script:
    //   INITSLOT 1,0
    //   PUSH1; PUSH2; PUSH2; PACK; STLOC0
    //   PUSH3; LDLOC0; PUSH0; PICK; UNPACK; DROP; REVERSE4; RET
    let script = [
        0x57, 0x01, 0x00, // INITSLOT 1 local, 0 args
        0x11, 0x12, 0x12, 0xC0, 0x70, // PUSH1; PUSH2; PUSH2; PACK; STLOC0
        0x13, 0x68, 0x10, 0x4D, 0xC1, 0x45, 0x54,
        0x40, // PUSH3; LDLOC0; PUSH0; PICK; UNPACK; DROP; REVERSE4; RET
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
        high_level.contains("reverse top 4 stack values"),
        "REVERSE4 should be modeled after PICK->UNPACK on known PACK value: {high_level}"
    );
    assert!(
        !high_level.contains("insufficient values on stack for REVERSE4"),
        "PICK should preserve packed shape metadata for downstream UNPACK stack modeling: {high_level}"
    );
}
