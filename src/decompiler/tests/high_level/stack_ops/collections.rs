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

#[test]
fn high_level_rewrites_pickitem_as_indexing() {
    // Script: NEWARRAY0, PUSH0, PICKITEM, RET
    let script = [0xC2, 0x10, 0xCE, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("t0[t1]"),
        "expected PICKITEM to be rewritten as indexing: {high_level}"
    );
    assert!(
        !high_level.contains(" get "),
        "infix get should not appear after rewrite: {high_level}"
    );
}

#[test]
fn high_level_rewrites_setitem_as_index_assignment() {
    // Script: NEWMAP, PUSH0, PUSH1, SETITEM, RET
    let script = [0xC8, 0x10, 0x11, 0xD0, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("t0[t1] = t2;"),
        "expected SETITEM to be rewritten as indexing assignment: {high_level}"
    );
    assert!(
        !high_level.contains("set_item("),
        "set_item helper should not appear after rewrite: {high_level}"
    );
}

#[test]
fn high_level_rewrites_haskey_as_function_call() {
    // Script: NEWMAP, PUSH0, HASKEY, RET
    let script = [0xC8, 0x10, 0xCB, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("has_key("),
        "expected HASKEY to be rewritten as function call: {high_level}"
    );
    assert!(
        !high_level.contains(" has_key "),
        "infix has_key should not appear after rewrite: {high_level}"
    );
}

#[test]
fn high_level_istype_respects_operand_tag() {
    // Script: PUSH1, ISTYPE array (0x40), RET
    let script = [0x11, 0xD9, 0x40, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("is_type_array("),
        "ISTYPE should map to a helper named for the operand tag: {high_level}"
    );
    assert!(
        !high_level.contains("is_type("),
        "High-level output should not use the two-argument shorthand: {high_level}"
    );
}
