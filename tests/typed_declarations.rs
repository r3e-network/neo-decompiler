//! End-to-end coverage for `Decompiler::with_typed_declarations`.
//!
//! Phase 1 of the advanced-decompiler evolution: the existing-but-unused
//! type-inference engine (`analysis::types`) now annotates C# body-local
//! declarations with inferred types. Opt-in (default off) so historical output
//! is unchanged.

#![allow(clippy::unwrap_used)]

use std::fs;

use neo_decompiler::{ContractManifest, Decompiler, OutputFormat};

/// Locate the repo root from CARGO_MANIFEST_DIR.
fn repo_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Read a `(nef, manifest)` pair from TestingArtifacts.
fn artifact(name: &str) -> (Vec<u8>, Option<String>) {
    let root = repo_root();
    let nef = fs::read(
        root.join("TestingArtifacts")
            .join(name)
            .with_extension("nef"),
    )
    .unwrap();
    let manifest = fs::read_to_string(
        root.join("TestingArtifacts")
            .join(name)
            .with_extension("manifest.json"),
    )
    .ok();
    (nef, manifest)
}

fn decompile_csharp(nef: &[u8], manifest: Option<&str>, typed: bool) -> String {
    let m = manifest.and_then(|s| ContractManifest::from_json_str(s).ok());
    let decompiler = Decompiler::new().with_typed_declarations(typed);
    let dec = decompiler
        .decompile_bytes_with_manifest(nef, m, OutputFormat::CSharp)
        .unwrap();
    dec.csharp.unwrap_or_default()
}

#[test]
fn typed_declarations_annotate_inferred_integer_locals() {
    // LoopIf: a counter `loc0` initialised from PUSH0 and used with PUSH3/LT
    // and INC — type inference should resolve it to Integer → `BigInteger`.
    let (nef, manifest) = artifact("edgecases/LoopIf");

    // Default (off): body locals render as `var loc0`, never typed.
    let untyped = decompile_csharp(&nef, manifest.as_deref(), false);
    assert!(
        !untyped.contains("BigInteger loc0"),
        "typed-off output must not declare loc0 as BigInteger:\n{untyped}"
    );

    // Opt-in (on): the same local is now declared with its inferred type.
    let typed = decompile_csharp(&nef, manifest.as_deref(), true);
    assert!(
        typed.contains("BigInteger loc0"),
        "typed output should declare loc0 as BigInteger; got:\n{typed}"
    );
}

#[test]
fn typed_declarations_produce_valid_empty_type_fallback() {
    // A typed declaration must never emit an empty type token (e.g. ` loc0 =`
    // with a leading double space) — unknowns fall back to `var`, not `""`.
    let (nef, manifest) = artifact("edgecases/LoopIf");
    let typed = decompile_csharp(&nef, manifest.as_deref(), true);
    assert!(
        !typed.contains("\n  loc0 =") && !typed.contains("var  loc0"),
        "typed output must not contain a malformed empty-type declaration"
    );
    // Temps (`tN`) are never in the slot map, so they must remain `var`.
    for line in typed.lines() {
        let t = line.trim();
        if t.starts_with("t0 ") || t.starts_with("t0=") {
            panic!("temp should not appear as a bare declaration: {t:?}");
        }
    }
}

#[test]
fn typed_declarations_off_matches_default() {
    // The flag-off path must be byte-identical to a Decompiler that never
    // touched the flag — guarantees the feature is purely additive.
    let (nef, manifest) = artifact("edgecases/LoopIf");
    let parsed = ContractManifest::from_json_str(manifest.as_deref().unwrap_or("")).ok();
    let default = Decompiler::new()
        .decompile_bytes_with_manifest(&nef, parsed, OutputFormat::CSharp)
        .unwrap()
        .csharp
        .unwrap_or_default();
    let off = decompile_csharp(&nef, manifest.as_deref(), false);
    assert_eq!(
        default, off,
        "with_typed_declarations(false) must equal the default decompiler"
    );
}
