use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use neo_decompiler::{Decompiler, OutputFormat};
use std::fs;
use std::path::Path;

use crate::artifact::{Artifact, ArtifactEntry, ContractStatus};
use crate::common::create_parent;
use crate::csharp_embed::{
    extract_section, unescape_verbatim, MANIFEST_PREFIX, MANIFEST_SUFFIX, NEF_PREFIX, NEF_SUFFIX,
};
use crate::expected_failures::{find_entry, ExpectedFailure};

pub(crate) fn process_artifact(
    decompiler: &Decompiler,
    artifact: &ArtifactEntry,
    known_unsupported: &[ExpectedFailure],
    expected_invalid: &[ExpectedFailure],
) -> ContractStatus {
    match &artifact.kind {
        Artifact::CSharp { path } => process_csharp_contract(
            decompiler,
            path,
            &artifact.output_base,
            &artifact.id,
            known_unsupported,
            expected_invalid,
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
            expected_invalid,
        ),
    }
}

fn process_csharp_contract(
    decompiler: &Decompiler,
    source_path: &Path,
    output_base: &Path,
    id: &str,
    known_unsupported: &[ExpectedFailure],
    expected_invalid: &[ExpectedFailure],
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
        expected_invalid,
    )
}

fn process_nef_contract(
    decompiler: &Decompiler,
    nef_path: &Path,
    manifest_path: &Path,
    output_base: &Path,
    id: &str,
    known_unsupported: &[ExpectedFailure],
    expected_invalid: &[ExpectedFailure],
) -> ContractStatus {
    assert!(nef_path.is_file(), "NEF missing: {}", nef_path.display());
    assert!(
        manifest_path.is_file(),
        "Manifest missing: {}",
        manifest_path.display()
    );

    decompile_and_write_outputs(
        decompiler,
        id,
        nef_path,
        manifest_path,
        output_base,
        known_unsupported,
        expected_invalid,
    )
}

fn decompile_and_write_outputs(
    decompiler: &Decompiler,
    id: &str,
    nef_path: &Path,
    manifest_path: &Path,
    output_base: &Path,
    known_unsupported: &[ExpectedFailure],
    expected_invalid: &[ExpectedFailure],
) -> ContractStatus {
    let high_level_path = output_base.with_extension("high-level.cs");
    let pseudocode_path = output_base.with_extension("pseudocode.txt");
    let csharp_path = output_base.with_extension("csharp.cs");
    let error_path = output_base.with_extension("error.txt");

    create_parent(&high_level_path);
    create_parent(&pseudocode_path);
    create_parent(&csharp_path);
    create_parent(&error_path);

    match decompiler.decompile_file_with_manifest(nef_path, Some(manifest_path), OutputFormat::All)
    {
        Ok(result) => {
            assert!(
                find_entry(id, expected_invalid).is_none(),
                "expected-invalid artifact {id} unexpectedly decompiled"
            );
            assert!(
                find_entry(id, known_unsupported).is_none(),
                "known-unsupported artifact {id} now decompiles; remove its registry entry"
            );
            let high_level = result.high_level.as_deref().unwrap_or_default();
            let pseudocode = result.pseudocode.as_deref().unwrap_or_default();
            let csharp = result.csharp.as_deref().unwrap_or_default();
            fs::write(&high_level_path, high_level.as_bytes()).unwrap_or_else(|err| {
                panic!("failed to write {}: {err}", high_level_path.display())
            });
            fs::write(&pseudocode_path, pseudocode.as_bytes()).unwrap_or_else(|err| {
                panic!("failed to write {}: {err}", pseudocode_path.display())
            });
            fs::write(&csharp_path, csharp.as_bytes())
                .unwrap_or_else(|err| panic!("failed to write {}: {err}", csharp_path.display()));
            ContractStatus::Success
        }
        Err(err) => {
            if find_entry(id, expected_invalid).is_some() {
                fs::write(&error_path, err.to_string()).unwrap_or_else(|io_err| {
                    panic!("failed to write {}: {io_err}", error_path.display())
                });
                eprintln!("Rejected expected-invalid artifact {id}: {err}");
                ContractStatus::ExpectedInvalid
            } else if find_entry(id, known_unsupported).is_some() {
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
