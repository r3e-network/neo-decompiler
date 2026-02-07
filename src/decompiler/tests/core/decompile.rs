use super::*;

#[test]
fn decompile_end_to_end() {
    let nef_bytes = sample_nef();
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    assert_eq!(decompilation.instructions.len(), 4);
    assert!(decompilation
        .pseudocode
        .as_deref()
        .expect("pseudocode output")
        .contains("ADD"));
    assert!(decompilation
        .high_level
        .as_deref()
        .expect("high-level output")
        .contains("contract NeoContract"));
    assert!(decompilation
        .high_level
        .as_deref()
        .expect("high-level output")
        .contains("fn script_entry()"));
}

#[test]
fn decompile_with_manifest_produces_contract_name() {
    let nef_bytes = sample_nef();
    let manifest = sample_manifest();
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef_bytes, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds with manifest");

    assert!(decompilation
        .high_level
        .as_deref()
        .expect("high-level output")
        .contains("contract ExampleContract"));
    assert!(decompilation
        .high_level
        .as_deref()
        .expect("high-level output")
        .contains("fn main() -> int {"));
}

#[test]
fn decompile_lifts_indirect_calls_without_not_yet_translated_warning() {
    // Script: CALLA 0x1234, CALLT 0x0001, RET
    let nef_bytes = build_nef(&[0x36, 0x34, 0x12, 0x37, 0x01, 0x00, 0x40]);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");

    assert!(
        high_level.contains("calla(0x1234)"),
        "CALLA should be lifted to an indirect-call statement: {high_level}"
    );
    assert!(
        high_level.contains("callt(0x0001)"),
        "CALLT should be lifted to an indirect-call statement: {high_level}"
    );
    assert!(
        !high_level.contains("not yet translated"),
        "indirect calls should no longer emit not-yet-translated placeholders: {high_level}"
    );
}

#[test]
fn decompile_lifts_relative_calls_without_control_flow_warning() {
    // Script: CALL +0, CALL_L +0, RET
    let nef_bytes = build_nef(&[0x34, 0x00, 0x35, 0x00, 0x00, 0x00, 0x00, 0x40]);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");

    assert!(
        high_level.contains("call_0x0002()"),
        "CALL should be lifted to a relative call placeholder: {high_level}"
    );
    assert!(
        high_level.contains("call_0x0007()"),
        "CALL_L should be lifted to a relative call placeholder: {high_level}"
    );
    assert!(
        !high_level.contains("control flow not yet lifted"),
        "relative calls should no longer use control-flow-not-lifted warnings: {high_level}"
    );
}

#[test]
fn decompile_lifts_unconditional_jumps_without_control_flow_warning() {
    // Script: JMP +0 (to JMP_L), JMP_L +0 (to RET), RET
    let nef_bytes = build_nef(&[0x22, 0x00, 0x23, 0x00, 0x00, 0x00, 0x00, 0x40]);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");

    assert!(
        high_level.contains("goto label_0x0002;"),
        "JMP should be lifted as a label-based transfer: {high_level}"
    );
    assert!(
        high_level.contains("goto label_0x0007;"),
        "JMP_L should be lifted as a label-based transfer: {high_level}"
    );
    assert!(
        high_level.contains("label_0x0002:"),
        "JMP target label should be emitted in output: {high_level}"
    );
    assert!(
        high_level.contains("label_0x0007:"),
        "JMP_L target label should be emitted in output: {high_level}"
    );
    assert!(
        !high_level.contains("control flow not yet lifted"),
        "unconditional jumps should no longer emit control-flow-not-lifted warnings: {high_level}"
    );
}

#[test]
fn decompile_lifts_endtry_transfers_without_control_flow_warning() {
    // Script: ENDTRY +0 (to ENDTRY_L), ENDTRY_L +0 (to RET), RET
    let nef_bytes = build_nef(&[0x3D, 0x00, 0x3E, 0x00, 0x00, 0x00, 0x00, 0x40]);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");

    assert!(
        high_level.contains("leave label_0x0002;"),
        "ENDTRY should be lifted as a label-based leave transfer: {high_level}"
    );
    assert!(
        high_level.contains("leave label_0x0007;"),
        "ENDTRY_L should be lifted as a label-based leave transfer: {high_level}"
    );
    assert!(
        high_level.contains("label_0x0002:"),
        "ENDTRY target label should be emitted in output: {high_level}"
    );
    assert!(
        high_level.contains("label_0x0007:"),
        "ENDTRY_L target label should be emitted in output: {high_level}"
    );
    assert!(
        !high_level.contains("control flow not yet lifted"),
        "ENDTRY opcodes should no longer emit control-flow-not-lifted warnings: {high_level}"
    );
}

#[test]
fn decompile_uses_label_style_for_unresolved_jump_targets() {
    // Script: JMP +3 (to 0x0005, no decoded instruction there), RET
    let nef_bytes = build_nef(&[0x22, 0x03, 0x40]);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");

    assert!(
        high_level.contains("goto label_0x0005;"),
        "unresolved jump target should still use label-style transfer naming: {high_level}"
    );
    assert!(
        !high_level.contains("goto_0x0005();"),
        "legacy function-style jump placeholder should not be emitted: {high_level}"
    );
}

#[test]
fn decompile_uses_label_style_for_unresolved_endtry_targets() {
    // Script: ENDTRY +3 (to 0x0005, no decoded instruction there), RET
    let nef_bytes = build_nef(&[0x3D, 0x03, 0x40]);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");

    assert!(
        high_level.contains("leave label_0x0005;"),
        "unresolved endtry target should still use label-style transfer naming: {high_level}"
    );
    assert!(
        !high_level.contains("leave_0x0005();"),
        "legacy function-style endtry placeholder should not be emitted: {high_level}"
    );
}
