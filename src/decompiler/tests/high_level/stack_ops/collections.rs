use super::super::*;

#[test]
fn high_level_packs_literal_arrays() {
    // Script: PUSH1, PUSH2, PUSH2, PACK, RET
    // PACK uses the literal count (2) to build an array from stack values.
    let script = [0x11, 0x12, 0x12, 0xC0, 0x40];
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
        "expected literal PACK to be lifted: {high_level}"
    );
    assert!(
        !high_level.contains("pack_dynamic"),
        "literal PACK should not fall back to dynamic form: {high_level}"
    );
}
