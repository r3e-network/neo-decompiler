use super::super::super::*;

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
