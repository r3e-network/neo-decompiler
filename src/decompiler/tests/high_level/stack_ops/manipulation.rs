use super::super::*;

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
fn high_level_lifts_boolean_ops() {
    // Script: PUSH1, PUSH1, BOOLAND, RET
    let script = [0x11, 0x11, 0xAB, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    assert!(decompilation
        .high_level
        .as_deref()
        .expect("high-level output")
        .contains("let t2 = t0 && t1;"));
}

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
fn high_level_handles_stack_manipulation_and_unary_ops() {
    // Script: PUSH1, DUP, ADD, INC, RET
    let script = [0x11, 0x4A, 0x9E, 0x9C, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    assert!(decompilation
        .high_level
        .as_deref()
        .expect("high-level output")
        .contains("let t1 = t0; // duplicate top of stack"));
    assert!(decompilation
        .high_level
        .as_deref()
        .expect("high-level output")
        .contains("let t3 = t2 + 1;"));
}
