use super::*;

#[test]
fn disassemble_bytes_returns_instruction_stream_without_rendering() {
    let nef_bytes = sample_nef();
    let output = Decompiler::new()
        .disassemble_bytes(&nef_bytes)
        .expect("disassembly succeeds");

    assert_eq!(output.instructions.len(), 4);
    assert!(output.warnings.is_empty());
    assert_eq!(output.instructions[0].offset, 0);
    assert_eq!(output.instructions[1].offset, 1);
}

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
fn decompile_uses_method_token_signature_for_callt_arguments_and_returns() {
    // Script: PUSH1; CALLT 0x0000; RET
    // Token: foo(param_count=1, returns=false)
    let nef_bytes = build_nef_with_single_token(
        &[0x11, 0x37, 0x00, 0x00, 0x40],
        [0u8; 20],
        "foo",
        1,
        false,
        0x0F,
    );
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("foo(t0);"),
        "CALLT should consume declared token argument and emit call expression: {high_level}"
    );
    assert!(
        !high_level.contains("let t1 = foo("),
        "CALLT token marked non-returning should not push a synthetic return temp: {high_level}"
    );
    assert!(
        high_level.contains("return;"),
        "script entry should end with a bare return after non-returning CALLT: {high_level}"
    );
}

#[test]
fn decompile_lifts_relative_calls_without_control_flow_warning() {
    // Script: CALL +2, CALL_L +5, RET
    let nef_bytes = build_nef(&[0x34, 0x02, 0x35, 0x05, 0x00, 0x00, 0x00, 0x40]);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");

    assert!(
        high_level.contains("sub_0x0002()"),
        "CALL should resolve to inferred method name: {high_level}"
    );
    assert!(
        high_level.contains("sub_0x0007()"),
        "CALL_L should resolve to inferred method name: {high_level}"
    );
    assert!(
        !high_level.contains("control flow not yet lifted"),
        "relative calls should no longer use control-flow-not-lifted warnings: {high_level}"
    );
}

#[test]
fn decompile_resolves_relative_call_target_to_inferred_method_name() {
    // Script layout:
    // 0x0000: CALL +5  (target = 0x0005)
    // 0x0002: RET
    // 0x0003..0x0004: NOP padding
    // 0x0005: INITSLOT 0,0
    // 0x0008: RET
    let nef_bytes = build_nef(&[
        0x34, 0x05, // CALL +5
        0x40, // RET
        0x21, 0x21, // NOP x2
        0x57, 0x00, 0x00, // INITSLOT 0,0
        0x40, // RET
    ]);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");

    assert!(
        high_level.contains("sub_0x0005()"),
        "relative CALL should use inferred callee name when target matches method start: {high_level}"
    );
}

#[test]
fn decompile_relative_call_passes_known_method_arguments() {
    // Script layout:
    // 0x0000: PUSH1
    // 0x0001: CALL +7  (target = 0x0008)
    // 0x0003: RET
    // 0x0004..0x0007: NOP padding
    // 0x0008: INITSLOT 0,1
    // 0x000B: LDARG0
    // 0x000C: RET
    let nef_bytes = build_nef(&[
        0x11, // PUSH1
        0x34, 0x07, // CALL +7
        0x40, // RET
        0x21, 0x21, 0x21, 0x21, // NOP x4
        0x57, 0x00, 0x01, // INITSLOT 0,1
        0x78, // LDARG0
        0x40, // RET
    ]);
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("sub_0x0008("),
        "relative CALL should target inferred method call syntax: {high_level}"
    );
    assert!(
        !high_level.contains("sub_0x0008()"),
        "relative CALL into one-arg method should pass argument expression: {high_level}"
    );
}

#[test]
fn decompile_lifts_unconditional_jumps_without_control_flow_warning() {
    // Script: JMP +2 (to JMP_L), JMP_L +5 (to RET), RET
    let nef_bytes = build_nef(&[0x22, 0x02, 0x23, 0x05, 0x00, 0x00, 0x00, 0x40]);
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
    // Script: ENDTRY +2 (to ENDTRY_L), ENDTRY_L +5 (to RET), RET
    let nef_bytes = build_nef(&[0x3D, 0x02, 0x3E, 0x05, 0x00, 0x00, 0x00, 0x40]);
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
    // Script: JMP +5 (to 0x0005, no decoded instruction there), RET
    let nef_bytes = build_nef(&[0x22, 0x05, 0x40]);
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
    // Script: ENDTRY +5 (to 0x0005, no decoded instruction there), RET
    let nef_bytes = build_nef(&[0x3D, 0x05, 0x40]);
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
fn decompile_resolves_pusha_calla_to_internal_call_placeholder() {
    // Script layout:
    // 0x0000: PUSHA +10  (target = 0x000A)
    // 0x0005: CALLA
    // 0x0006: RET
    // 0x0007..0x0009: NOP padding
    // 0x000A: INITSLOT 0,0
    // 0x000D: RET
    let nef_bytes = build_nef(&[
        0x0A, 0x0A, 0x00, 0x00, 0x00, // PUSHA +10
        0x36, // CALLA
        0x40, // RET
        0x21, 0x21, 0x21, // NOP x3
        0x57, 0x00, 0x00, // INITSLOT 0,0
        0x40, // RET
    ]);

    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("sub_0x000A()"),
        "PUSHA+CALLA should resolve to the inferred method name when available: {high_level}"
    );
    assert!(
        !high_level.contains("calla("),
        "resolved PUSHA+CALLA should not remain as generic indirect call: {high_level}"
    );
}

#[test]
fn decompile_resolves_local_pointer_flow_into_calla() {
    // Script layout:
    // 0x0000: PUSHA +9  (target = 0x0009)
    // 0x0005: STLOC0
    // 0x0006: LDLOC0
    // 0x0007: CALLA
    // 0x0008: RET
    // 0x0009: INITSLOT 0,0
    // 0x000C: RET
    let nef_bytes = build_nef(&[
        0x0A, 0x09, 0x00, 0x00, 0x00, // PUSHA +9
        0x70, // STLOC0
        0x68, // LDLOC0
        0x36, // CALLA
        0x40, // RET
        0x57, 0x00, 0x00, // INITSLOT 0,0
        0x40, // RET
    ]);

    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("sub_0x0009()"),
        "local pointer flow should resolve CALLA to inferred method name: {high_level}"
    );
    assert!(
        !high_level.contains("calla(loc0)"),
        "resolved local pointer flow should not remain generic CALLA: {high_level}"
    );
}

#[test]
fn decompile_resolves_static_pointer_flow_into_calla() {
    // Script layout:
    // 0x0000: PUSHA +9  (target = 0x0009)
    // 0x0005: STSFLD0
    // 0x0006: LDSFLD0
    // 0x0007: CALLA
    // 0x0008: RET
    // 0x0009: INITSLOT 0,0
    // 0x000C: RET
    let nef_bytes = build_nef(&[
        0x0A, 0x09, 0x00, 0x00, 0x00, // PUSHA +9
        0x60, // STSFLD0
        0x58, // LDSFLD0
        0x36, // CALLA
        0x40, // RET
        0x57, 0x00, 0x00, // INITSLOT 0,0
        0x40, // RET
    ]);

    let decompilation = Decompiler::new()
        .decompile_bytes(&nef_bytes)
        .expect("decompile succeeds");

    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    assert!(
        high_level.contains("sub_0x0009()"),
        "static pointer flow should resolve CALLA to inferred method name: {high_level}"
    );
    assert!(
        !high_level.contains("calla(static0)"),
        "resolved static pointer flow should not remain generic CALLA: {high_level}"
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
        high_level.contains("sub_0x") || high_level.contains("call_"),
        "sequential CALL instructions should each produce a call expression: {high_level}"
    );
}

#[test]
fn decompile_nested_loop_in_if() {
    // Script layout (offsets in brackets):
    //   [0] PUSH0          -- condition for outer if
    //   [1] JMPIFNOT +8    -- target = 9 (RET), outer if
    //   [3] PUSH0          -- condition for while loop
    //   [4] JMPIFNOT +5    -- target = 9 (RET), loop exit
    //   [6] NOP            -- loop body
    //   [7] JMP -4         -- back-edge to [3]
    //   [9] RET
    let nef_bytes = build_nef(&[
        0x10, // PUSH0
        0x26, 0x08, // JMPIFNOT +8
        0x10, // PUSH0
        0x26, 0x05, // JMPIFNOT +5
        0x21, // NOP
        0x22, 0xFC, // JMP -4
        0x40, // RET
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
    //   [1]  JMPIFNOT +12     -- target = 13 (RET), loop exit
    //   [3]  TRY (catch=+7, finally=0) -- catch at 10, no finally
    //   [6]  NOP              -- try body
    //   [7]  ENDTRY +6        -- leave to 13 (RET)
    //   [9]  NOP              -- padding
    //   [10] ENDFINALLY       -- catch handler
    //   [11] JMP -11          -- back-edge to [0]
    //   [13] RET
    let nef_bytes = build_nef(&[
        0x10, // PUSH0
        0x26, 0x0C, // JMPIFNOT +12
        0x3B, 0x07, 0x00, // TRY catch=+7, finally=0
        0x21, // NOP
        0x3D, 0x06, // ENDTRY +6
        0x21, // NOP
        0x3F, // ENDFINALLY
        0x22, 0xF5, // JMP -11
        0x40, // RET
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
    //   [1]  JMPIFNOT +10   -- target = 11 (RET), outer if
    //   [3]  PUSH0          -- condition for inner if
    //   [4]  JMPIFNOT +5    -- target = 9 (inner else body)
    //   [6]  NOP            -- inner if-true body
    //   [7]  JMP +4         -- skip inner else, target = 11 (RET)
    //   [9]  NOP            -- inner else body
    //   [10] NOP            -- padding
    //   [11] RET
    let nef_bytes = build_nef(&[
        0x10, // PUSH0
        0x26, 0x0A, // JMPIFNOT +10
        0x10, // PUSH0
        0x26, 0x05, // JMPIFNOT +5
        0x21, // NOP
        0x22, 0x04, // JMP +4
        0x21, // NOP
        0x21, // NOP
        0x40, // RET
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
        0x40, // RET
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
