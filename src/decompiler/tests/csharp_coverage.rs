use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::decompiler::cfg::method_body::Fidelity;
use crate::decompiler::csharp::render_csharp;
use crate::decompiler::output_format::{OutputFormat, RenderOptions};
use crate::{ContractManifest, Decompiler};

const PINNED_DEVPACK_COMMIT: &str = "5b0b63880b6201ae3f974cc845e93a90462d8043";

fn collect_nef_files(path: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_nef_files(&path, files)?;
        } else if path.extension().is_some_and(|extension| extension == "nef") {
            files.push(path);
        }
    }
    Ok(())
}

fn collect_renderer_sources(path: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_renderer_sources(&path, files)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
    Ok(())
}

#[derive(Debug)]
struct ExpectedInvalid {
    id: String,
    expected: Option<String>,
}

fn load_expected_invalid(root: &Path) -> Vec<ExpectedInvalid> {
    let path = root.join("expected_invalid.txt");
    let Ok(contents) = fs::read_to_string(&path) else {
        return Vec::new();
    };
    contents
        .lines()
        .filter_map(|line| {
            let line = line.split('#').next().map(str::trim).unwrap_or_default();
            if line.is_empty() {
                return None;
            }
            let (id, expected) = line
                .split_once(':')
                .map_or((line, None), |(id, expected)| (id, Some(expected.trim())));
            Some(ExpectedInvalid {
                id: id.trim().to_string(),
                expected: expected.map(str::to_string),
            })
        })
        .collect()
}

fn artifact_id(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .with_extension("")
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn find_expected_invalid<'a>(
    id: &str,
    entries: &'a [ExpectedInvalid],
) -> Option<&'a ExpectedInvalid> {
    let basename = id.rsplit('/').next();
    entries
        .iter()
        .find(|entry| entry.id == id || basename.is_some_and(|name| name == entry.id))
}

#[test]
fn csharp_renderer_has_no_legacy_body_dependencies() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/decompiler/csharp/render");
    let mut files = Vec::new();
    collect_renderer_sources(&root, &mut files)
        .unwrap_or_else(|error| panic!("failed to collect {}: {error}", root.display()));
    files.sort();
    let forbidden = [
        "HighLevelEmitter",
        "csharpize_statement(",
        "csharpize_statement_typed(",
        "csharpize_expression(",
        "LegacyFallback",
        "render_legacy_body",
    ];
    let mut violations = Vec::new();
    for path in files {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        for needle in forbidden {
            if source.contains(needle) {
                violations.push(format!("{} contains {needle}", path.display()));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "legacy C# body dependencies:\n{}",
        violations.join("\n")
    );
}

#[test]
fn csharp_corpus_has_zero_structured_fallback() {
    let root = env::var_os("NEO_CSHARP_CORPUS_DIR").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("TestingArtifacts"),
        PathBuf::from,
    );
    let mut files = Vec::new();
    collect_nef_files(&root, &mut files)
        .unwrap_or_else(|error| panic!("failed to collect {}: {error}", root.display()));
    files.sort();
    let mut failures = Vec::new();
    let mut decompiled = 0usize;
    let mut status_counts = BTreeMap::new();
    let mut issue_classes = BTreeMap::new();
    let mut issue_locations = BTreeMap::<_, BTreeSet<String>>::new();
    let mut incomplete_locations = BTreeSet::new();
    let expected_invalid = load_expected_invalid(&root);
    let mut invalid_seen = HashSet::new();

    for nef_path in files {
        if nef_path
            .components()
            .any(|component| component.as_os_str() == "decompiled")
        {
            continue;
        }
        let id = artifact_id(&nef_path, &root);
        let bytes = fs::read(&nef_path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", nef_path.display()));
        let manifest_path = nef_path.with_extension("manifest.json");
        let manifest_text = fs::read_to_string(&manifest_path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", manifest_path.display()));
        let manifest = ContractManifest::from_json_str(&manifest_text)
            .unwrap_or_else(|error| panic!("failed to parse {}: {error}", manifest_path.display()));
        let decompilation = Decompiler::new().decompile_bytes_with_manifest(
            &bytes,
            Some(manifest),
            OutputFormat::All,
        );

        if let Some(invalid) = find_expected_invalid(&id, &expected_invalid) {
            let error = match decompilation {
                Ok(_) => panic!(
                    "expected-invalid artifact {} unexpectedly decompiled",
                    nef_path.display()
                ),
                Err(error) => error,
            };
            if let Some(expected) = &invalid.expected {
                assert!(
                    error.to_string().contains(expected),
                    "expected-invalid artifact {} error did not contain {expected:?}: {error}",
                    nef_path.display()
                );
            }
            invalid_seen.insert(invalid.id.clone());
            continue;
        }

        let decompilation = decompilation
            .unwrap_or_else(|error| panic!("failed to decompile {}: {error}", nef_path.display()));
        let rendered = render_csharp(
            &decompilation.nef,
            &decompilation.instructions,
            decompilation.manifest.as_ref(),
            &decompilation.call_graph,
            &decompilation.method_contracts,
            &decompilation.types,
            &RenderOptions {
                inline_single_use_temps: true,
                emit_trace_comments: false,
                typed_declarations: true,
            },
        );
        decompiled += 1;
        for forbidden in ["phi(", "φ(", "**"] {
            assert!(
                !rendered.source.contains(forbidden),
                "forbidden structured placeholder {forbidden:?} in {}",
                nef_path.display()
            );
        }
        for (start, methods) in &rendered.coverage.methods {
            for coverage in methods.values() {
                if coverage.fidelity.status == Fidelity::Incomplete {
                    incomplete_locations.insert(format!("{id}@0x{start:04X}"));
                }
                *status_counts
                    .entry(coverage.fidelity.status)
                    .or_insert(0usize) += 1;
                for issue in &coverage.fidelity.issues {
                    let issue_key = (issue.fidelity, issue.kind, issue.opcode.mnemonic());
                    *issue_classes.entry(issue_key).or_insert(0usize) += 1;
                    issue_locations
                        .entry(issue_key)
                        .or_default()
                        .insert(format!("{id}@0x{start:04X}"));
                }
                if !matches!(
                    coverage.backend,
                    super::super::csharp::BodyBackend::Structured
                ) {
                    failures.push(format!(
                        "{} @{}: {:?}",
                        nef_path.display(),
                        start,
                        coverage.primary_issue
                    ));
                }
            }
        }
        assert!(
            rendered
                .coverage
                .backend_counts
                .keys()
                .all(|backend| matches!(*backend, "structured" | "throwing_stub")),
            "unexpected C# backend for {}: {:?}",
            nef_path.display(),
            rendered.coverage.backend_counts
        );
    }

    for invalid in &expected_invalid {
        assert!(
            invalid_seen.contains(&invalid.id),
            "stale expected-invalid registry entry: {}",
            invalid.id
        );
    }
    assert!(decompiled > 0, "no repository NEF corpus artifacts found");
    assert_pinned_incomplete_baseline(&root, &incomplete_locations);
    eprintln!(
        "C# fidelity census: {decompiled} contracts, statuses={status_counts:?}, issues={issue_classes:?}"
    );
    for (issue, locations) in issue_locations {
        eprintln!(
            "C# fidelity locations: {issue:?} => {}",
            locations.into_iter().collect::<Vec<_>>().join(", ")
        );
    }
    assert!(
        failures.is_empty(),
        "structured C# fallback corpus:\n{}",
        failures.join("\n")
    );
}

fn assert_pinned_incomplete_baseline(root: &Path, incomplete_locations: &BTreeSet<String>) {
    let Ok(provenance_text) = fs::read_to_string(root.join("provenance.json")) else {
        return;
    };
    let provenance: serde_json::Value = serde_json::from_str(&provenance_text)
        .unwrap_or_else(|error| panic!("parse corpus provenance.json: {error}"));
    if provenance
        .pointer("/source/commit")
        .and_then(serde_json::Value::as_str)
        != Some(PINNED_DEVPACK_COMMIT)
    {
        return;
    }

    let expected = BTreeSet::from(["Contract_Foreach@0x04AC".to_string()]);
    assert_eq!(
        incomplete_locations, &expected,
        "pinned v3.10.0 C# incomplete-method baseline changed"
    );
}
