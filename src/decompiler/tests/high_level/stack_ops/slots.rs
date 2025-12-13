use super::super::*;

#[test]
fn high_level_lifts_local_slots() {
    // Script: INITSLOT 1,0; PUSH1; STLOC0; LDLOC0; RET
    let script = [0x57, 0x01, 0x00, 0x11, 0x70, 0x68, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(high_level.contains("// declare 1 locals, 0 arguments"));
    assert!(high_level.contains("let loc0 = t0;"));
    assert!(high_level.contains("return loc0;"));
}
