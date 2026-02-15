use super::*;

#[test]
fn high_level_emits_break_and_continue() {
    // Script demonstrating break/continue inside a while loop
    let script = [
        0x57, 0x01, 0x00, 0x10, 0x70, 0x68, 0x13, 0xB5, 0x26, 0x1A, 0x68, 0x11, 0xB3, 0x26, 0x08,
        0x68, 0x11, 0x9E, 0x70, 0x22, 0xF2, 0x68, 0x12, 0xB3, 0x26, 0x04, 0x22, 0x08, 0x68, 0x11,
        0x9E, 0x70, 0x22, 0xE5, 0x40,
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
        high_level.contains("break;"),
        "missing break statement: {high_level}"
    );
    assert!(
        high_level.contains("continue;"),
        "missing continue statement: {high_level}"
    );
}
