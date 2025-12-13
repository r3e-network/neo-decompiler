use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use neo_decompiler::{Decompiler, OutputFormat};
use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};

const MANIFEST_PREFIX: &str = "ContractManifest.Parse(@\"";
const MANIFEST_SUFFIX: &str = "\");";
const NEF_PREFIX: &str = "Convert.FromBase64String(@\"";
const NEF_SUFFIX: &str = "\")";

#[derive(Debug, Clone, PartialEq, Eq)]
struct KnownUnsupported {
    id: String,
    expected: Option<String>,
}

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
        .filter(|artifact| is_known_unsupported(artifact, &known_unsupported))
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

#[derive(PartialEq, Eq)]
enum ContractStatus {
    Success,
    KnownUnsupported,
}

#[derive(Debug)]
enum Artifact {
    CSharp {
        path: PathBuf,
    },
    NefManifest {
        nef_path: PathBuf,
        manifest_path: PathBuf,
    },
}

#[derive(Debug)]
struct ArtifactEntry {
    kind: Artifact,
    id: String,
    output_base: PathBuf,
}

fn collect_artifacts(artifacts_dir: &Path, output_dir: &Path) -> Vec<ArtifactEntry> {
    let mut artifacts = Vec::new();
    let mut stack = vec![artifacts_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("list artifacts") {
            let entry = entry.expect("artifact entry");
            let path = entry.path();
            if path.starts_with(output_dir) {
                continue;
            }
            if entry.file_type().expect("file type").is_dir() {
                stack.push(path);
                continue;
            }
            match path.extension().and_then(OsStr::to_str) {
                Some("cs") => {
                    let rel_base = relative_base(&path, artifacts_dir);
                    artifacts.push(ArtifactEntry {
                        id: format_id(&rel_base),
                        output_base: output_dir.join(&rel_base),
                        kind: Artifact::CSharp { path },
                    });
                }
                Some("nef") => {
                    let rel_base = relative_base(&path, artifacts_dir);
                    let manifest_path = path.with_extension("manifest.json");
                    if manifest_path.is_file() {
                        artifacts.push(ArtifactEntry {
                            id: format_id(&rel_base),
                            output_base: output_dir.join(&rel_base),
                            kind: Artifact::NefManifest {
                                nef_path: path,
                                manifest_path,
                            },
                        });
                    } else {
                        eprintln!(
                            "Skipping {}: missing manifest {}",
                            path.display(),
                            manifest_path.display()
                        );
                    }
                }
                _ => {}
            }
        }
    }
    artifacts
}

fn process_artifact(
    decompiler: &Decompiler,
    artifact: &ArtifactEntry,
    known_unsupported: &[KnownUnsupported],
) -> ContractStatus {
    match &artifact.kind {
        Artifact::CSharp { path } => process_csharp_contract(
            decompiler,
            path,
            &artifact.output_base,
            &artifact.id,
            known_unsupported,
        ),
        Artifact::NefManifest {
            nef_path,
            manifest_path,
        } => process_nef_contract(
            decompiler,
            nef_path,
            manifest_path,
            &artifact.output_base,
            &artifact.id,
            known_unsupported,
        ),
    }
}

fn process_csharp_contract(
    decompiler: &Decompiler,
    source_path: &Path,
    output_base: &Path,
    id: &str,
    known_unsupported: &[KnownUnsupported],
) -> ContractStatus {
    let source = fs::read_to_string(source_path).unwrap_or_else(|err| {
        panic!("failed to read {}: {err}", source_path.display());
    });

    let manifest_raw = extract_section(&source, MANIFEST_PREFIX, MANIFEST_SUFFIX)
        .unwrap_or_else(|| panic!("manifest section not found in {}", source_path.display()));
    let manifest_json = unescape_verbatim(manifest_raw);

    let nef_base64 = extract_section(&source, NEF_PREFIX, NEF_SUFFIX)
        .unwrap_or_else(|| panic!("NEF section not found in {}", source_path.display()));
    let nef_base64 = unescape_verbatim(nef_base64)
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>();
    let nef_bytes = BASE64
        .decode(nef_base64)
        .unwrap_or_else(|err| panic!("invalid base64 in {}: {err}", source_path.display()));

    let nef_path = output_base.with_extension("nef");
    let manifest_path = output_base.with_extension("manifest.json");

    create_parent(&nef_path);
    create_parent(&manifest_path);

    fs::write(&nef_path, &nef_bytes)
        .unwrap_or_else(|err| panic!("failed to write {}: {err}", nef_path.display()));
    fs::write(&manifest_path, manifest_json.as_bytes())
        .unwrap_or_else(|err| panic!("failed to write {}: {err}", manifest_path.display()));

    decompile_and_write_outputs(
        decompiler,
        id,
        &nef_path,
        &manifest_path,
        output_base,
        known_unsupported,
    )
}

fn process_nef_contract(
    decompiler: &Decompiler,
    nef_path: &Path,
    manifest_path: &Path,
    output_base: &Path,
    id: &str,
    known_unsupported: &[KnownUnsupported],
) -> ContractStatus {
    if !nef_path.is_file() {
        panic!("NEF missing: {}", nef_path.display());
    }
    if !manifest_path.is_file() {
        panic!("Manifest missing: {}", manifest_path.display());
    }

    decompile_and_write_outputs(
        decompiler,
        id,
        nef_path,
        manifest_path,
        output_base,
        known_unsupported,
    )
}

fn decompile_and_write_outputs(
    decompiler: &Decompiler,
    id: &str,
    nef_path: &Path,
    manifest_path: &Path,
    output_base: &Path,
    known_unsupported: &[KnownUnsupported],
) -> ContractStatus {
    let high_level_path = output_base.with_extension("high-level.cs");
    let pseudocode_path = output_base.with_extension("pseudocode.txt");
    let error_path = output_base.with_extension("error.txt");

    create_parent(&high_level_path);
    create_parent(&pseudocode_path);
    create_parent(&error_path);

    match decompiler.decompile_file_with_manifest(nef_path, Some(manifest_path), OutputFormat::All)
    {
        Ok(result) => {
            let high_level = result.high_level.as_deref().unwrap_or_default();
            let pseudocode = result.pseudocode.as_deref().unwrap_or_default();
            fs::write(&high_level_path, high_level.as_bytes()).unwrap_or_else(|err| {
                panic!("failed to write {}: {err}", high_level_path.display())
            });
            fs::write(&pseudocode_path, pseudocode.as_bytes()).unwrap_or_else(|err| {
                panic!("failed to write {}: {err}", pseudocode_path.display())
            });
            ContractStatus::Success
        }
        Err(err) => {
            if find_known_entry(id, known_unsupported).is_some() {
                fs::write(&error_path, err.to_string()).unwrap_or_else(|io_err| {
                    panic!("failed to write {}: {io_err}", error_path.display())
                });
                eprintln!("Skipping {id} due to known limitation: {err}");
                ContractStatus::KnownUnsupported
            } else {
                panic!("failed to decompile {}: {err}", nef_path.display());
            }
        }
    }
}

fn load_known_unsupported(artifacts_dir: &Path) -> Vec<KnownUnsupported> {
    const DEFAULT: &[&str] = &["Contract_Delegate", "Contract_Lambda"];

    let mut entries = DEFAULT
        .iter()
        .map(|s| KnownUnsupported {
            id: s.to_string(),
            expected: None,
        })
        .collect::<Vec<KnownUnsupported>>();

    let path = artifacts_dir.join("known_unsupported.txt");
    if let Ok(contents) = fs::read_to_string(&path) {
        for line in contents.lines() {
            let trimmed = line.split('#').next().map(str::trim).unwrap_or_default();
            if trimmed.is_empty() {
                continue;
            }
            let (id, expected) = if let Some((id, expected)) = trimmed.split_once(':') {
                (id.trim().to_string(), Some(expected.trim().to_string()))
            } else {
                (trimmed.to_string(), None)
            };
            entries.push(KnownUnsupported { id, expected });
        }
    }

    entries.sort_by(|a, b| a.id.cmp(&b.id));
    entries.dedup_by(|a, b| a.id == b.id && a.expected == b.expected);
    entries
}

fn extract_section<'a>(source: &'a str, prefix: &str, suffix: &str) -> Option<&'a str> {
    let start = source.find(prefix)? + prefix.len();
    let rest = &source[start..];
    let end = rest.find(suffix)?;
    Some(rest[..end].trim())
}

fn unescape_verbatim(input: &str) -> String {
    input.replace("\"\"", "\"")
}

fn relative_base(path: &Path, root: &Path) -> PathBuf {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.with_extension("")
}

fn format_id(path: &Path) -> String {
    path.components()
        .filter_map(|c| match c {
            Component::Normal(os) => os.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn is_known_unsupported(artifact: &ArtifactEntry, known_unsupported: &[KnownUnsupported]) -> bool {
    find_known_entry(&artifact.id, known_unsupported).is_some()
}

fn find_known_entry<'a>(
    id: &str,
    known_unsupported: &'a [KnownUnsupported],
) -> Option<&'a KnownUnsupported> {
    let basename = id.rsplit('/').next();
    known_unsupported
        .iter()
        .find(|entry| entry.id == id || basename.map(|name| name == entry.id).unwrap_or(false))
}

fn find_expected_message<'a>(id: &str, known: &'a [KnownUnsupported]) -> Option<&'a str> {
    find_known_entry(id, known).and_then(|entry| entry.expected.as_deref())
}

fn create_parent(path: &Path) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create output directories");
    }
}

fn assert_non_empty(path: &Path, msg: &str) {
    assert!(path.is_file(), "{msg}: {}", path.display());
    let contents = fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
    assert!(!contents.trim().is_empty(), "{msg}: {}", path.display());
}
