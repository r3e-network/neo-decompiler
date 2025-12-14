use neo_decompiler::Decompiler;
use std::fs;
use std::path::Path;

use crate::artifact::{collect_artifacts, ContractStatus};
use crate::common::assert_non_empty;
use crate::known_unsupported::{
    find_expected_message, is_known_unsupported, load_known_unsupported,
};
use crate::process::process_artifact;

#[test]
fn decompile_testing_artifacts_into_folder() {
    let artifacts_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("TestingArtifacts");
    if !artifacts_dir.is_dir() {
        eprintln!(
            "Skipping artifact decompilation test: {} not found",
            artifacts_dir.display()
        );
        return;
    }

    let output_dir = artifacts_dir.join("decompiled");
    if output_dir.exists() {
        fs::remove_dir_all(&output_dir).expect("remove existing decompiled folder");
    }
    fs::create_dir_all(&output_dir).expect("create decompiled folder");

    let decompiler = Decompiler::new();
    let mut processed = 0usize;
    let mut skipped = Vec::new();
    let mut known_failures = Vec::new();
    let mut successes = Vec::new();

    let known_unsupported = load_known_unsupported(&artifacts_dir);
    let artifacts = collect_artifacts(&artifacts_dir, &output_dir);
    let expected_skips: Vec<String> = artifacts
        .iter()
        .filter(|artifact| is_known_unsupported(&artifact.id, &known_unsupported))
        .map(|artifact| artifact.id.clone())
        .collect();

    for artifact in artifacts {
        let id = artifact.id.clone();
        let output_base = artifact.output_base.clone();
        match process_artifact(&decompiler, &artifact, &known_unsupported) {
            ContractStatus::Success => {
                processed += 1;
                successes.push(output_base);
            }
            ContractStatus::KnownUnsupported => {
                skipped.push(id.clone());
                known_failures.push((id, output_base));
            }
        }
    }

    if processed == 0 {
        eprintln!(
            "No testing artifacts found in {} (skipping)",
            artifacts_dir.display()
        );
        return;
    }
    skipped.sort();
    let mut expected = expected_skips;
    expected.sort();
    assert_eq!(
        skipped, expected,
        "unexpected contracts failed to decompile"
    );

    for (id, output_base) in &known_failures {
        let error_path = output_base.with_extension("error.txt");
        assert!(
            error_path.is_file(),
            "known-unsupported artifact {id} should emit {error_path:?}"
        );
        let contents = fs::read_to_string(&error_path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", error_path.display()));
        assert!(
            !contents.trim().is_empty(),
            "error output for {id} must not be empty"
        );
        if let Some(expected) = find_expected_message(id, &known_unsupported) {
            assert!(
                contents.contains(expected),
                "error output for {id} should contain expected hint {:?}",
                expected
            );
        }
    }

    for output_base in &successes {
        assert_non_empty(
            &output_base.with_extension("high-level.cs"),
            "high-level output missing",
        );
        assert_non_empty(
            &output_base.with_extension("pseudocode.txt"),
            "pseudocode output missing",
        );
    }

    eprintln!(
        "Processed {} artifacts ({} known unsupported).",
        processed,
        known_failures.len()
    );
}
