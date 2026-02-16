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

#[test]
fn high_level_unpack_of_stored_packed_value_keeps_reverse3_stack_shape() {
    // Script:
    //   INITSLOT 1,0
    //   PUSH1; PUSH2; PUSH2; PACK; STLOC0
    //   PUSH3; LDLOC0; UNPACK; DROP; REVERSE3; RET
    let script = [
        0x57, 0x01, 0x00, // INITSLOT 1 local, 0 args
        0x11, 0x12, 0x12, 0xC0, 0x70, // PUSH1; PUSH2; PUSH2; PACK; STLOC0
        0x13, 0x68, 0xC1, 0x45, 0x53, 0x40, // PUSH3; LDLOC0; UNPACK; DROP; REVERSE3; RET
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
        high_level.contains("pack 2 element"),
        "script should include literal PACK shape used for UNPACK modeling: {high_level}"
    );
    assert!(
        high_level.contains("reverse top 3 stack values"),
        "REVERSE3 should still be recognized after UNPACK of stored PACK value: {high_level}"
    );
    assert!(
        !high_level.contains("insufficient values on stack for REVERSE3"),
        "UNPACK from stored PACK value should preserve enough stack entries for REVERSE3: {high_level}"
    );
}
