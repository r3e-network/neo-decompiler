use super::*;

#[test]
fn high_level_lifts_simple_if_block() {
    // Script: PUSH1, JMPIFNOT +5, PUSH2, RET, PUSH3, RET
    let script = [0x11, 0x26, 0x05, 0x12, 0x40, 0x13, 0x40];
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
    // Script: PUSH1, JMPIFNOT +4, PUSH2, RET
    let script = [0x11, 0x26, 0x04, 0x12, 0x40];
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
    // Script: PUSH1, JMPIFNOT +5, PUSH2, JMP +4, PUSH3, RET, RET
    let script = [0x11, 0x26, 0x05, 0x12, 0x22, 0x04, 0x13, 0x40, 0x40];
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

#[test]
fn high_level_lifts_jmpeq_forward_branch() {
    // Script: PUSH1, PUSH1, JMPEQ +4 (to RET), PUSH2, RET
    let script = [0x11, 0x11, 0x28, 0x04, 0x12, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("if t0 != t1 {"),
        "JMPEQ fall-through body should use negated condition: {high_level}"
    );
    assert!(
        !high_level.contains("jump-if-eq ->"),
        "JMPEQ should no longer emit a raw jump warning: {high_level}"
    );
}

#[test]
fn high_level_lifts_jmpif_forward_branch() {
    // Script: PUSH1, JMPIF +4 (to RET), PUSH2, RET
    let script = [0x11, 0x24, 0x04, 0x12, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("if !t0 {"),
        "JMPIF forward branch should be lifted as negated if-condition: {high_level}"
    );
    assert!(
        !high_level.contains("jump-if ->"),
        "JMPIF should no longer emit raw jump-if warnings: {high_level}"
    );
}

#[test]
fn high_level_lifts_jmpif_l_forward_branch() {
    // Script: PUSH1, JMPIF_L +6 (to RET), PUSH2, RET
    let script = [0x11, 0x25, 0x06, 0x00, 0x00, 0x00, 0x12, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("if !t0 {"),
        "JMPIF_L forward branch should be lifted as negated if-condition: {high_level}"
    );
    assert!(
        !high_level.contains("jump-if ->"),
        "JMPIF_L should no longer emit raw jump-if warnings: {high_level}"
    );
}

#[test]
fn high_level_else_branch_restores_pre_branch_stack_snapshot() {
    // Script:
    // PUSH1, PUSH2, PUSH3, PUSH1(cond), JMPIFNOT +6, DROP, DROP, JMP +3, REVERSE3, RET
    // Without else-entry stack restoration, REVERSE3 underflows because the then-branch
    // drops values before the emitter reaches the else block.
    let script = [
        0x11, 0x12, 0x13, 0x11, 0x26, 0x06, 0x45, 0x45, 0x22, 0x03, 0x53, 0x40,
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
        high_level.contains("reverse top 3 stack values"),
        "else branch should retain the pre-branch stack shape: {high_level}"
    );
    assert!(
        !high_level.contains("insufficient values on stack for REVERSE3"),
        "else entry should restore the pre-branch stack snapshot before REVERSE3: {high_level}"
    );
}
