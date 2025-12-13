use super::*;

#[test]
fn tolerant_mode_emits_unknown_opcode() {
    let script = [0xFFu8, 0x40]; // UNKNOWN, RET
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::with_unknown_handling(UnknownHandling::Permit)
        .decompile_bytes(&nef_bytes)
        .expect("decompile in tolerant mode");

    assert!(
        decompilation
            .pseudocode
            .as_deref()
            .expect("pseudocode output")
            .contains("0000: UNKNOWN_0xFF"),
        "pseudocode should include unknown opcode"
    );
    assert!(
        decompilation
            .high_level
            .as_deref()
            .expect("high-level output")
            .contains("UNKNOWN_0xFF (not yet translated)"),
        "high-level output should note unknown opcode"
    );
}
