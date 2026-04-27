use super::*;

#[test]
fn untranslated_opcode_inline_comment_survives_clean_mode() {
    // Earlier `warn()` delegated to `note()`, which gated the inline
    // `// XXXX: <opcode> (not yet translated)` comment on
    // `emit_trace_comments`. In clean mode the comment was silently
    // dropped, leaving the lifted source visually complete even when
    // it had a real hole. The structured warning still survived in
    // `warnings: Vec<String>`, but a typical reader of the rendered
    // source had no in-place signal. The JS port has always emitted
    // the comment unconditionally; both ports now surface the marker
    // at the gap regardless of trace mode.
    let script = [0xFFu8, 0x40]; // UNKNOWN, RET
    let nef_bytes = build_nef(&script);
    let decompilation = Decompiler::with_unknown_handling(UnknownHandling::Permit)
        .with_trace_comments(false)
        .decompile_bytes(&nef_bytes)
        .expect("decompile in clean mode");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("UNKNOWN_0xFF (not yet translated)"),
        "clean-mode high-level should still surface the inline untranslated marker:\n{high_level}"
    );
    // Structured warning channel must keep firing too, regardless of
    // trace mode (this is what programmatic tooling consumes).
    assert!(
        decompilation
            .warnings
            .iter()
            .any(|w| w.contains("UNKNOWN_0xFF (not yet translated)")),
        "structured warning channel should also surface the untranslated marker:\n{:?}",
        decompilation.warnings,
    );
}

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
