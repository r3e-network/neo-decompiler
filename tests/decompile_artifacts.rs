use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use neo_decompiler::Decompiler;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;

const MANIFEST_PREFIX: &str = "ContractManifest.Parse(@\"";
const MANIFEST_SUFFIX: &str = "\");";
const NEF_PREFIX: &str = "Convert.FromBase64String(@\"";
const NEF_SUFFIX: &str = "\")";

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

    for entry in fs::read_dir(&artifacts_dir).expect("list artifacts") {
        let entry = entry.expect("artifact entry");
        if entry.file_type().expect("file type").is_file()
            && entry.path().extension() == Some(OsStr::new("cs"))
        {
            let path = entry.path();
            let name = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .expect("file stem");
            match process_contract(&decompiler, &path, &output_dir, name) {
                ContractStatus::Success => processed += 1,
                ContractStatus::KnownUnsupported => skipped.push(name.to_string()),
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
    let expected: Vec<&str> = KNOWN_UNSUPPORTED.iter().copied().collect();
    assert_eq!(
        skipped, expected,
        "unexpected contracts failed to decompile"
    );
}

const KNOWN_UNSUPPORTED: &[&str] = &["Contract_Delegate", "Contract_Lambda"];

#[derive(PartialEq, Eq)]
enum ContractStatus {
    Success,
    KnownUnsupported,
}

fn process_contract(
    decompiler: &Decompiler,
    source_path: &Path,
    output_dir: &Path,
    name: &str,
) -> ContractStatus {
    let source = fs::read_to_string(source_path).unwrap_or_else(|err| {
        panic!("failed to read {}: {err}", source_path.display());
    });

    let manifest_raw = extract_section(&source, MANIFEST_PREFIX, MANIFEST_SUFFIX)
        .unwrap_or_else(|| panic!("manifest section not found in {}", source_path.display()));
    let manifest_json = unescape_verbatim(manifest_raw);

    let nef_base64 = extract_section(&source, NEF_PREFIX, NEF_SUFFIX)
        .unwrap_or_else(|| panic!("NEF section not found in {}", source_path.display()));
    let nef_base64 = unescape_verbatim(&nef_base64)
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>();
    let nef_bytes = BASE64
        .decode(nef_base64)
        .unwrap_or_else(|err| panic!("invalid base64 in {}: {err}", source_path.display()));

    let nef_path = output_dir.join(format!("{name}.nef"));
    let manifest_path = output_dir.join(format!("{name}.manifest.json"));
    let high_level_path = output_dir.join(format!("{name}.high-level.cs"));
    let pseudocode_path = output_dir.join(format!("{name}.pseudocode.txt"));

    fs::write(&nef_path, &nef_bytes)
        .unwrap_or_else(|err| panic!("failed to write {}: {err}", nef_path.display()));
    fs::write(&manifest_path, manifest_json.as_bytes())
        .unwrap_or_else(|err| panic!("failed to write {}: {err}", manifest_path.display()));

    match decompiler.decompile_file_with_manifest(&nef_path, Some(&manifest_path)) {
        Ok(result) => {
            fs::write(&high_level_path, result.high_level.as_bytes()).unwrap_or_else(|err| {
                panic!("failed to write {}: {err}", high_level_path.display())
            });
            fs::write(&pseudocode_path, result.pseudocode.as_bytes()).unwrap_or_else(|err| {
                panic!("failed to write {}: {err}", pseudocode_path.display())
            });
            ContractStatus::Success
        }
        Err(err) => {
            if KNOWN_UNSUPPORTED.contains(&name) {
                let error_path = output_dir.join(format!("{name}.error.txt"));
                fs::write(&error_path, err.to_string()).unwrap_or_else(|io_err| {
                    panic!("failed to write {}: {io_err}", error_path.display())
                });
                eprintln!("Skipping {name} due to known limitation: {err}");
                ContractStatus::KnownUnsupported
            } else {
                panic!("failed to decompile {}: {err}", source_path.display());
            }
        }
    }
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
