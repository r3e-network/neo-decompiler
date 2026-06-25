//! End-to-end validation of the IR-spine pipeline (`--format ir`) on real bytecode.
//!
//! These build real NEF containers (or reuse a real artifact) and run the full
//! lift → SSA → optimize → structure → render path, asserting that each
//! control-flow construct is recovered. This complements the structurer unit
//! tests (which use hand-built SSA) by exercising the whole pipeline.

#![allow(clippy::unwrap_used)]

use std::fs;

use neo_decompiler::{Decompiler, NefParser, OutputFormat};

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
        ir.contains("if (") || ir.contains("while (") || ir.contains("do {"),
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
