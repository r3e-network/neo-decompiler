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
        .contains("return t0 && t1;"));
}

#[test]
fn high_level_handles_stack_manipulation_and_unary_ops() {
    // Script: PUSH1, DUP, ADD, INC, RET
    let script = [0x11, 0x4A, 0x9E, 0x9C, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let hl = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    // DUP of a simple literal/identifier no longer materializes a
    // temp — the duplicate reference is just another copy of the
    // existing value (mirrors JS port's `materialiseStackTopForDup`
    // skip for `SIMPLE_IDENT_OR_LITERAL_RE`). So the t-numbering
    // shifts by one compared to the older "always materialize DUP"
    // behaviour: ADD now allocates `t1`, INC's INC allocates t2 (then
    // gets inlined into the return).
    assert!(hl.contains("let t1 = t0 + t0;"), "expected t0+t0: {hl}");
    assert!(hl.contains("return t1 + 1;"), "expected return t1+1: {hl}");
}
