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
    // Script: CALLA (no operand), CALLT 0x0001, RET
    let nef_bytes = build_nef(&[0x36, 0x37, 0x01, 0x00, 0x40]);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");

    assert!(
        high_level.contains("calla("),
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

#[test]
fn decompile_calla_with_stack_setup() {
    // Script: PUSH1 (push a value), PUSH0 (push pointer placeholder), CALLA, RET
    // Tests that CALLA consumes a pointer from the stack and emits an indirect call.
    let nef_bytes = build_nef(&[0x11, 0x10, 0x36, 0x40]);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("calla("),
        "CALLA should produce an indirect call expression: {high_level}"
    );
}

#[test]
fn decompile_multiple_sequential_calls() {
    // Script: CALL +2, CALL +0, RET, RET
    // Two sequential CALL instructions targeting different offsets.
    let nef_bytes = build_nef(&[0x34, 0x02, 0x34, 0x00, 0x40, 0x40]);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("call_"),
        "sequential CALL instructions should each produce a call placeholder: {high_level}"
    );
}

#[test]
fn decompile_nested_loop_in_if() {
    // Script layout (offsets in brackets):
    //   [0] PUSH0          -- condition for outer if
    //   [1] JMPIFNOT +6    -- target = 9 (RET), outer if
    //   [3] PUSH0          -- condition for while loop
    //   [4] JMPIFNOT +3    -- target = 9 (RET), loop exit
    //   [6] NOP            -- loop body
    //   [7] JMP -6         -- back-edge to [3]
    //   [9] RET
    let nef_bytes = build_nef(&[
        0x10,       // PUSH0
        0x26, 0x06, // JMPIFNOT +6
        0x10,       // PUSH0
        0x26, 0x03, // JMPIFNOT +3
        0x21,       // NOP
        0x22, 0xFA, // JMP -6
        0x40,       // RET
    ]);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");

    assert!(
        high_level.contains("if"),
        "outer branch should produce an if block: {high_level}"
    );
    assert!(
        high_level.contains("while"),
        "inner loop should produce a while block: {high_level}"
    );
    assert!(
        !high_level.contains("not yet translated"),
        "nested loop-in-if should not emit not-yet-translated placeholders: {high_level}"
    );
}

#[test]
fn decompile_try_in_loop() {
    // Script layout (offsets in brackets):
    //   [0]  PUSH0            -- condition for while loop
    //   [1]  JMPIFNOT +10     -- target = 13 (RET), loop exit
    //   [3]  TRY (catch=+4, finally=0) -- catch at 10, no finally
    //   [6]  NOP              -- try body
    //   [7]  ENDTRY +4        -- leave to 13 (RET)
    //   [9]  NOP              -- padding
    //   [10] ENDFINALLY       -- catch handler
    //   [11] JMP -13          -- back-edge to [0]
    //   [13] RET
    let nef_bytes = build_nef(&[
        0x10,             // PUSH0
        0x26, 0x0A,       // JMPIFNOT +10
        0x3B, 0x04, 0x00, // TRY catch=+4, finally=0
        0x21,             // NOP
        0x3D, 0x04,       // ENDTRY +4
        0x21,             // NOP
        0x3F,             // ENDFINALLY
        0x22, 0xF3,       // JMP -13
        0x40,             // RET
    ]);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");

    assert!(
        high_level.contains("try"),
        "try block should be emitted inside the loop: {high_level}"
    );
    assert!(
        high_level.contains("while") || high_level.contains("loop"),
        "enclosing loop should be recognized: {high_level}"
    );
}

#[test]
fn decompile_nested_if_else() {
    // Script layout (offsets in brackets):
    //   [0]  PUSH0          -- condition for outer if
    //   [1]  JMPIFNOT +8    -- target = 11 (RET), outer if
    //   [3]  PUSH0          -- condition for inner if
    //   [4]  JMPIFNOT +3    -- target = 9 (inner else body)
    //   [6]  NOP            -- inner if-true body
    //   [7]  JMP +2         -- skip inner else, target = 11 (RET)
    //   [9]  NOP            -- inner else body
    //   [10] NOP            -- padding
    //   [11] RET
    let nef_bytes = build_nef(&[
        0x10,       // PUSH0
        0x26, 0x08, // JMPIFNOT +8
        0x10,       // PUSH0
        0x26, 0x03, // JMPIFNOT +3
        0x21,       // NOP
        0x22, 0x02, // JMP +2
        0x21,       // NOP
        0x21,       // NOP
        0x40,       // RET
    ]);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");

    assert!(
        high_level.contains("if"),
        "nested if structure should be present: {high_level}"
    );
    assert!(
        high_level.contains("else {"),
        "inner if-else should produce an else branch: {high_level}"
    );
}

#[test]
fn decompile_all_comparison_jumps() {
    // Script layout: six comparison jumps (JMPEQ, JMPNE, JMPGT, JMPGE, JMPLT, JMPLE),
    // each preceded by PUSH0+PUSH1 to supply two stack operands, with a NOP filler
    // between each block. Each jump targets the next PUSH0 (or RET for the last).
    //
    //   [0]  PUSH0           [1]  PUSH1
    //   [2]  JMPEQ +1        target = 5
    //   [4]  NOP
    //   [5]  PUSH0           [6]  PUSH1
    //   [7]  JMPNE +1        target = 10
    //   [9]  NOP
    //   [10] PUSH0           [11] PUSH1
    //   [12] JMPGT +1        target = 15
    //   [14] NOP
    //   [15] PUSH0           [16] PUSH1
    //   [17] JMPGE +1        target = 20
    //   [19] NOP
    //   [20] PUSH0           [21] PUSH1
    //   [22] JMPLT +1        target = 25
    //   [24] NOP
    //   [25] PUSH0           [26] PUSH1
    //   [27] JMPLE +1        target = 30
    //   [29] NOP
    //   [30] RET
    let nef_bytes = build_nef(&[
        0x10, 0x11, 0x28, 0x01, 0x21, // PUSH0, PUSH1, JMPEQ +1, NOP
        0x10, 0x11, 0x2A, 0x01, 0x21, // PUSH0, PUSH1, JMPNE +1, NOP
        0x10, 0x11, 0x2C, 0x01, 0x21, // PUSH0, PUSH1, JMPGT +1, NOP
        0x10, 0x11, 0x2E, 0x01, 0x21, // PUSH0, PUSH1, JMPGE +1, NOP
        0x10, 0x11, 0x30, 0x01, 0x21, // PUSH0, PUSH1, JMPLT +1, NOP
        0x10, 0x11, 0x32, 0x01, 0x21, // PUSH0, PUSH1, JMPLE +1, NOP
        0x40,                          // RET
    ]);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");

    for op in ["==", "!=", ">", ">=", "<", "<="] {
        assert!(
            high_level.contains(op),
            "comparison operator {op} should appear in output: {high_level}"
        );
    }
    assert!(
        !high_level.contains("not yet translated"),
        "comparison jumps should not emit not-yet-translated placeholders: {high_level}"
    );
}
