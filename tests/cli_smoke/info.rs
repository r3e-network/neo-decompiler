use predicates::str::contains;
use serde_json::Value;
use tempfile::tempdir;

use crate::common::{
    assert_schema, build_sample_nef, neo_decompiler_cmd, write_oversize_nef, SchemaKind,
    SAMPLE_MANIFEST,
};

#[test]
fn info_command_prints_header() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("contract.nef");
    std::fs::write(&nef_path, build_sample_nef()).unwrap();

    neo_decompiler_cmd()
        .arg("info")
        .arg(&nef_path)
        .assert()
        .success()
        .stdout(contains("Method tokens: 1"))
        .stdout(contains("#0: hash="))
        .stdout(contains("(GasToken::Transfer)"))
        .stdout(contains(
            "Script hash (LE): 9DE87DC65A6A581E502CAE845C6F13645B10C5EA",
        ))
        .stdout(contains(
            "Script hash (BE): EAC5105B64136F5C84AE2C501E586A5AC67DE89D",
        ))
        .stdout(contains(
            "flags=0x0F (ReadStates|WriteStates|AllowCall|AllowNotify)",
        ));
}

#[test]
fn info_command_supports_json_output() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("contract.nef");
    let manifest_path = dir.path().join("contract.manifest.json");
    std::fs::write(&nef_path, build_sample_nef()).unwrap();
    std::fs::write(&manifest_path, SAMPLE_MANIFEST).unwrap();

    let output = neo_decompiler_cmd()
        .arg("info")
        .arg("--format")
        .arg("json")
        .arg(&nef_path)
        .output()
        .expect("json output");

    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).expect("valid json");
    assert_eq!(
        value["script_hash_le"],
        Value::String("9DE87DC65A6A581E502CAE845C6F13645B10C5EA".into())
    );
    let tokens = value["method_tokens"].as_array().expect("tokens array");
    assert!(!tokens.is_empty());
    assert_eq!(
        value["manifest"]["abi"]["methods"][0]["name"],
        Value::String("symbol".into())
    );
    assert_eq!(
        value["manifest"]["permissions"][0]["contract"]["type"],
        Value::String("Hash".into())
    );
    assert_eq!(
        value["manifest"]["trusts"]["type"],
        Value::String("Contracts".into())
    );
    assert_eq!(
        value["manifest_path"],
        Value::String(manifest_path.display().to_string())
    );
    assert!(value["warnings"].is_array());
    assert_eq!(
        value["manifest"]["permissions"][1]["contract"]["type"],
        Value::String("Group".into())
    );
    assert_eq!(
        value["manifest"]["permissions"][1]["contract"]["value"],
        Value::String("03ABCD".into())
    );
    assert_eq!(
        value["manifest"]["groups"][0]["pubkey"],
        Value::String("039999999999999999999999999999999999999999999999999999999999999999".into())
    );
    assert_eq!(
        value["manifest"]["groups"][0]["signature"],
        Value::String("deadbeef".into())
    );
    assert_eq!(tokens[0]["returns_value"], Value::Bool(true));
    assert_schema(SchemaKind::Info, &value);

    let compact = neo_decompiler_cmd()
        .arg("info")
        .arg("--format")
        .arg("json")
        .arg("--json-compact")
        .arg(&nef_path)
        .output()
        .expect("compact json output");
    assert!(compact.status.success());
    assert!(
        compact.stdout.len() < output.stdout.len(),
        "compact json should be shorter"
    );
    let compact_value: Value = serde_json::from_slice(&compact.stdout).expect("compact json parse");
    assert_eq!(value, compact_value);
}

#[test]
fn info_command_loads_manifest_when_available() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("contract.nef");
    let manifest_path = dir.path().join("contract.manifest.json");

    std::fs::write(&nef_path, build_sample_nef()).unwrap();
    std::fs::write(&manifest_path, SAMPLE_MANIFEST).unwrap();

    neo_decompiler_cmd()
        .arg("info")
        .arg(&nef_path)
        .assert()
        .success()
        .stdout(contains("Manifest contract: SampleToken"))
        .stdout(contains("Groups:"))
        .stdout(contains(
            "pubkey=039999999999999999999999999999999999999999999999999999999999999999",
        ))
        .stdout(contains("Permissions:"))
        .stdout(contains("Trusts:"));
}

#[test]
fn info_command_rejects_large_nef() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("oversize.nef");
    write_oversize_nef(&nef_path);

    neo_decompiler_cmd()
        .arg("info")
        .arg(&nef_path)
        .assert()
        .failure()
        .stderr(contains("file size"))
        .stderr(contains("exceeds maximum"));
}

#[test]
fn info_command_strict_manifest_rejects_invalid_manifest_values() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("contract.nef");
    let manifest_path = dir.path().join("contract.manifest.json");

    std::fs::write(&nef_path, build_sample_nef()).unwrap();
    std::fs::write(
        &manifest_path,
        r#"
        {
            "name": "InvalidTrusts",
            "abi": { "methods": [], "events": [] },
            "trusts": "invalid"
        }
        "#,
    )
    .unwrap();

    neo_decompiler_cmd()
        .arg("--manifest")
        .arg(&manifest_path)
        .arg("--strict-manifest")
        .arg("info")
        .arg(&nef_path)
        .assert()
        .failure()
        .stderr(contains("manifest validation error"));
}
