use predicates::str::contains;
use serde_json::Value;
use tempfile::tempdir;

use crate::common::{
    assert_schema, build_nef_with_metadata_controls, build_nef_with_no_tokens, build_sample_nef,
    neo_decompiler_cmd, write_oversize_nef, SchemaKind, CONTROL_METHOD,
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
    // Tokens JSON also surfaces script_hash for cross-correlation
    // with explorer URLs / info reports (parity with disasm and
    // decompile JSON which already exposed it).
    assert_eq!(
        value["script_hash_le"],
        Value::String("EAC5105B64136F5C84AE2C501E586A5AC67DE89D".into())
    );
    assert_eq!(
        value["script_hash_be"],
        Value::String("9DE87DC65A6A581E502CAE845C6F13645B10C5EA".into())
    );
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

#[test]
fn tokens_text_escapes_controls_but_json_keeps_raw_method() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("controls.nef");
    std::fs::write(&nef_path, build_nef_with_metadata_controls()).unwrap();

    let output = neo_decompiler_cmd()
        .arg("tokens")
        .arg(&nef_path)
        .output()
        .expect("text output");
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).expect("UTF-8 text");
    assert!(text.contains("method=m\\r\\n\\u{0085}\\u{2028}\\u{2029}\\u{001B}\\u{202E}INJECT"));
    assert!(text.contains("warning:"));
    for forbidden in [
        '\r', '\u{0085}', '\u{2028}', '\u{2029}', '\u{001B}', '\u{202E}',
    ] {
        assert!(!text.contains(forbidden));
    }

    let json_output = neo_decompiler_cmd()
        .arg("tokens")
        .arg("--format")
        .arg("json")
        .arg(&nef_path)
        .output()
        .expect("JSON output");
    assert!(json_output.status.success());
    let json: Value = serde_json::from_slice(&json_output.stdout).expect("valid JSON");
    assert_eq!(json["method_tokens"][0]["method"], CONTROL_METHOD);
}
