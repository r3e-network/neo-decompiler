use super::*;

#[test]
fn high_level_lifts_do_while_loop() {
    // Script: body; PUSH1; JMPIF -5; RET
    let script = [0x11, 0x21, 0x11, 0x24, 0xFB, 0x40];
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("do {"),
        "missing do/while header: {high_level}"
    );
    assert!(
        high_level.contains("} while ("),
        "missing do/while tail: {high_level}"
    );
}
