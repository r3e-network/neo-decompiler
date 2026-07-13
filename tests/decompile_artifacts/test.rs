use neo_decompiler::Decompiler;
use std::fs;
use std::path::Path;

use crate::artifact::{collect_artifacts, ContractStatus};
use crate::common::assert_non_empty;
use crate::expected_failures::{
    find_entry, find_expected_message, load_expected_invalid, load_known_unsupported,
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

    // Match the CLI's clean defaults so the committed sample outputs in
    // `TestingArtifacts/decompiled/` reflect what users actually see when
    // running `neo-decompiler decompile`. The library `Decompiler::new()`
    // still defaults to verbose (trace comments on, single-use temps not
    // inlined) for backwards compatibility with downstream embedders that
    // assert on the trace-form output.
    let decompiler = Decompiler::new()
        .with_inline_single_use_temps(true)
        .with_trace_comments(false);
    let mut processed = 0usize;
    let mut skipped = Vec::new();
    let mut known_failures = Vec::new();
    let mut invalid_failures = Vec::new();
    let mut successes = Vec::new();

    let known_unsupported = load_known_unsupported(&artifacts_dir);
    let expected_invalid = load_expected_invalid(&artifacts_dir);
    let artifacts = collect_artifacts(&artifacts_dir, &output_dir);
    let expected_known: Vec<String> = artifacts
        .iter()
        .filter(|artifact| find_entry(&artifact.id, &known_unsupported).is_some())
        .map(|artifact| artifact.id.clone())
        .collect();
    let expected_invalid_ids: Vec<String> = artifacts
        .iter()
        .filter(|artifact| find_entry(&artifact.id, &expected_invalid).is_some())
        .map(|artifact| artifact.id.clone())
        .collect();

    for artifact in &artifacts {
        assert!(
            find_entry(&artifact.id, &known_unsupported).is_none()
                || find_entry(&artifact.id, &expected_invalid).is_none(),
            "artifact {} is listed as both known-unsupported and expected-invalid",
            artifact.id
        );
    }
    for entry in known_unsupported.iter().chain(&expected_invalid) {
        assert!(
            artifacts
                .iter()
                .any(|artifact| find_entry(&artifact.id, std::slice::from_ref(entry)).is_some()),
            "stale expected-failure registry entry: {}",
            entry.id
        );
    }

    for artifact in artifacts {
        let id = artifact.id.clone();
        let output_base = artifact.output_base.clone();
        match process_artifact(
            &decompiler,
            &artifact,
            &known_unsupported,
            &expected_invalid,
        ) {
            ContractStatus::Success => {
                processed += 1;
                successes.push(output_base);
            }
            ContractStatus::KnownUnsupported => {
                skipped.push(id.clone());
                known_failures.push((id, output_base));
            }
            ContractStatus::ExpectedInvalid => {
                invalid_failures.push((id, output_base));
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
    let mut expected = expected_known;
    expected.sort();
    assert_eq!(
        skipped, expected,
        "unexpected contracts failed to decompile"
    );

    let mut rejected_invalid = invalid_failures
        .iter()
        .map(|(id, _)| id.clone())
        .collect::<Vec<_>>();
    rejected_invalid.sort();
    let mut expected_invalid_ids = expected_invalid_ids;
    expected_invalid_ids.sort();
    assert_eq!(
        rejected_invalid, expected_invalid_ids,
        "expected-invalid artifacts must be rejected"
    );

    for (id, output_base, registry, classification) in known_failures
        .iter()
        .map(|(id, output)| (id, output, &known_unsupported, "known-unsupported"))
        .chain(
            invalid_failures
                .iter()
                .map(|(id, output)| (id, output, &expected_invalid, "expected-invalid")),
        )
    {
        let error_path = output_base.with_extension("error.txt");
        assert!(
            error_path.is_file(),
            "{classification} artifact {id} should emit {error_path:?}"
        );
        let contents = fs::read_to_string(&error_path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", error_path.display()));
        assert!(
            !contents.trim().is_empty(),
            "error output for {id} must not be empty"
        );
        if let Some(expected) = find_expected_message(id, registry) {
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
        assert_non_empty(
            &output_base.with_extension("csharp.cs"),
            "C# output missing",
        );
    }

    eprintln!(
        "Processed {} artifacts ({} known unsupported, {} expected invalid).",
        processed,
        known_failures.len(),
        invalid_failures.len()
    );
}
