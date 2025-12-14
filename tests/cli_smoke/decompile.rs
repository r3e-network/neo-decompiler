use predicates::str::contains;
use serde_json::Value;
use tempfile::tempdir;

use crate::common::{
    assert_schema, build_nef_with_unknown_opcode, build_sample_nef, neo_decompiler_cmd, SchemaKind,
    SAMPLE_MANIFEST,
};

#[test]
fn decompile_command_outputs_high_level_by_default() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("contract.nef");
    std::fs::write(&nef_path, build_sample_nef()).unwrap();

    neo_decompiler_cmd()
        .arg("decompile")
        .arg(&nef_path)
        .assert()
        .success()
        .stdout(contains("contract NeoContract"))
        .stdout(contains("GasToken::Transfer"));
}

#[test]
fn decompile_command_accepts_inline_single_use_temps_flag() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("contract.nef");
    std::fs::write(&nef_path, build_sample_nef()).unwrap();

    neo_decompiler_cmd()
        .arg("decompile")
        .arg("--inline-single-use-temps")
        .arg(&nef_path)
        .assert()
        .success()
        .stdout(contains("contract NeoContract"));
}

#[test]
fn decompile_command_supports_pseudocode_format() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("contract.nef");
    std::fs::write(&nef_path, build_sample_nef()).unwrap();

    neo_decompiler_cmd()
        .arg("decompile")
        .arg("--format")
        .arg("pseudocode")
        .arg(&nef_path)
        .assert()
        .success()
        .stdout(contains("ADD"));
}

#[test]
fn decompile_can_fail_on_unknown_opcodes() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("unknown.nef");
    std::fs::write(&nef_path, build_nef_with_unknown_opcode()).unwrap();

    neo_decompiler_cmd()
        .arg("decompile")
        .arg("--format")
        .arg("pseudocode")
        .arg(&nef_path)
        .assert()
        .success()
        .stdout(contains("UNKNOWN_0xFF"));

    neo_decompiler_cmd()
        .arg("decompile")
        .arg("--format")
        .arg("pseudocode")
        .arg("--fail-on-unknown-opcodes")
        .arg(&nef_path)
        .assert()
        .failure()
        .stderr(contains("unknown opcode 0xFF"));
}

#[test]
fn decompile_command_supports_csharp_format() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("contract.nef");
    let manifest_path = dir.path().join("contract.manifest.json");
    std::fs::write(&nef_path, build_sample_nef()).unwrap();
    std::fs::write(&manifest_path, SAMPLE_MANIFEST).unwrap();

    neo_decompiler_cmd()
        .arg("--manifest")
        .arg(&manifest_path)
        .arg("decompile")
        .arg("--format")
        .arg("csharp")
        .arg(&nef_path)
        .assert()
        .success()
        .stdout(contains("namespace NeoDecompiler.Generated"))
        .stdout(contains("public static string symbol()"));
}

#[test]
fn decompile_command_supports_json_format() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("contract.nef");
    let manifest_path = dir.path().join("contract.manifest.json");
    std::fs::write(&nef_path, build_sample_nef()).unwrap();
    std::fs::write(&manifest_path, SAMPLE_MANIFEST).unwrap();

    let output = neo_decompiler_cmd()
        .arg("--manifest")
        .arg(&manifest_path)
        .arg("decompile")
        .arg("--format")
        .arg("json")
        .arg(&nef_path)
        .output()
        .expect("json decompile");
    assert!(output.status.success());

    let value: Value = serde_json::from_slice(&output.stdout).expect("json parse");
    assert!(value["high_level"]
        .as_str()
        .expect("string")
        .contains("contract SampleToken"));
    assert_eq!(
        value["manifest_path"],
        Value::String(manifest_path.display().to_string())
    );
    assert!(value["csharp"]
        .as_str()
        .expect("csharp string")
        .contains("namespace NeoDecompiler.Generated"));
    assert!(value["csharp"]
        .as_str()
        .expect("csharp string")
        .contains("[ManifestExtra(\"Author\", \"Example Author\")]"));
    assert_eq!(
        value["manifest"]["abi"]["methods"][0]["name"],
        Value::String("symbol".into())
    );
    assert_eq!(
        value["instructions"][0]["opcode"],
        Value::String("PUSH0".into())
    );
    assert_eq!(
        value["instructions"][0]["operand_value"]["value"],
        Value::from(0)
    );
    assert_eq!(
        value["manifest"]["permissions"][0]["contract"]["type"],
        Value::String("Hash".into())
    );
    assert_eq!(
        value["manifest"]["trusts"]["type"],
        Value::String("Contracts".into())
    );
    assert!(value["warnings"].is_array());
    let tokens = value["method_tokens"].as_array().expect("tokens array");
    assert_eq!(tokens[0]["returns_value"], Value::Bool(true));
    assert_eq!(
        value["manifest"]["groups"][0]["pubkey"],
        Value::String("039999999999999999999999999999999999999999999999999999999999999999".into())
    );
    assert!(value["analysis"]["call_graph"]["methods"].is_array());
    assert!(value["analysis"]["call_graph"]["edges"].is_array());
    assert!(value["analysis"]["xrefs"]["methods"].is_array());
    assert!(value["analysis"]["types"]["methods"].is_array());
    assert_schema(SchemaKind::Decompile, &value);
}

#[test]
fn decompile_command_uses_manifest_when_provided() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("contract.nef");
    let manifest_path = dir.path().join("custom.manifest.json");

    std::fs::write(&nef_path, build_sample_nef()).unwrap();
    std::fs::write(&manifest_path, SAMPLE_MANIFEST).unwrap();

    neo_decompiler_cmd()
        .arg("--manifest")
        .arg(&manifest_path)
        .arg("decompile")
        .arg(&nef_path)
        .assert()
        .success()
        .stdout(contains("contract SampleToken"))
        .stdout(contains("permissions {"))
        .stdout(contains("trusts = ["));
}
