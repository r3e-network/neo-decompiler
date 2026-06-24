//! End-to-end coverage for real stack-effect SSA via `Decompilation::compute_ssa`.
//!
//! Phase 2 of the advanced-decompiler evolution: the SSA builder now produces
//! genuine def/use chains and φ nodes (instead of the PUSH-only skeleton), driven
//! by the comprehensive stack-effect model in `cfg::ssa::effects`.

#![allow(clippy::unwrap_used)]

use std::fs;

use neo_decompiler::{Decompiler, OutputFormat, SsaStats};

fn repo_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn decompile_loop_if() -> neo_decompiler::Decompilation {
    let root = repo_root();
    let nef = fs::read(root.join("TestingArtifacts/edgecases/LoopIf.nef")).unwrap();
    let manifest = fs::read_to_string(root.join("TestingArtifacts/edgecases/LoopIf.manifest.json"))
        .ok()
        .and_then(|s| neo_decompiler::ContractManifest::from_json_str(&s).ok());
    let mut dec = Decompiler::new()
        .decompile_bytes_with_manifest(&nef, manifest, OutputFormat::All)
        .unwrap();
    dec.compute_ssa();
    dec
}

#[test]
fn compute_ssa_produces_real_definitions_on_a_real_contract() {
    let dec = decompile_loop_if();
    let ssa = dec.ssa().expect("SSA should be computed");

    // The skeleton produced zero definitions; the real stack-effect SSA must
    // surface at least one definition (LoopIf has PUSH/STLOC/LDLOC/INC/...).
    assert!(
        !ssa.definitions.is_empty(),
        "real SSA should record variable definitions; got stats: {}",
        ssa.stats()
    );
}

#[test]
fn compute_ssa_stats_report_statements_and_blocks() {
    let dec = decompile_loop_if();
    let stats: SsaStats = dec.ssa().unwrap().stats();
    assert!(stats.block_count > 0, "LoopIf CFG has basic blocks");
    assert!(
        stats.total_statements > 0,
        "real SSA should produce assignment statements, not just comments"
    );
    assert!(
        stats.total_variables > 0,
        "real SSA should define variables"
    );
}

#[test]
fn compute_ssa_is_idempotent() {
    // Calling compute_ssa twice must not panic or duplicate work.
    let root = repo_root();
    let nef = fs::read(root.join("TestingArtifacts/edgecases/LoopIf.nef")).unwrap();
    let mut dec = Decompiler::new()
        .decompile_bytes_with_manifest(&nef, None, OutputFormat::All)
        .unwrap();
    dec.compute_ssa();
    let first = dec.ssa().unwrap().stats();
    dec.compute_ssa();
    let second = dec.ssa().unwrap().stats();
    assert_eq!(first, second, "compute_ssa must be idempotent");
}

#[test]
fn optimize_ssa_runs_without_panicking_and_keeps_form_consistent() {
    // optimize_ssa must run the optimization passes to a fixed point on a real
    // contract, leave the SSA indexes consistent, and be safe to call twice.
    let root = repo_root();
    let nef = fs::read(root.join("TestingArtifacts/edgecases/LoopIf.nef")).unwrap();
    let manifest = fs::read_to_string(root.join("TestingArtifacts/edgecases/LoopIf.manifest.json"))
        .ok()
        .and_then(|s| neo_decompiler::ContractManifest::from_json_str(&s).ok());

    let mut dec = Decompiler::new()
        .decompile_bytes_with_manifest(&nef, manifest, OutputFormat::All)
        .unwrap();
    let rounds = dec.optimize_ssa();
    // LoopIf has at least one foldable arithmetic (i++-style ADD); the optimizer
    // may or may not simplify depending on stack layout, but it must not corrupt
    // the form. Verify the indexes are still self-consistent.
    let ssa = dec.ssa().expect("optimize_ssa computes SSA");
    let stats = ssa.stats();
    let _ = rounds;
    assert!(
        stats.total_variables == ssa.definitions.len(),
        "definition count ({}) must match variable count ({}) after optimization",
        ssa.definitions.len(),
        stats.total_variables
    );

    // Calling optimize again is a no-op (already at fixed point) and must not panic.
    let second = dec.optimize_ssa();
    let _ = second;
}
