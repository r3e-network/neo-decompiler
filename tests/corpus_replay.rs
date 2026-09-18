//! Corpus replay / regression test.
//!
//! Replays committed `TestingArtifacts/*` fixtures through the full pipeline
//! (NEF parse → disassemble → CFG → SSA → render) under `catch_unwind`, and
//! additionally replays every locally generated fuzz corpus under
//! `fuzz/corpus/` when present. Because that directory is gitignored, the
//! committed fixtures are the always-present seed so the fence runs with real
//! coverage on a fresh CI checkout, while any local corpora layered on top
//! extend it. `decompile_all_artifacts_across_formats_without_panics`
//! re-decompiles every `TestingArtifacts/*` contract across all output formats.
//!
//! This is the regression fence introduced in the advanced-decompiler Phase 0:
//! it catches panics on fuzzer-found inputs and pins artifact decompilation so
//! later phases detect regressions immediately.

#![allow(clippy::unwrap_used)]

use std::fs;
use std::panic::{catch_unwind, UnwindSafe};
use std::path::{Path, PathBuf};

use neo_decompiler::{
    CfgBuilder, ContractManifest, Decompiler, Disassembler, NefParser, OutputFormat,
};

/// Locate the repo root from CARGO_MANIFEST_DIR (set by cargo at build time).
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Recursively collect files under `dir`.
fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    collect_files_except(dir, out, None);
}

fn collect_files_except(dir: &Path, out: &mut Vec<PathBuf>, excluded: Option<&Path>) {
    if excluded == Some(dir) {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files_except(&path, out, excluded);
        } else {
            out.push(path);
        }
    }
}

/// Discover committed `.nef` / `.manifest.json` fixtures under
/// `TestingArtifacts/`. These always ship in git, so the panic fence has
/// meaningful coverage even on a fresh CI checkout where the gitignored
/// `fuzz/corpus/` directories are empty. Returns `(nef_files, manifest_files)`.
fn committed_fixtures(root: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut all = Vec::new();
    let artifacts = root.join("TestingArtifacts");
    // The decompiled directory contains generated copies of the inputs, not
    // additional authoritative fixtures. Keep their presence from multiplying
    // replay time or making local and clean-checkout coverage diverge.
    collect_files_except(&artifacts, &mut all, Some(&artifacts.join("decompiled")));
    let is_nef = |p: &PathBuf| p.extension().and_then(|e| e.to_str()) == Some("nef");
    let is_manifest = |p: &PathBuf| {
        p.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with(".manifest.json"))
    };
    let mut nefs: Vec<PathBuf> = all.iter().filter(|p| is_nef(p)).cloned().collect();
    let mut manifests: Vec<PathBuf> = all.iter().filter(|p| is_manifest(p)).cloned().collect();
    nefs.sort();
    manifests.sort();
    (nefs, manifests)
}

/// Raw script bytes extracted from every committed `.nef`, used to seed the
/// raw-bytecode target so it never runs empty.
fn committed_scripts(nef_files: &[PathBuf]) -> Vec<Vec<u8>> {
    let parser = NefParser::new();
    nef_files
        .iter()
        .filter_map(|p| fs::read(p).ok())
        .filter_map(|bytes| parser.parse(&bytes).ok())
        .map(|nef| nef.script)
        .collect()
}

/// One entry point per corpus: each exercises a different pipeline slice.
#[derive(Copy, Clone, PartialEq, Eq)]
enum Target {
    /// Full NEF-based decompile pipeline.
    NefDecompile,
    /// Raw bytecode: disassemble → CFG (no NEF wrapper).
    RawDecompile,
    /// NEF container parse only.
    NefParse,
    /// Manifest JSON parse only.
    Manifest,
}

impl Target {
    fn dir(&self) -> &'static str {
        match self {
            Self::NefDecompile => "fuzz/corpus/fuzz_decompile",
            Self::RawDecompile => "fuzz/corpus/fuzz_decompile_raw",
            Self::NefParse => "fuzz/corpus/fuzz_nef_parse",
            Self::Manifest => "fuzz/corpus/fuzz_manifest",
        }
    }
}

fn run_target(data: &[u8], target: Target) {
    match target {
        Target::NefDecompile => {
            let _ = Decompiler::new().decompile_bytes(data);
        }
        Target::NefParse => {
            let _ = NefParser::new().parse(data);
        }
        Target::RawDecompile => {
            let dis = Disassembler::new();
            if let Ok(instrs) = dis.disassemble(data) {
                if !instrs.is_empty() {
                    // Disassemble + build the CFG as a panic fence across the
                    // whole corpus. (Real SSA construction is exercised on
                    // representative artifacts by ir_pipeline / ssa_e2e; running
                    // it on every fuzz input is too slow for this fence.)
                    let _ = CfgBuilder::new(&instrs).build();
                }
            }
        }
        Target::Manifest => {
            // Manifest corpus is JSON text; parse best-effort.
            if let Ok(text) = std::str::from_utf8(data) {
                let _ = ContractManifest::from_json_str(text);
            }
        }
    }
}

/// The only panic boundary for replay operations. Preserve the input identity
/// and original panic message while failing the test outside `catch_unwind`.
fn run_labeled_target<T>(label: &str, target: &str, run: impl FnOnce() -> T + UnwindSafe) -> T {
    match catch_unwind(run) {
        Ok(result) => result,
        Err(payload) => {
            let detail = payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                .unwrap_or("non-string panic payload");
            panic!("corpus replay panic in {target} target at {label}: {detail}");
        }
    }
}

#[test]
#[should_panic(
    expected = "corpus replay panic in synthetic target at sentinel input: injected panic"
)]
fn replay_harness_propagates_panics_with_input_and_target_labels() {
    run_labeled_target("sentinel input", "synthetic", || panic!("injected panic"));
}

#[test]
fn fixture_discovery_excludes_generated_mirrors_and_preserves_source_subtrees() {
    let root = tempfile::tempdir().expect("temporary fixture root");
    let artifacts = root.path().join("TestingArtifacts");
    for relative in [
        "original.nef",
        "original.manifest.json",
        "edgecases/edge.nef",
        "devpack/devpack.manifest.json",
        "decompiled/original.nef",
        "decompiled/nested/original.manifest.json",
    ] {
        let path = artifacts.join(relative);
        fs::create_dir_all(path.parent().expect("fixture parent")).expect("fixture directory");
        fs::write(path, b"fixture").expect("fixture file");
    }

    let (nefs, manifests) = committed_fixtures(root.path());
    assert_eq!(
        nefs,
        vec![
            artifacts.join("edgecases/edge.nef"),
            artifacts.join("original.nef")
        ]
    );
    assert_eq!(
        manifests,
        vec![
            artifacts.join("devpack/devpack.manifest.json"),
            artifacts.join("original.manifest.json")
        ]
    );
}

#[test]
fn replay_all_fuzz_corpora_without_panics() {
    let root = repo_root();
    let (nef_files, manifest_files) = committed_fixtures(&root);
    let raw_scripts = committed_scripts(&nef_files);

    for target in [
        Target::NefDecompile,
        Target::RawDecompile,
        Target::NefParse,
        Target::Manifest,
    ] {
        // Seed the fence with committed fixtures so every checkout (including a
        // fresh CI clone where the gitignored corpora are absent) replays real
        // inputs. Local fuzz corpora, when present, are layered on top.
        let mut inputs: Vec<(String, Vec<u8>)> = Vec::new();
        match target {
            Target::NefDecompile | Target::NefParse => {
                for f in &nef_files {
                    if let Ok(data) = fs::read(f) {
                        inputs.push((format!("committed artifact {}", f.display()), data));
                    }
                }
            }
            Target::RawDecompile => {
                for (i, script) in raw_scripts.iter().enumerate() {
                    inputs.push((format!("committed script #{i}"), script.clone()));
                }
            }
            Target::Manifest => {
                for f in &manifest_files {
                    if let Ok(data) = fs::read(f) {
                        inputs.push((format!("committed manifest {}", f.display()), data));
                    }
                }
            }
        }

        // Additionally replay any locally generated fuzz corpus (gitignored).
        let dir = root.join(target.dir());
        let mut corpus_files = Vec::new();
        collect_files(&dir, &mut corpus_files);
        corpus_files.sort();
        for file in &corpus_files {
            // Skip the synthetic named .nef seed (already covered by fixtures).
            if file.extension().and_then(|e| e.to_str()) == Some("nef") {
                continue;
            }
            if let Ok(data) = fs::read(file) {
                inputs.push((format!("{} corpus {}", target.dir(), file.display()), data));
            }
        }

        let count = inputs.len();
        for (label, data) in &inputs {
            run_labeled_target(label, target.dir(), || {
                run_target(data, target);
            });
        }

        // The committed fixtures guarantee non-empty coverage on every checkout,
        // so this now flags a genuine regression (fixtures gone) rather than an
        // absent local, gitignored fuzz corpus.
        assert!(
            count > 0,
            "no replay inputs for {} (committed fixtures missing?)",
            target.dir()
        );
    }
}

#[test]
fn decompile_all_artifacts_across_formats_without_panics() {
    let root = repo_root();
    let (nef_files, _) = committed_fixtures(&root);

    assert!(!nef_files.is_empty(), "no .nef artifacts discovered");

    let decompiler = Decompiler::new();
    let mut decompiled = 0usize;
    for nef_path in &nef_files {
        let manifest_path = nef_path.with_extension("manifest.json");
        let data = fs::read(nef_path).expect("read nef artifact");
        let manifest = fs::read_to_string(&manifest_path)
            .ok()
            .and_then(|text| ContractManifest::from_json_str(&text).ok());

        let result = run_labeled_target(
            &nef_path.display().to_string(),
            "artifact all-formats",
            || decompiler.decompile_bytes_with_manifest(&data, manifest, OutputFormat::All),
        );
        if result.is_ok() {
            decompiled += 1;
        }
    }
    assert!(decompiled > 0, "no artifact decompiled successfully");
}

/// Smoke-test the parser directly on the nef corpus (mirrors fuzz_nef_parse).
/// Deterministic byte generator (LCG) so the batch is reproducible in CI.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }

    fn byte(&mut self) -> u8 {
        self.next() as u8
    }

    fn index(&mut self, len: usize) -> usize {
        if len == 0 {
            0
        } else {
            (self.next() as usize) % len
        }
    }
}

/// Deterministically mutate committed raw scripts into malformed bytecode that
/// exercises operand truncation, unknown opcodes, and pathological control
/// flow through the disassemble + CFG fence (and the full decompile path).
fn malformed_script_variants(script: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut variants: Vec<(String, Vec<u8>)> = Vec::new();
    if script.is_empty() {
        return variants;
    }

    // Truncated operands: every strict prefix of the script ends one instruction
    // short of its operand, exercising UnexpectedEof paths in decode.
    for len in (1..script.len()).step_by((script.len() / 64).max(1)) {
        variants.push((format!("truncate@{len}"), script[..len].to_vec()));
    }

    let mut rng = Lcg(0x9E37_79B9_7F4A_7C15);
    // Random byte flips in an otherwise valid script.
    for i in 0..96 {
        let mut mutated = script.to_vec();
        let flips = 1 + (rng.next() as usize % 8);
        for _ in 0..flips {
            let pos = rng.index(mutated.len());
            mutated[pos] ^= rng.byte();
        }
        variants.push((format!("flip#{i}"), mutated));
    }
    // Append a random tail (open jumps / operands past the intended end).
    for i in 0..32 {
        let mut mutated = script.to_vec();
        let tail_len = 1 + (rng.next() as usize % 48);
        for _ in 0..tail_len {
            mutated.push(rng.byte());
        }
        variants.push((format!("tail#{i}"), mutated));
    }
    // Insert random bytes in the middle (splits instructions, alters offsets).
    for i in 0..48 {
        let mut mutated = script.to_vec();
        let pos = rng.index(mutated.len());
        let insert_len = 1 + (rng.next() as usize % 8);
        let mut tail = Vec::with_capacity(insert_len);
        for _ in 0..insert_len {
            tail.push(rng.byte());
        }
        mutated.splice(pos..pos, tail);
        variants.push((format!("insert#{i}"), mutated));
    }
    variants
}

#[test]
fn programmatic_malformed_scripts_without_panics() {
    let root = repo_root();
    let (nef_files, _) = committed_fixtures(&root);
    let seeds = committed_scripts(&nef_files);
    assert!(
        !seeds.is_empty(),
        "no committed raw scripts to seed malformed batch"
    );

    let mut variants: Vec<(String, Vec<u8>)> = Vec::new();
    for (i, script) in seeds.iter().enumerate() {
        for (label, data) in malformed_script_variants(script) {
            variants.push((format!("script#{i}:{label}"), data));
        }
    }

    for (label, data) in &variants {
        // Raw script fence: disassemble + CFG when disassembly succeeds.
        run_labeled_target(label, "programmatic-malformed/raw", || {
            run_target(data, Target::RawDecompile);
        });
        // Full NEF decompile on the raw bytes as a container (most fail parse,
        // which is itself the parse-boundary coverage).
        run_labeled_target(label, "programmatic-malformed/nef", || {
            run_target(data, Target::NefDecompile);
        });
    }
}

#[test]
fn nef_parser_corpus_smoke() {
    let root = repo_root();
    let mut files = Vec::new();
    collect_files(&root.join("fuzz/corpus/fuzz_nef_parse"), &mut files);
    let parser = NefParser::new();
    for file in &files {
        if file.extension().and_then(|e| e.to_str()) == Some("nef") {
            continue;
        }
        let Ok(data) = fs::read(file) else { continue };
        run_labeled_target(&file.display().to_string(), Target::NefParse.dir(), || {
            let _ = parser.parse(&data);
        });
    }
}
