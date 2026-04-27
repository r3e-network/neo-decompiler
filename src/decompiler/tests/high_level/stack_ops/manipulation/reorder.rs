use super::super::super::*;

#[test]
fn high_level_lifts_rot_operation() {
    // Script: PUSH1, PUSH2, PUSH3, ROT, RET
    // ROT: [a, b, c] -> [b, c, a]; the final RET returns the now-top
    // (the original `a`, i.e. PUSH1's value `1`).
    let script = [0x11, 0x12, 0x13, 0x51, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    // The substantive lift: PUSH1 ends up on top after ROT, so the
    // `return` references the value that started life as PUSH1.
    assert!(
        high_level.contains("return 1;") || high_level.contains("return t0;"),
        "ROT should expose the rotated-to-top value via return: {high_level}",
    );
}

#[test]
fn high_level_lifts_tuck_operation() {
    // Script: PUSH1, PUSH2, TUCK, RET — TUCK duplicates top under second.
    // After TUCK: stack is [2, 1, 2]; RET returns the top (a copy of
    // PUSH2's value). The lifted form materialises that copy into a
    // fresh `let tN = t_push2;` so the return references either the
    // original PUSH2 temp or the materialised copy.
    let script = [0x11, 0x12, 0x4E, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    let returns_any_temp_or_two = ["return 2;", "return t1;", "return t2;"]
        .iter()
        .any(|needle| high_level.contains(needle));
    assert!(
        returns_any_temp_or_two,
        "TUCK should leave PUSH2's value (or its copy) as the top of the lifted return: {high_level}",
    );
}
