use super::super::*;

#[test]
fn high_level_pops_assert_condition() {
    // Script: PUSH2 (value), PUSH1 (condition), ASSERT, RET
    // ASSERT consumes the condition but keeps the remaining stack.
    let script = [0x12, 0x11, 0x39, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = legacy_high_level_decompiler()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("assert(t1);"),
        "ASSERT should emit an assert call: {high_level}"
    );
    assert!(
        high_level.contains("return t0;"),
        "ASSERT should pop the condition, returning the original value: {high_level}"
    );
}

#[test]
fn high_level_abort_clears_stack() {
    // Script: PUSH1, ABORT, RET
    // ABORT terminates execution; we treat it as clearing the tracked stack.
    let script = [0x11, 0x38, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = legacy_high_level_decompiler()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("abort();"),
        "ABORT should emit an abort call: {high_level}"
    );
    assert!(
        high_level.contains("return;"),
        "stack should be cleared after ABORT: {high_level}"
    );
}

#[test]
fn high_level_throw_clears_stack() {
    // Script: PUSH1, THROW, RET
    // THROW terminates execution; decompiler should not keep stale stack values.
    let script = [0x11, 0x3A, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = legacy_high_level_decompiler()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("throw(t0);"),
        "THROW should emit a throw call: {high_level}"
    );
    assert!(
        high_level.contains("return;"),
        "stack should be cleared after THROW: {high_level}"
    );
}

#[test]
fn high_level_from_ir_default_path_still_uses_stack_emitter() {
    // Legacy high-level path still uses the stack emitter with trace comments
    // (has `t` temps and different shape than the IR dialect).
    let script = [
        0x0C,
        0x01,
        0x00,
        0x10,
        0x0B,
        0x00,
        0x0A,
        0x00,
        0x13,
        0x9B,
        0x26,
        0x07,
        0x21,
        0x0A,
        0x00,
        0x11,
        0x9E,
        0x0B,
        0x00,
        0x22,
        (-12i8) as u8,
        0x40,
    ];
    let nef = build_nef(&script);
    let decompilation = legacy_high_level_decompiler()
        .decompile_bytes(&nef)
        .expect("default high-level decompiles");
    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        !high_level.is_empty(),
        "default high-level path must still produce output"
    );
}
