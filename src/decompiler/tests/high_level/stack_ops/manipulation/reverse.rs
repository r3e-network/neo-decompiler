use super::super::super::*;

#[test]
fn high_level_lifts_reverse3_operation() {
    // Script: PUSH1, PUSH2, PUSH3, REVERSE3, RET. After REVERSE3 the
    // top of stack is PUSH1's value (the stack flips end-for-end).
    let script = [0x11, 0x12, 0x13, 0x53, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("return 1;") || high_level.contains("return t0;"),
        "REVERSE3 should expose PUSH1's value at the top via return: {high_level}",
    );
}

#[test]
fn high_level_lifts_reverse4_operation() {
    // Script: PUSH1, PUSH2, PUSH3, PUSH4, REVERSE4, RET. After
    // REVERSE4 the top of stack is PUSH1's value.
    let script = [0x11, 0x12, 0x13, 0x14, 0x54, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("return 1;") || high_level.contains("return t0;"),
        "REVERSE4 should expose PUSH1's value at the top via return: {high_level}",
    );
}

#[test]
fn high_level_lifts_reversen_operation() {
    // Script: PUSH1, PUSH2, PUSH3, PUSH3 (count=3), REVERSEN, RET.
    // REVERSEN reverses the top 3 entries; the top after reversal is
    // PUSH1's value.
    let script = [0x11, 0x12, 0x13, 0x13, 0x55, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("return 1;") || high_level.contains("return t0;"),
        "REVERSEN should expose PUSH1's value at the top via return: {high_level}",
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
    // The previous "reverse top 3 stack values" comment was VM
    // narration — stripped from clean output now. The substantive
    // check below ensures REVERSE3 didn't underflow.
    assert!(
        !high_level.contains("insufficient values on stack for REVERSE3"),
        "UNPACK from stored PACK value should preserve enough stack entries for REVERSE3: {high_level}"
    );
}
