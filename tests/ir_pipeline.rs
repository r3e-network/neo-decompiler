//! End-to-end validation of the IR-spine pipeline (`--format ir`) on real bytecode.
//!
//! These build real NEF containers (or reuse a real artifact) and run the full
//! lift → SSA → optimize → structure → render path, asserting that each
//! control-flow construct is recovered. This complements the structurer unit
//! tests (which use hand-built SSA) by exercising the whole pipeline.

#![allow(clippy::unwrap_used)]

use std::fs;

use neo_decompiler::{Decompiler, NefParser, OutputFormat, ReturnBehavior};

fn repo_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Minimal NEF3 wrapper around a script (mirrors the in-crate test helper).
fn build_nef(script: &[u8]) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(b"NEF3");
    let mut compiler = [0u8; 64];
    compiler[..4].copy_from_slice(b"test");
    data.extend_from_slice(&compiler);
    data.push(0); // source (empty varstring)
    data.push(0); // reserved byte
    data.push(0); // method token count
    data.extend_from_slice(&0u16.to_le_bytes()); // reserved word
    write_varint(&mut data, script.len() as u32);
    data.extend_from_slice(script);
    let checksum = NefParser::calculate_checksum(&data);
    data.extend_from_slice(&checksum.to_le_bytes());
    data
}

fn build_nef_with_token(
    script: &[u8],
    method: &str,
    parameter_count: u16,
    has_return_value: bool,
) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(b"NEF3");
    let mut compiler = [0u8; 64];
    compiler[..4].copy_from_slice(b"test");
    data.extend_from_slice(&compiler);
    data.push(0); // source (empty varstring)
    data.push(0); // reserved byte
    data.push(1); // method token count
    data.extend_from_slice(&[0u8; 20]);
    write_varint(&mut data, method.len() as u32);
    data.extend_from_slice(method.as_bytes());
    data.extend_from_slice(&parameter_count.to_le_bytes());
    data.push(u8::from(has_return_value));
    data.push(0x0F); // CallFlags::All
    data.extend_from_slice(&0u16.to_le_bytes()); // reserved word
    write_varint(&mut data, script.len() as u32);
    data.extend_from_slice(script);
    let checksum = NefParser::calculate_checksum(&data);
    data.extend_from_slice(&checksum.to_le_bytes());
    data
}

#[test]
fn structured_ir_decodes_signed_wide_integer() {
    let mut script = vec![neo_decompiler::OpCode::Pushint128.byte()];
    script.extend([0xFF; 16]);
    script.push(neo_decompiler::OpCode::Ret.byte());
    let nef = build_nef(&script);
    let mut decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef, None, OutputFormat::All)
        .expect("wide integer script decompiles");

    let ir = decompilation.render_structured_ir();

    assert!(
        ir.contains("-1"),
        "signed PUSHINT128 must decode as -1:\n{ir}"
    );
    assert!(
        !ir.contains("ffffffffffffffffffffffffffffffff"),
        "wide integer bytes must not leak as decimal content:\n{ir}"
    );
}

#[test]
fn structured_ir_recovers_pack_families_and_constant_unpack() {
    let cases = [
        (
            vec![
                neo_decompiler::OpCode::Push2.byte(),
                neo_decompiler::OpCode::Push1.byte(),
                neo_decompiler::OpCode::Push2.byte(),
                neo_decompiler::OpCode::Pack.byte(),
                neo_decompiler::OpCode::Ret.byte(),
            ],
            "[1, 2]",
        ),
        (
            vec![
                neo_decompiler::OpCode::Push2.byte(),
                neo_decompiler::OpCode::Push1.byte(),
                neo_decompiler::OpCode::Push2.byte(),
                neo_decompiler::OpCode::Packstruct.byte(),
                neo_decompiler::OpCode::Ret.byte(),
            ],
            "struct[1, 2]",
        ),
        (
            vec![
                neo_decompiler::OpCode::Push4.byte(),
                neo_decompiler::OpCode::Push3.byte(),
                neo_decompiler::OpCode::Push2.byte(),
                neo_decompiler::OpCode::Push1.byte(),
                neo_decompiler::OpCode::Push2.byte(),
                neo_decompiler::OpCode::Packmap.byte(),
                neo_decompiler::OpCode::Ret.byte(),
            ],
            "{1: 2, 3: 4}",
        ),
        (
            vec![
                neo_decompiler::OpCode::Push2.byte(),
                neo_decompiler::OpCode::Push1.byte(),
                neo_decompiler::OpCode::Push2.byte(),
                neo_decompiler::OpCode::Pack.byte(),
                neo_decompiler::OpCode::Unpack.byte(),
                neo_decompiler::OpCode::Drop.byte(),
                neo_decompiler::OpCode::Sub.byte(),
                neo_decompiler::OpCode::Ret.byte(),
            ],
            "return 1;",
        ),
    ];

    for (script, expected) in cases {
        let nef = build_nef(&script);
        let mut decompilation = Decompiler::new()
            .decompile_bytes_with_manifest(&nef, None, OutputFormat::All)
            .expect("collection script decompiles");

        let ir = decompilation.render_structured_ir();

        assert!(ir.contains(expected), "expected {expected:?}:\n{ir}");
        assert!(
            !ir.contains('?'),
            "valid constant collection must be exact:\n{ir}"
        );
    }
}

fn write_varint(buf: &mut Vec<u8>, value: u32) {
    match value {
        0x00..=0xFC => buf.push(value as u8),
        0xFD..=0xFFFF => {
            buf.push(0xFD);
            buf.extend_from_slice(&(value as u16).to_le_bytes());
        }
        _ => {
            buf.push(0xFE);
            buf.extend_from_slice(&value.to_le_bytes());
        }
    }
}

fn ir_for(nef: &[u8]) -> String {
    let mut dec = Decompiler::new()
        .decompile_bytes_with_manifest(nef, None, OutputFormat::All)
        .unwrap();
    dec.render_structured_ir()
}

#[test]
fn structured_ir_preserves_setitem_mutation() {
    let nef = build_nef(&[0xC8, 0x11, 0x12, 0xD0, 0x40]);
    let ir = ir_for(&nef);

    let constructor = ir
        .find("newmap()")
        .unwrap_or_else(|| panic!("map constructor must remain visible:\n{ir}"));
    let mutation = ir
        .find("set_item(t_0, 1, 2);")
        .unwrap_or_else(|| panic!("SETITEM must remain visible:\n{ir}"));

    assert!(
        constructor < mutation,
        "map constructor must precede its mutation:\n{ir}"
    );
    assert!(
        ir.contains("return;"),
        "SETITEM must not manufacture a return value:\n{ir}"
    );
}

#[test]
fn structured_ir_preserves_assert_and_message() {
    let assert_ir = ir_for(&build_nef(&[0x11, 0x39, 0x40]));
    let assert_message_ir = ir_for(&build_nef(&[0x11, 0x12, 0xE1, 0x40]));

    assert!(assert_ir.contains("assert(1);"), "{assert_ir}");
    assert!(
        assert_message_ir.contains("assert(1, 2);"),
        "condition must precede its message:\n{assert_message_ir}"
    );
    assert!(!assert_ir.contains("return/throw/abort"), "{assert_ir}");
    assert!(
        !assert_message_ir.contains("return/throw/abort"),
        "{assert_message_ir}"
    );
}

#[test]
fn structured_ir_routes_natural_endtry_through_finally() {
    // INITSLOT; TRY finally@10; loc0=1; ENDTRY continuation@13;
    // finally { loc0=2; }; return loc0.
    let nef = build_nef(&[
        0x57, 0x01, 0x00, 0x3B, 0x00, 0x07, 0x11, 0x70, 0x3D, 0x05, 0x12, 0x70, 0x3F, 0x68, 0x40,
    ]);
    let manifest = neo_decompiler::ContractManifest::from_json_str(
        r#"{
            "name": "NaturalFinally",
            "abi": { "methods": [{
                "name": "main", "parameters": [], "returntype": "Integer", "offset": 0
            }] }
        }"#,
    )
    .unwrap();
    let mut decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef, Some(manifest), OutputFormat::All)
        .expect("try/finally script decompiles");

    let ir = decompilation.render_structured_ir();

    assert!(ir.contains("try {\n            loc0 = 1;"), "{ir}");
    assert!(ir.contains("finally {\n            loc0 = 2;"), "{ir}");
    assert!(ir.contains("return loc0;"), "{ir}");
    assert!(!ir.contains("ENDFINALLY"), "{ir}");
}

#[test]
fn structured_ir_routes_return_through_nested_finally() {
    // INITSLOT; outer TRY; inner TRY; return 1 through both finally regions.
    // The compiler places the outer ENDTRY before the inner finally and the
    // single physical RET after the outer finally.
    let nef = build_nef(&[
        0x57, 0x01, 0x00, 0x3B, 0x00, 0x0E, 0x3B, 0x00, 0x08, 0x11, 0x3D, 0x02, 0x3D, 0x08, 0x12,
        0x70, 0x3F, 0x13, 0x70, 0x3F, 0x40,
    ]);
    let manifest = neo_decompiler::ContractManifest::from_json_str(
        r#"{
            "name": "NestedReturnFinally",
            "abi": { "methods": [{
                "name": "main", "parameters": [], "returntype": "Integer", "offset": 0
            }] }
        }"#,
    )
    .unwrap();
    let mut decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef, Some(manifest), OutputFormat::All)
        .expect("nested return/finally script decompiles");

    let ir = decompilation.render_structured_ir();

    assert_eq!(ir.matches("finally {").count(), 2, "{ir}");
    let captured_return = ir
        .find("return 1;")
        .unwrap_or_else(|| panic!("captured return value:\n{ir}"));
    let inner_finally = ir.find("finally {").expect("inner finally");
    assert!(captured_return < inner_finally, "{ir}");
    assert!(
        !ir.contains("goto label_") && !ir.contains("return ?;"),
        "{ir}"
    );
    assert!(!ir.contains("ENDFINALLY"), "{ir}");
}

#[test]
fn structured_ir_distinguishes_throw_abort_and_abort_message() {
    let throw_ir = ir_for(&build_nef(&[0x11, 0x3A]));
    let abort_ir = ir_for(&build_nef(&[0x38]));
    let abort_message_ir = ir_for(&build_nef(&[0x12, 0xE0]));

    assert!(throw_ir.contains("throw(1);"), "{throw_ir}");
    assert!(!throw_ir.contains("abort("), "{throw_ir}");
    assert!(abort_ir.contains("abort();"), "{abort_ir}");
    assert!(abort_message_ir.contains("abort(2);"), "{abort_message_ir}");
}

#[test]
fn structured_ir_seeds_catch_exception_value() {
    // TRY catch@6; NOP; ENDTRY -> RET; catch starts by consuming the VM's
    // implicit exception object, then leaves through its own ENDTRY.
    let nef = build_nef(&[0x3B, 0x06, 0x00, 0x21, 0x3D, 0x05, 0x45, 0x3D, 0x02, 0x40]);
    let manifest = neo_decompiler::ContractManifest::from_json_str(
        r#"{
            "name": "CatchState",
            "abi": { "methods": [{
                "name": "main", "parameters": [], "returntype": "Void", "offset": 0
            }] }
        }"#,
    )
    .unwrap();
    let mut decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef, Some(manifest), OutputFormat::All)
        .expect("catch script decompiles");

    let ir = decompilation.render_structured_ir();

    assert!(
        ir.contains("catch(exception_b2_0)"),
        "catch header must expose the implicit exception SSA value:\n{ir}"
    );
    assert!(
        !ir.contains('?'),
        "the implicit exception value must not underflow:\n{ir}"
    );
}

#[test]
fn structured_ir_keeps_value_shared_by_assert_and_branch_defined() {
    // GetTime; DUP; ASSERT; JMPIF then; else return 1; then return 2.
    // ASSERT consumes the duplicate while the branch consumes the original,
    // so both statements must share one materialized syscall result.
    let ir = ir_for(&build_nef(&[
        0x41, 0xB7, 0xC3, 0x88, 0x03, 0x4A, 0x39, 0x24, 0x04, 0x11, 0x40, 0x12, 0x40,
    ]));

    assert!(
        ir.contains("t_0 = syscall(\"System.Runtime.GetTime\");"),
        "shared condition must retain its definition:\n{ir}"
    );
    assert!(ir.contains("assert(t_0);"), "{ir}");
    assert!(ir.contains("if (t_0)"), "{ir}");
    assert_eq!(
        ir.matches("syscall(\"System.Runtime.GetTime\")").count(),
        1,
        "shared condition must evaluate the syscall exactly once:\n{ir}"
    );
}

#[test]
fn structured_ir_keeps_assert_in_do_while_latch() {
    // Entry jumps to a bottom-tested latch. The latch duplicates its condition,
    // asserts the duplicate, then branches back to the loop body.
    let ir = ir_for(&build_nef(&[
        0x21, 0x22, 0x02, 0x08, 0x4A, 0x39, 0x24, 0xFA, 0x40,
    ]));

    let loop_start = ir
        .find("do {")
        .unwrap_or_else(|| panic!("expected do-while loop:\n{ir}"));
    let assertion = ir
        .find("assert(true);")
        .unwrap_or_else(|| panic!("latch assertion must remain visible:\n{ir}"));
    let loop_test = ir
        .find("} while (true);")
        .unwrap_or_else(|| panic!("expected bottom loop test:\n{ir}"));

    assert!(
        loop_start < assertion && assertion < loop_test,
        "latch assertion must execute inside the loop before its test:\n{ir}"
    );
    assert_eq!(ir.matches("assert(true);").count(), 1, "{ir}");
}

#[test]
fn structured_ir_rechecks_assert_in_while_header() {
    // The finite loop header calls GetTime, asserts the result, and evaluates
    // its branch condition. A source-level while must replay that header at the
    // body tail so each backedge performs the same call and assertion.
    let ir = ir_for(&build_nef(&[
        0x22, 0x02, 0x41, 0xB7, 0xC3, 0x88, 0x03, 0x39, 0x08, 0x24, 0x03, 0x40, 0x21, 0x22, 0xF5,
    ]));

    let loop_start = ir
        .find("while (")
        .unwrap_or_else(|| panic!("expected finite while loop:\n{ir}"));
    let assertions: Vec<_> = ir.match_indices("assert(t_0);").collect();

    assert_eq!(
        assertions.len(),
        2,
        "loop header assertion must run before the first test and on each backedge:\n{ir}"
    );
    assert!(
        assertions[0].0 < loop_start && loop_start < assertions[1].0,
        "the replayed assertion must remain inside the loop body:\n{ir}"
    );
    assert_eq!(
        ir.matches("syscall(\"System.Runtime.GetTime\")").count(),
        2,
        "the assertion condition must be recomputed on each header visit:\n{ir}"
    );
}

#[test]
fn structured_ir_keeps_assert_in_switch_comparison_block() {
    // The second equality test has an ASSERT prelude. Switch promotion cannot
    // represent that path-local effect, so the if/else form must be retained.
    let ir = ir_for(&build_nef(&[
        0x57, 0x01, 0x00, 0x11, 0x70, 0x78, 0x10, 0x97, 0x26, 0x06, 0x1A, 0x70, 0x22, 0x0F, 0x08,
        0x39, 0x78, 0x11, 0x97, 0x26, 0x06, 0x1B, 0x70, 0x22, 0x04, 0x1C, 0x70, 0x68, 0x40,
    ]));

    assert!(
        ir.contains("assert(true);"),
        "comparison-block assertion must remain visible:\n{ir}"
    );
    assert_eq!(ir.matches("assert(true);").count(), 1, "{ir}");
    assert!(
        !ir.contains("switch ("),
        "path-local assertion requires if/else structuring:\n{ir}"
    );
}

/// Like `ir_for`, but loads the sidecar `.manifest.json` so the per-method +
/// envelope path can pick up class name and ABI methods.
fn ir_with_manifest_for(nef_path: &std::path::Path) -> String {
    let data = fs::read(nef_path).unwrap();
    let manifest_path = nef_path.with_extension("manifest.json");
    let manifest = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|e| panic!("missing sidecar manifest {}: {e}", manifest_path.display()));
    let manifest = neo_decompiler::manifest::ContractManifest::from_json_str(&manifest)
        .unwrap_or_else(|e| panic!("invalid manifest {}: {e}", manifest_path.display()));
    let mut dec = Decompiler::new()
        .decompile_bytes_with_manifest(&data, Some(manifest), OutputFormat::All)
        .unwrap();
    dec.render_structured_ir()
}

#[test]
fn ir_pipeline_recovers_a_switch_from_real_bytecode() {
    // Neo C# `switch` lowering (an equality cascade on one scrutinee):
    //   switch (arg0) { case 0: loc0 = 10; case 1: loc0 = 11; default: loc0 = 12; }
    //   return loc0;
    //
    // The scrutinee MUST be a non-constant (arg0, a function input). An earlier
    // form used `loc0 = 1; switch (loc0)`, but once locals became versioned SSA
    // variables the optimizer correctly constant-folded `1 == 0` / `1 == 1` and
    // dissolved the cascade — the honest output for that dead code is *not* a
    // switch. Switching on arg0 keeps the comparisons non-foldable so the
    // structurer's switch recovery is genuinely exercised, and the case bodies
    // carry their stored constants once locals flow as SSA values.
    let script = [
        0x57, 0x01, 0x00, // INITSLOT 1 local, 0 args
        0x11, 0x70, // PUSH1; STLOC0 (dead init — the switch is on arg0, not loc0)
        0x78, 0x10, 0x97, // LDARG0; PUSH0; EQUAL
        0x26, 0x06, // JMPIFNOT +6 -> else branch
        0x1A, 0x70, // PUSH10; STLOC0
        0x22, 0x0D, // JMP +13 -> end
        0x78, 0x11, 0x97, // LDARG0; PUSH1; EQUAL
        0x26, 0x06, // JMPIFNOT +6 -> else branch
        0x1B, 0x70, // PUSH11; STLOC0
        0x22, 0x04, // JMP +4 -> end
        0x1C, 0x70, // PUSH12; STLOC0
        0x68, 0x40, // LDLOC0; RET
    ];
    let nef = build_nef(&script);
    let ir = ir_for(&nef);
    assert!(
        ir.contains("switch ("),
        "the IR pipeline should recover a switch from the equality cascade; got:\n{ir}"
    );
    assert!(
        ir.contains("case "),
        "the switch should render its cases; got:\n{ir}"
    );
    assert!(
        ir.contains("10") && ir.contains("11") && ir.contains("12"),
        "switch case bodies should carry the stored constants (10/11/12); got:\n{ir}"
    );
}

#[test]
fn ir_pipeline_recovers_an_if_from_a_real_artifact() {
    // LoopIf.nef has a conditional branch over the loop body; the IR structurer
    // should surface an `if` construct (or `while`/`do-while`) rather than a
    // flat block.
    let root = repo_root();
    let nef = fs::read(root.join("TestingArtifacts/edgecases/LoopIf.nef")).unwrap();
    let ir = ir_for(&nef);
    assert!(
        ir.contains("if (")
            || ir.contains("while (")
            || ir.contains("for (")
            || ir.contains("do {"),
        "LoopIf has control flow the structurer should recover; got:\n{ir}"
    );
}

#[test]
fn ir_pipeline_is_well_formed_across_artifacts() {
    // Every successfully-decompiled artifact must yield balanced-brace IR.
    let root = repo_root();
    for nef_path in fs::read_dir(root.join("TestingArtifacts"))
        .unwrap()
        .flatten()
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("nef"))
        .map(|e| e.path())
    {
        let data = match fs::read(&nef_path) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let Ok(mut dec) =
            Decompiler::new().decompile_bytes_with_manifest(&data, None, OutputFormat::All)
        else {
            continue;
        };
        let ir = dec.render_structured_ir();
        let open = ir.chars().filter(|&c| c == '{').count();
        let close = ir.chars().filter(|&c| c == '}').count();
        assert_eq!(
            open,
            close,
            "IR for {} must have balanced braces:\n{ir}",
            nef_path.display()
        );
    }
}

#[test]
fn ir_pipeline_renders_per_method_envelope_for_multimethod() {
    // MultiMethod.nef's manifest declares two methods (main@0, helper@6), but
    // the bytecode's helper is unreachable from main's RET (dead code after the
    // return). Per-method CFG construction must still render both independent
    // ABI bodies; whole-script reachability must not erase helper@6.
    let root = repo_root();
    let nef_path = root.join("TestingArtifacts/edgecases/multi/MultiMethod.nef");
    let ir = ir_with_manifest_for(&nef_path);
    assert!(
        ir.contains("contract MultiMethod {"),
        "envelope must wrap the IR in the legacy contract header; got:\n{ir}"
    );
    assert!(
        ir.contains("fn main() -> int;") && ir.contains("fn helper() -> int;"),
        "envelope ABI must list both declared methods; got:\n{ir}"
    );
    assert!(
        ir.contains("fn main() -> int {"),
        "envelope must render the reachable method's body as `fn main() -> int {{`; got:\n{ir}"
    );
    assert!(
        ir.contains("fn helper() -> int {"),
        "envelope must render the detached ABI helper's body as `fn helper() -> int {{`; got:\n{ir}"
    );
    assert!(
        ir.contains("return 2;"),
        "helper's PUSH1 + PUSH1 + ADD result must survive SSA as a structured return; got:\n{ir}"
    );
    assert!(
        ir.trim_end().ends_with('}'),
        "envelope must close the contract block with `}}`; got:\n{ir}"
    );
}

#[test]
fn ir_pipeline_loopif_envelope_preserves_while_loop() {
    // LoopIf.nef is a single-method artifact; the per-method envelope must keep
    // the structurer's `while` recovery intact inside the contract wrapper.
    let root = repo_root();
    let nef_path = root.join("TestingArtifacts/edgecases/LoopIf.nef");
    let ir = ir_with_manifest_for(&nef_path);
    assert!(
        ir.contains("contract LoopIf {"),
        "envelope must wrap LoopIf in its contract header; got:\n{ir}"
    );
    assert!(
        ir.contains("while (") || ir.contains("for (") || ir.contains("do {"),
        "envelope must preserve the structurer's loop recovery; got:\n{ir}"
    );
    assert!(
        ir.trim_end().ends_with('}'),
        "envelope must close the contract block with `}}`; got:\n{ir}"
    );
}

#[test]
fn structured_ir_deversions_source_slots_before_phi_lowering() {
    let root = repo_root();
    let nef_path = root.join("TestingArtifacts/edgecases/LoopIf.nef");
    let ir = ir_with_manifest_for(&nef_path);

    assert!(
        ir.contains("loc0 ="),
        "local slot must remain visible:\n{ir}"
    );
    assert!(
        !ir.contains("loc0_"),
        "all SSA versions of local slot zero must share one source name:\n{ir}"
    );
    assert!(
        !ir.contains("_copy_tmp_"),
        "de-versioning must happen before phi copy scheduling:\n{ir}"
    );
}

#[test]
fn ir_pipeline_recovers_loopif_counting_loop() {
    // LoopIf's back-edge re-enters the PUSH0/STLOC initializer, so both branch
    // outcomes stay inside an unconditional natural loop. High-level recovery
    // still lifts counting-loop intent into for/while with init not defeating
    // the condition inside `while (true)`.
    let root = repo_root();
    let nef_path = root.join("TestingArtifacts/edgecases/LoopIf.nef");
    let ir = ir_with_manifest_for(&nef_path);

    assert!(
        !ir.contains("while (true)"),
        "counting-loop recovery must not leave while(true):\n{ir}"
    );
    assert!(
        ir.contains("for (loc0 = 0;")
            || (ir.contains("loc0 = 0;")
                && (ir.contains("while ((loc0") || ir.contains("while (loc0"))),
        "expected for/while counting loop on loc0:\n{ir}"
    );
    assert!(
        ir.contains("loc0 < 3") || ir.contains("(loc0 < 3)"),
        "loop condition must retain the bound comparison:\n{ir}"
    );
    // Body after the loop header must not re-assign the zero initializer.
    let header_at = ir
        .find("for (")
        .or_else(|| ir.find("while ("))
        .unwrap_or_else(|| panic!("structured loop header:\n{ir}"));
    let body_at = ir[header_at..]
        .find('{')
        .map(|offset| header_at + offset)
        .unwrap_or_else(|| panic!("loop body open:\n{ir}"));
    assert!(
        !ir[body_at..].contains("loc0 = 0"),
        "initializer must not re-execute inside the loop body:\n{ir}"
    );
}

#[test]
fn structured_ir_uses_manifest_parameter_names_in_signature_and_body() {
    // INITSLOT 0 locals, 2 args; LDARG1; RET.
    let nef = build_nef(&[0x57, 0x00, 0x02, 0x79, 0x40]);
    let manifest = neo_decompiler::ContractManifest::from_json_str(
        r#"{
            "name": "ParameterNames",
            "abi": {
                "methods": [{
                    "name": "choose",
                    "parameters": [
                        { "name": "from", "type": "Hash160" },
                        { "name": "amount", "type": "Integer" }
                    ],
                    "returntype": "Integer",
                    "offset": 0
                }]
            }
        }"#,
    )
    .unwrap();
    let mut dec = Decompiler::new()
        .decompile_bytes_with_manifest(&nef, Some(manifest), OutputFormat::All)
        .unwrap();

    let ir = dec.render_structured_ir();

    assert!(
        ir.contains("fn choose(from: hash160, amount: int) -> int {"),
        "structured method signature must use manifest parameters:\n{ir}"
    );
    assert!(
        ir.contains("return amount;"),
        "structured body must use the source parameter name:\n{ir}"
    );
    assert!(
        !ir.contains("ldarg1()") && !ir.contains("arg1_"),
        "structured body must not expose VM argument-load artifacts:\n{ir}"
    );
}

#[test]
fn structured_ir_disambiguates_parameter_from_generated_ssa_name() {
    // INITSLOT 1,1; loc0 = 1; return loc0 + arg0. The manifest deliberately
    // names arg0 `loc0_0`, which is also the generated display name of the
    // first local definition.
    let nef = build_nef(&[0x57, 0x01, 0x01, 0x11, 0x70, 0x68, 0x78, 0x9E, 0x40]);
    let manifest = neo_decompiler::ContractManifest::from_json_str(
        r#"{
            "name": "NameCollision",
            "abi": {
                "methods": [{
                    "name": "sum",
                    "parameters": [{ "name": "loc0_0", "type": "Integer" }],
                    "returntype": "Integer",
                    "offset": 0
                }]
            }
        }"#,
    )
    .unwrap();
    let mut dec = Decompiler::new()
        .decompile_bytes_with_manifest(&nef, Some(manifest), OutputFormat::All)
        .unwrap();

    let ir = dec.render_structured_ir();

    assert!(
        ir.contains("fn sum(loc0_0: int) -> int"),
        "manifest parameter name must remain stable in the signature:\n{ir}"
    );
    assert!(
        ir.contains("loc0 + loc0_0"),
        "generated local must be renamed away from the source parameter:\n{ir}"
    );
}

#[test]
fn structured_ir_keeps_recursive_private_return_contract_unknown() {
    let nef = build_nef(&[0x19, 0x34, 0x05, 0x40, 0x21, 0x21, 0x34, 0x00, 0x40]);
    let manifest = neo_decompiler::ContractManifest::from_json_str(
        r#"{
            "name": "RecursiveHelper",
            "abi": { "methods": [{
                "name": "main", "parameters": [], "returntype": "Integer", "offset": 0
            }] }
        }"#,
    )
    .unwrap();
    let mut dec = Decompiler::new()
        .decompile_bytes_with_manifest(&nef, Some(manifest), OutputFormat::All)
        .unwrap();

    assert_eq!(
        dec.method_contracts
            .get(6)
            .map(|contract| contract.return_behavior),
        Some(ReturnBehavior::Unknown)
    );

    let ir = dec.render_structured_ir();
    assert!(
        ir.contains("fn sub_0x0006() -> any") && ir.contains("return sub_0x0006();"),
        "unknown recursive helper must remain conservatively value-producing:\n{ir}"
    );
}

#[test]
fn structured_ir_uses_resolved_internal_call_contract() {
    // main: PUSH2 (right arg), PUSH1 (left arg), CALL helper@6, RET.
    // helper: INITSLOT 0,2; LDARG0; LDARG1; ADD; RET.
    let nef = build_nef(&[
        0x12, 0x11, 0x34, 0x04, 0x40, 0x21, 0x57, 0x00, 0x02, 0x78, 0x79, 0x9E, 0x40,
    ]);
    let manifest = neo_decompiler::ContractManifest::from_json_str(
        r#"{
            "name": "TypedValueCall",
            "abi": {
                "methods": [
                    {
                        "name": "main",
                        "parameters": [],
                        "returntype": "Integer",
                        "offset": 0
                    },
                    {
                        "name": "helper",
                        "parameters": [
                            { "name": "left", "type": "Integer" },
                            { "name": "right", "type": "Integer" }
                        ],
                        "returntype": "Integer",
                        "offset": 6
                    }
                ]
            }
        }"#,
    )
    .unwrap();
    let mut dec = Decompiler::new()
        .decompile_bytes_with_manifest(&nef, Some(manifest), OutputFormat::All)
        .unwrap();

    let ir = dec.render_structured_ir();

    assert!(
        ir.contains("helper(1, 2)"),
        "resolved internal call must use the ABI name and source argument order:\n{ir}"
    );
    assert!(
        !ir.contains("call_0x"),
        "resolved internal call must not retain an offset placeholder:\n{ir}"
    );
}

#[test]
fn structured_ir_applies_inferred_contract_to_helper_definition() {
    // main: PUSH1; CALL helper@6; RET. helper is absent from the manifest but
    // declares one argument and returns it.
    let nef = build_nef(&[
        0x11, 0x34, 0x05, 0x40, 0x21, 0x21, 0x57, 0x00, 0x01, 0x78, 0x40,
    ]);
    let manifest = neo_decompiler::ContractManifest::from_json_str(
        r#"{
            "name": "InferredHelper",
            "abi": { "methods": [{
                "name": "main", "parameters": [], "returntype": "Integer", "offset": 0
            }] }
        }"#,
    )
    .unwrap();
    let mut dec = Decompiler::new()
        .decompile_bytes_with_manifest(&nef, Some(manifest), OutputFormat::All)
        .unwrap();

    let ir = dec.render_structured_ir();

    assert!(
        ir.contains("sub_0x0006(1)"),
        "call site must use the inferred helper contract:\n{ir}"
    );
    assert!(
        ir.contains("fn sub_0x0006(arg0: any) -> any"),
        "helper definition must use the same inferred arity and return contract:\n{ir}"
    );
    assert!(
        ir.contains("return arg0;"),
        "inferred argument must be seeded in the helper body:\n{ir}"
    );
}

#[test]
fn structured_ir_infers_private_void_helper_without_phantom_result() {
    // main: ambient PUSH9; argument PUSH1; CALL helper@7; RET. helper consumes
    // its argument and returns no value.
    let nef = build_nef(&[
        0x19, 0x11, 0x34, 0x05, 0x40, 0x21, 0x21, 0x57, 0x00, 0x01, 0x78, 0x45, 0x40,
    ]);
    let manifest = neo_decompiler::ContractManifest::from_json_str(
        r#"{
            "name": "InferredVoidHelper",
            "abi": { "methods": [{
                "name": "main", "parameters": [], "returntype": "Integer", "offset": 0
            }] }
        }"#,
    )
    .unwrap();
    let mut dec = Decompiler::new()
        .decompile_bytes_with_manifest(&nef, Some(manifest), OutputFormat::All)
        .unwrap();

    assert_eq!(
        dec.method_contracts
            .get(7)
            .map(|contract| contract.return_behavior),
        Some(ReturnBehavior::Void)
    );

    let ir = dec.render_structured_ir();

    assert!(
        ir.contains("sub_0x0007(1);") && ir.contains("return 9;"),
        "private void call must preserve the caller's ambient value:\n{ir}"
    );
    assert!(
        ir.contains("fn sub_0x0007(arg0: any) -> void"),
        "private helper definition must use inferred void return behavior:\n{ir}"
    );
    assert!(
        !ir.contains("= sub_0x0007(1)") && !ir.contains("return sub_0x0007(1)"),
        "private void helper must not manufacture a result:\n{ir}"
    );
}

#[test]
fn structured_ir_infers_void_return_through_private_wrapper_chain() {
    // main preserves ambient 9 while calling wrapper@6; wrapper calls leaf@10;
    // leaf returns void. Return inference must resolve the private call graph
    // from the leaf upward rather than treating wrapper's CALL as opaque.
    let nef = build_nef(&[
        0x19, 0x34, 0x05, 0x40, 0x21, 0x21, 0x34, 0x04, 0x40, 0x21, 0x40,
    ]);
    let manifest = neo_decompiler::ContractManifest::from_json_str(
        r#"{
            "name": "VoidWrapperChain",
            "abi": { "methods": [{
                "name": "main", "parameters": [], "returntype": "Integer", "offset": 0
            }] }
        }"#,
    )
    .unwrap();
    let mut dec = Decompiler::new()
        .decompile_bytes_with_manifest(&nef, Some(manifest), OutputFormat::All)
        .unwrap();

    assert_eq!(
        [6, 10].map(|offset| {
            dec.method_contracts
                .get(offset)
                .map(|contract| contract.return_behavior)
        }),
        [Some(ReturnBehavior::Void), Some(ReturnBehavior::Void)]
    );

    let ir = dec.render_structured_ir();

    assert!(
        ir.contains("sub_0x0006();") && ir.contains("sub_0x000A();"),
        "private wrapper chain must remain visible:\n{ir}"
    );
    assert!(
        ir.contains("return 9;")
            && ir.contains("fn sub_0x0006() -> void")
            && ir.contains("fn sub_0x000A() -> void"),
        "void inference must propagate from leaf to wrapper and caller:\n{ir}"
    );
    assert!(
        !ir.contains("= sub_0x0006()") && !ir.contains("= sub_0x000A()"),
        "void private calls must not manufacture results:\n{ir}"
    );
}

#[test]
fn structured_ir_return_inference_keeps_unconsumed_entry_argument() {
    // main calls helper@5 with two direct-stack arguments. The helper swaps
    // them, passes one to a void token call, and returns the other. Return
    // inference must seed the helper entry stack or it will misclassify the
    // helper as void after the nested call.
    let nef = build_nef_with_token(
        &[0x12, 0x11, 0x34, 0x03, 0x40, 0x50, 0x37, 0x00, 0x00, 0x40],
        "notify",
        1,
        false,
    );
    let manifest = neo_decompiler::ContractManifest::from_json_str(
        r#"{
            "name": "EntryArgumentReturn",
            "abi": { "methods": [{
                "name": "main", "parameters": [], "returntype": "Integer", "offset": 0
            }] }
        }"#,
    )
    .unwrap();
    let mut dec = Decompiler::new()
        .decompile_bytes_with_manifest(&nef, Some(manifest), OutputFormat::All)
        .unwrap();

    let ir = dec.render_structured_ir();

    assert!(
        ir.contains("fn sub_0x0005(arg0: any, arg1: any) -> any"),
        "unconsumed entry argument must keep the helper value-returning:\n{ir}"
    );
    assert!(
        ir.contains("sub_0x0005(1, 2)"),
        "caller must consume the helper's returned value:\n{ir}"
    );
}

#[test]
fn structured_ir_seeds_inferred_entry_stack_arguments() {
    // main pushes helper arguments right-to-left and calls helper@6. The helper
    // has no INITSLOT and consumes both values directly with SUB.
    let nef = build_nef(&[0x12, 0x11, 0x34, 0x04, 0x40, 0x21, 0x9F, 0x40]);
    let manifest = neo_decompiler::ContractManifest::from_json_str(
        r#"{
            "name": "EntryStackArgs",
            "abi": { "methods": [{
                "name": "main", "parameters": [], "returntype": "Integer", "offset": 0
            }] }
        }"#,
    )
    .unwrap();
    let mut dec = Decompiler::new()
        .decompile_bytes_with_manifest(&nef, Some(manifest), OutputFormat::All)
        .unwrap();

    let ir = dec.render_structured_ir();

    assert!(
        ir.contains("sub_0x0006(1, 2)"),
        "caller must use inferred two-argument contract:\n{ir}"
    );
    assert!(
        ir.contains("fn sub_0x0006(arg0: any, arg1: any) -> any"),
        "helper signature must declare inferred entry-stack arguments:\n{ir}"
    );
    assert!(
        ir.contains("arg1") && ir.contains("arg0") && !ir.contains('?'),
        "helper body must read seeded arguments instead of underflow placeholders:\n{ir}"
    );
}

#[test]
fn structured_ir_preserves_offsetless_entry_arity_for_recursive_call() {
    // The first offsetless ABI method is the script entry. Its manifest arity
    // must survive bytecode argument inference so the self-call consumes and
    // renders the declared value argument.
    let nef = build_nef(&[0x78, 0x34, 0xFF, 0x40]);
    let manifest = neo_decompiler::ContractManifest::from_json_str(
        r#"{
            "name": "OffsetlessRecursive",
            "abi": { "methods": [{
                "name": "recur",
                "parameters": [{ "name": "value", "type": "Integer" }],
                "returntype": "Integer"
            }] }
        }"#,
    )
    .unwrap();
    let mut dec = Decompiler::new()
        .decompile_bytes_with_manifest(&nef, Some(manifest), OutputFormat::All)
        .unwrap();

    let ir = dec.render_structured_ir();

    assert!(
        ir.contains("recur(value)"),
        "recursive call must retain the offsetless entry's manifest arity:\n{ir}"
    );
    assert!(
        !ir.contains("recur()"),
        "recursive call must not drop the declared argument:\n{ir}"
    );
}

#[test]
fn structured_ir_keeps_resolved_void_call_as_statement() {
    // main: ambient PUSH9, argument PUSH1, CALL helper@6, RET.
    // helper: INITSLOT 0,1; LDARG0; DROP; RET.
    let nef = build_nef(&[
        0x19, 0x11, 0x34, 0x04, 0x40, 0x21, 0x57, 0x00, 0x01, 0x78, 0x45, 0x40,
    ]);
    let manifest = neo_decompiler::ContractManifest::from_json_str(
        r#"{
            "name": "TypedVoidCall",
            "abi": {
                "methods": [
                    {
                        "name": "main",
                        "parameters": [],
                        "returntype": "Integer",
                        "offset": 0
                    },
                    {
                        "name": "helper",
                        "parameters": [{ "name": "value", "type": "Integer" }],
                        "returntype": "Void",
                        "offset": 6
                    }
                ]
            }
        }"#,
    )
    .unwrap();
    let mut dec = Decompiler::new()
        .decompile_bytes_with_manifest(&nef, Some(manifest), OutputFormat::All)
        .unwrap();

    let ir = dec.render_structured_ir();

    assert!(
        ir.contains("helper(1);"),
        "resolved void call must remain visible as a statement:\n{ir}"
    );
    assert!(
        ir.contains("return 9;"),
        "resolved void call must preserve the caller's ambient return value:\n{ir}"
    );
    assert!(
        !ir.contains("= helper(1)") && !ir.contains("return helper(1)"),
        "resolved void call must not manufacture a result:\n{ir}"
    );
}

#[test]
fn structured_ir_keeps_resolved_void_call_before_branch() {
    // main: condition PUSH1; argument PUSH2; void CALL helper; JMPIF.
    // The call consumes only its argument, leaving the earlier condition for
    // the branch. The side-effect statement must not be mistaken for the
    // condition-producing SSA assignment and suppressed by structuring.
    let nef = build_nef(&[
        0x11, 0x12, 0x34, 0x08, 0x24, 0x04, 0x13, 0x40, 0x14, 0x40, 0x57, 0x00, 0x01, 0x78, 0x45,
        0x40,
    ]);
    let manifest = neo_decompiler::ContractManifest::from_json_str(
        r#"{
            "name": "VoidBeforeBranch",
            "abi": {
                "methods": [
                    {
                        "name": "main",
                        "parameters": [],
                        "returntype": "Integer",
                        "offset": 0
                    },
                    {
                        "name": "helper",
                        "parameters": [{ "name": "value", "type": "Integer" }],
                        "returntype": "Void",
                        "offset": 10
                    }
                ]
            }
        }"#,
    )
    .unwrap();
    let mut dec = Decompiler::new()
        .decompile_bytes_with_manifest(&nef, Some(manifest), OutputFormat::All)
        .unwrap();

    let ir = dec.render_structured_ir();
    let call = ir
        .find("helper(2);")
        .unwrap_or_else(|| panic!("void call before branch must remain visible:\n{ir}"));
    let branch = ir[call..]
        .find("if (")
        .map(|offset| call + offset)
        .unwrap_or_else(|| panic!("branch must remain structured after the void call:\n{ir}"));

    assert!(call < branch, "call must execute before the branch:\n{ir}");
    assert!(
        !ir.contains("cond_"),
        "branch condition must resolve to its SSA value:\n{ir}"
    );
}

#[test]
fn structured_ir_uses_manifest_argument_as_branch_condition() {
    // INITSLOT 0,1; LDARG0; JMPIF then; else return 3; then return 4.
    let nef = build_nef(&[0x57, 0x00, 0x01, 0x78, 0x24, 0x04, 0x13, 0x40, 0x14, 0x40]);
    let manifest = neo_decompiler::ContractManifest::from_json_str(
        r#"{
            "name": "ArgumentBranch",
            "abi": {
                "methods": [{
                    "name": "main",
                    "parameters": [{ "name": "enabled", "type": "Boolean" }],
                    "returntype": "Integer",
                    "offset": 0
                }]
            }
        }"#,
    )
    .unwrap();
    let mut dec = Decompiler::new()
        .decompile_bytes_with_manifest(&nef, Some(manifest), OutputFormat::All)
        .unwrap();

    let ir = dec.render_structured_ir();

    assert!(
        ir.contains("if (enabled)"),
        "branch must retain the source argument condition:\n{ir}"
    );
    assert!(
        !ir.contains("cond_"),
        "branch must not fall back to an undefined condition:\n{ir}"
    );
}

#[test]
fn structured_ir_keeps_value_call_used_as_branch_condition() {
    // PUSH1; CALLT is_valid(1); JMPIF then; else return 3; then return 4.
    let nef = build_nef_with_token(
        &[0x11, 0x37, 0x00, 0x00, 0x24, 0x04, 0x13, 0x40, 0x14, 0x40],
        "is_valid",
        1,
        true,
    );
    let manifest = neo_decompiler::ContractManifest::from_json_str(
        r#"{
            "name": "CallBranch",
            "abi": {
                "methods": [{
                    "name": "main",
                    "parameters": [],
                    "returntype": "Integer",
                    "offset": 0
                }]
            }
        }"#,
    )
    .unwrap();
    let mut dec = Decompiler::new()
        .decompile_bytes_with_manifest(&nef, Some(manifest), OutputFormat::All)
        .unwrap();

    let ir = dec.render_structured_ir();

    assert!(
        ir.contains("if (is_valid(1))"),
        "value-producing call must remain as the branch condition:\n{ir}"
    );
    assert!(
        !ir.contains("cond_"),
        "call condition must not become an undefined placeholder:\n{ir}"
    );
}

#[test]
fn structured_ir_sanitizes_method_token_name() {
    let nef = build_nef_with_token(&[0x37, 0x00, 0x00, 0x40], "9-bad\nname", 0, false);
    let manifest = neo_decompiler::ContractManifest::from_json_str(
        r#"{
            "name": "TokenIdentifier",
            "abi": { "methods": [{
                "name": "main", "parameters": [], "returntype": "Void", "offset": 0
            }] }
        }"#,
    )
    .unwrap();
    let mut dec = Decompiler::new()
        .decompile_bytes_with_manifest(&nef, Some(manifest), OutputFormat::All)
        .unwrap();

    let ir = dec.render_structured_ir();

    assert!(
        ir.lines().any(|line| line.trim() == "_9_bad_name();"),
        "method-token call must render as one sanitized identifier:\n{ir}"
    );
    assert!(
        !ir.lines()
            .any(|line| line.trim_start().starts_with("name hash=")),
        "method-token metadata must not escape its comment line:\n{ir}"
    );
}

#[test]
fn structured_ir_uses_unique_labels_for_colliding_manifest_names() {
    let nef = build_nef(&[
        0x11, 0x34, 0x05, 0x40, 0x21, 0x21, 0x57, 0x00, 0x01, 0x78, 0x40,
    ]);
    let manifest = neo_decompiler::ContractManifest::from_json_str(
        r#"{
            "name": "CollidingNames",
            "abi": { "methods": [
                {
                    "name": "foo-bar", "parameters": [],
                    "returntype": "Integer", "offset": 0
                },
                {
                    "name": "foo bar", "parameters": [
                        { "name": "value", "type": "Integer" }
                    ],
                    "returntype": "Integer", "offset": 6
                }
            ] }
        }"#,
    )
    .unwrap();
    let mut dec = Decompiler::new()
        .decompile_bytes_with_manifest(&nef, Some(manifest), OutputFormat::All)
        .unwrap();

    let ir = dec.render_structured_ir();

    assert!(
        ir.contains("fn foo_bar() -> int {") && ir.contains("fn foo_bar_1(value: int) -> int {"),
        "colliding ABI names must receive unique definitions:\n{ir}"
    );
    assert!(
        ir.contains("foo_bar_1(1)"),
        "resolved call must use the unique callee label:\n{ir}"
    );
}

#[test]
fn structured_ir_uses_method_token_call_contract() {
    // Ambient PUSH9; argument PUSH1; CALLT token 0; RET. The token declares
    // one parameter and no return value, so CALLT must preserve the ambient 9.
    let nef = build_nef_with_token(&[0x19, 0x11, 0x37, 0x00, 0x00, 0x40], "notify", 1, false);
    let manifest = neo_decompiler::ContractManifest::from_json_str(
        r#"{
            "name": "TokenCall",
            "abi": {
                "methods": [{
                    "name": "main",
                    "parameters": [],
                    "returntype": "Integer",
                    "offset": 0
                }]
            }
        }"#,
    )
    .unwrap();
    let mut dec = Decompiler::new()
        .decompile_bytes_with_manifest(&nef, Some(manifest), OutputFormat::All)
        .unwrap();

    let ir = dec.render_structured_ir();

    assert!(
        ir.contains("notify(1);") && ir.contains("return 9;"),
        "CALLT must honor token arity and void-return metadata:\n{ir}"
    );
    assert!(
        !ir.contains("callt_0x"),
        "resolved method token must not retain an index placeholder:\n{ir}"
    );
}

#[test]
fn structured_ir_renders_known_syscall_value() {
    let nef = build_nef(&[0x11, 0x41, 0xF8, 0x27, 0xEC, 0x8C, 0x40]);
    let manifest = neo_decompiler::ContractManifest::from_json_str(
        r#"{
            "name": "WitnessCheck",
            "abi": { "methods": [{
                "name": "main", "parameters": [], "returntype": "Boolean", "offset": 0
            }] }
        }"#,
    )
    .unwrap();
    let mut dec = Decompiler::new()
        .decompile_bytes_with_manifest(&nef, Some(manifest), OutputFormat::All)
        .unwrap();

    let ir = dec.render_structured_ir();

    assert!(
        ir.lines()
            .any(|line| { line.trim() == "return syscall(\"System.Runtime.CheckWitness\", 1);" }),
        "known value syscall must be returned directly with its argument:\n{ir}"
    );
    assert!(
        !ir.contains("= syscall(\"System.Runtime.CheckWitness\"") && !ir.contains("return ?;"),
        "known value syscall must not retain a generated assignment:\n{ir}"
    );
}

#[test]
fn structured_ir_elides_known_syscall_temp_when_value_is_dropped() {
    let nef = build_nef(&[0x19, 0x11, 0x41, 0xF8, 0x27, 0xEC, 0x8C, 0x45, 0x40]);
    let manifest = neo_decompiler::ContractManifest::from_json_str(
        r#"{
            "name": "DroppedWitnessCheck",
            "abi": { "methods": [{
                "name": "main", "parameters": [], "returntype": "Integer", "offset": 0
            }] }
        }"#,
    )
    .unwrap();
    let mut dec = Decompiler::new()
        .decompile_bytes_with_manifest(&nef, Some(manifest), OutputFormat::All)
        .unwrap();

    let ir = dec.render_structured_ir();

    assert!(
        ir.lines()
            .any(|line| line.trim() == "syscall(\"System.Runtime.CheckWitness\", 1);"),
        "dropped value syscall must remain visible as a statement:\n{ir}"
    );
    assert!(
        ir.contains("return 9;") && !ir.contains("= syscall(\"System.Runtime.CheckWitness\""),
        "dropped value syscall must preserve the ambient return value without an assignment:\n{ir}"
    );
}

#[test]
fn structured_ir_renders_known_syscall_void_as_statement() {
    let nef = build_nef(&[0x19, 0x11, 0x41, 0xCF, 0xE7, 0x47, 0x96, 0x40]);
    let manifest = neo_decompiler::ContractManifest::from_json_str(
        r#"{
            "name": "RuntimeLog",
            "abi": { "methods": [{
                "name": "main", "parameters": [], "returntype": "Integer", "offset": 0
            }] }
        }"#,
    )
    .unwrap();
    let mut dec = Decompiler::new()
        .decompile_bytes_with_manifest(&nef, Some(manifest), OutputFormat::All)
        .unwrap();

    let ir = dec.render_structured_ir();

    assert!(
        ir.lines()
            .any(|line| line.trim() == "syscall(\"System.Runtime.Log\", 1);"),
        "known void syscall must remain visible as a statement:\n{ir}"
    );
    assert!(
        ir.contains("return 9;") && !ir.contains("= syscall(\"System.Runtime.Log\""),
        "void syscall must preserve the ambient return value without an assignment:\n{ir}"
    );
}

#[test]
fn structured_ir_removes_pointer_materialization_for_resolved_calla() {
    // main: ambient PUSH9; argument PUSH1; PUSHA helper@10; CALLA; RET.
    // The resolved pointer is control metadata, not a source-level call.
    let nef = build_nef(&[
        0x19, 0x11, 0x0A, 0x08, 0x00, 0x00, 0x00, 0x36, 0x40, 0x21, 0x57, 0x00, 0x01, 0x78, 0x45,
        0x40,
    ]);
    let manifest = neo_decompiler::ContractManifest::from_json_str(
        r#"{
            "name": "ResolvedCallA",
            "abi": {
                "methods": [
                    {
                        "name": "main",
                        "parameters": [],
                        "returntype": "Integer",
                        "offset": 0
                    },
                    {
                        "name": "helper",
                        "parameters": [{ "name": "value", "type": "Integer" }],
                        "returntype": "Void",
                        "offset": 10
                    }
                ]
            }
        }"#,
    )
    .unwrap();
    let mut dec = Decompiler::new()
        .decompile_bytes_with_manifest(&nef, Some(manifest), OutputFormat::All)
        .unwrap();

    let ir = dec.render_structured_ir();

    assert!(
        ir.contains("helper(1);") && ir.contains("return 9;"),
        "resolved CALLA must honor the callee contract:\n{ir}"
    );
    assert!(
        !ir.to_ascii_lowercase().contains("pusha"),
        "resolved CALLA must not leak pointer materialization:\n{ir}"
    );
}

#[test]
fn structured_ir_pusha_uses_absolute_target_offset() {
    // PUSHA at offset 2 with displacement +5 points to absolute offset 7.
    let nef = build_nef(&[0x10, 0x45, 0x0A, 0x05, 0x00, 0x00, 0x00, 0x40]);
    let manifest = neo_decompiler::ContractManifest::from_json_str(
        r#"{
            "name": "PointerTarget",
            "abi": { "methods": [{
                "name": "main", "parameters": [], "returntype": "Any", "offset": 0
            }] }
        }"#,
    )
    .unwrap();
    let mut dec = Decompiler::new()
        .decompile_bytes_with_manifest(&nef, Some(manifest), OutputFormat::All)
        .unwrap();

    let ir = dec.render_structured_ir();

    assert!(
        ir.contains("return 7;"),
        "PUSHA must render instruction offset plus displacement:\n{ir}"
    );
    assert!(
        !ir.contains("return 5;"),
        "relative displacement must not leak as the pointer value:\n{ir}"
    );
}

#[test]
fn structured_ir_defines_stack_phi_before_resolved_call() {
    // A branch chooses PUSH1 or PUSH2, then both paths call check(value).
    let nef = build_nef_with_token(
        &[
            0x11, 0x24, 0x06, 0x11, 0x22, 0x05, 0x21, 0x12, 0x21, 0x37, 0x00, 0x00, 0x40,
        ],
        "check",
        1,
        false,
    );
    let manifest = neo_decompiler::ContractManifest::from_json_str(
        r#"{
            "name": "PhiCall",
            "abi": { "methods": [{
                "name": "main", "parameters": [], "returntype": "Void", "offset": 0
            }] }
        }"#,
    )
    .unwrap();
    let mut dec = Decompiler::new()
        .decompile_bytes_with_manifest(&nef, Some(manifest), OutputFormat::All)
        .unwrap();

    let ir = dec.render_structured_ir();

    assert!(
        !ir.contains("phi(") && !ir.contains('φ'),
        "structured IR must not expose analysis phi syntax:\n{ir}"
    );
    assert_eq!(
        ir.matches("p4_0 = ").count(),
        2,
        "each incoming branch must define the merged value exactly once:\n{ir}"
    );
    assert!(
        ir.contains("check(p4_0);"),
        "resolved call must consume the edge-defined merged value:\n{ir}"
    );
}

#[test]
fn structured_ir_stack_phi_preserves_short_path_underflow() {
    // The fallthrough supplies one stack value while the taken path supplies
    // two. A two-argument token call at the merge must retain `?` for the value
    // absent on the short path.
    let nef = build_nef_with_token(
        &[
            0x11, 0x24, 0x06, 0x12, 0x22, 0x06, 0x21, 0x12, 0x11, 0x21, 0x37, 0x00, 0x00, 0x40,
        ],
        "helper",
        2,
        false,
    );
    let manifest = neo_decompiler::ContractManifest::from_json_str(
        r#"{
            "name": "UnevenPhiCall",
            "abi": { "methods": [{
                "name": "main", "parameters": [], "returntype": "Void", "offset": 0
            }] }
        }"#,
    )
    .unwrap();
    let mut dec = Decompiler::new()
        .decompile_bytes_with_manifest(&nef, Some(manifest), OutputFormat::All)
        .unwrap();

    let ir = dec.render_structured_ir();

    assert!(
        !ir.contains("phi(") && !ir.contains('φ'),
        "structured IR must not expose analysis phi syntax:\n{ir}"
    );
    let assignments: Vec<_> = ir.lines().filter(|line| line.contains("p4_0 = ")).collect();

    assert_eq!(
        assignments.len(),
        2,
        "both incoming paths must define the underflow-sensitive merge:\n{ir}"
    );
    assert!(
        assignments.iter().any(|line| line.trim() == "p4_0 = ?;")
            && assignments.iter().any(|line| {
                let line = line.trim();
                line.starts_with("p4_0 = ") && line != "p4_0 = ?;"
            }),
        "only the short path should contribute an unknown merged value:\n{ir}"
    );
    assert!(
        ir.contains("helper(p4_1, p4_0);"),
        "top-aligned merge must place short-path underflow in the second argument:\n{ir}"
    );
}

#[test]
fn structured_ir_distinguishes_user_append_from_vm_append() {
    // main: PUSH1; CALL append@4; RET
    // append: NEWARRAY0; DUP; PUSH1; APPEND; RET
    let nef = build_nef(&[0x11, 0x34, 0x03, 0x40, 0xC2, 0x4A, 0x11, 0xCF, 0x40]);
    let manifest = neo_decompiler::ContractManifest::from_json_str(
        r#"{
            "name": "AppendCollision",
            "abi": { "methods": [
                { "name": "main", "parameters": [], "returntype": "Array", "offset": 0 },
                { "name": "append", "parameters": [], "returntype": "Array", "offset": 4 }
            ] }
        }"#,
    )
    .unwrap();
    let mut dec = Decompiler::new()
        .decompile_bytes_with_manifest(&nef, Some(manifest), OutputFormat::All)
        .unwrap();

    let ir = dec.render_structured_ir();

    assert!(
        ir.contains("fn append()"),
        "user method must retain its name:\n{ir}"
    );
    assert!(
        ir.lines().any(|line| line.trim() == "return append();"),
        "internal user call must remain callable:\n{ir}"
    );
    assert!(
        ir.lines().any(|line| {
            let line = line.trim();
            line.starts_with("append(") && line.ends_with(");")
        }),
        "VM APPEND must remain a separate intrinsic effect:\n{ir}"
    );
}
