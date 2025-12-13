use super::*;

#[test]
fn high_level_lifts_simple_if_block() {
    // Script: PUSH1, JMPIFNOT +3, PUSH2, RET, PUSH3, RET
    let script = [0x11, 0x26, 0x03, 0x12, 0x40, 0x13, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(high_level.contains("if t0 {"));
    assert!(high_level.contains("// 0003: PUSH2"));
    assert!(high_level.contains("}\n        // 0006: RET"));
}

#[test]
fn high_level_closes_if_at_end() {
    // Script: PUSH1, JMPIFNOT +2, PUSH2, RET
    let script = [0x11, 0x26, 0x02, 0x12, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(high_level.contains("if t0 {"));
    assert!(high_level.contains("        }\n    }\n}"));
}

#[test]
fn high_level_lifts_if_else_block() {
    // Script: PUSH1, JMPIFNOT +3, PUSH2, JMP +2, PUSH3, RET, RET
    let script = [0x11, 0x26, 0x03, 0x12, 0x22, 0x02, 0x13, 0x40, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(high_level.contains("if t0 {"));
    assert!(high_level.contains("else {"));
    assert!(high_level.contains("let t1 = 2;"));
    assert!(high_level.contains("let t2 = 3;"));
}
