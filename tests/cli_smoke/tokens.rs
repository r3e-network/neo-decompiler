use predicates::str::contains;
use serde_json::Value;
use tempfile::tempdir;

use crate::common::{
    assert_schema, build_nef_with_no_tokens, build_sample_nef, neo_decompiler_cmd,
    write_oversize_nef, SchemaKind,
};

#[test]
fn tokens_command_lists_entries() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("contract.nef");
    std::fs::write(&nef_path, build_sample_nef()).unwrap();

    neo_decompiler_cmd()
        .arg("tokens")
        .arg(&nef_path)
        .assert()
        .success()
        .stdout(contains("method=Transfer"))
        .stdout(contains("GasToken::Transfer"))
        .stdout(contains("AllowCall"));
}

#[test]
fn tokens_command_supports_json_output() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("contract.nef");
    std::fs::write(&nef_path, build_sample_nef()).unwrap();

    let output = neo_decompiler_cmd()
        .arg("tokens")
        .arg("--format")
        .arg("json")
        .arg(&nef_path)
        .output()
        .expect("json output");
    assert!(output.status.success());

    let value: Value = serde_json::from_slice(&output.stdout).expect("json parse");
    let tokens = value["method_tokens"].as_array().expect("array");
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0]["native_contract"]["label"], "GasToken::Transfer");
    assert!(value["warnings"].is_array());
    assert_schema(SchemaKind::Tokens, &value);
}

#[test]
fn tokens_command_handles_empty() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("contract.nef");
    std::fs::write(&nef_path, build_nef_with_no_tokens()).unwrap();

    neo_decompiler_cmd()
        .arg("tokens")
        .arg(&nef_path)
        .assert()
        .success()
        .stdout(contains("no method tokens"));
}

#[test]
fn tokens_command_rejects_large_nef() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("oversize.nef");
    write_oversize_nef(&nef_path);

    neo_decompiler_cmd()
        .arg("tokens")
        .arg(&nef_path)
        .assert()
        .failure()
        .stderr(contains("file size"))
        .stderr(contains("exceeds maximum"));
}
