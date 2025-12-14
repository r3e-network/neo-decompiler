use predicates::str::contains;
use serde_json::Value;
use tempfile::tempdir;

use crate::common::{
    assert_schema, build_nef_with_unknown_opcode, build_sample_nef, neo_decompiler_cmd, SchemaKind,
};

#[test]
fn disasm_command_outputs_instructions() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("contract.nef");
    std::fs::write(&nef_path, build_sample_nef()).unwrap();

    let output = neo_decompiler_cmd()
        .arg("disasm")
        .arg(&nef_path)
        .output()
        .expect("disasm output");
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("0000: PUSH0"));

    let json_output = neo_decompiler_cmd()
        .arg("disasm")
        .arg("--format")
        .arg("json")
        .arg(&nef_path)
        .output()
        .expect("json disasm");
    assert!(json_output.status.success());
    let value: Value = serde_json::from_slice(&json_output.stdout).expect("json parse");
    let instructions = value["instructions"].as_array().expect("array");
    assert_eq!(instructions[0]["opcode"], "PUSH0");
    assert_eq!(instructions[0]["offset"], 0);
    assert_eq!(instructions[0]["operand_kind"], Value::String("I32".into()));
    assert_eq!(
        instructions[0]["operand_value"]["type"],
        Value::String("I32".into())
    );
    assert_eq!(instructions[0]["operand_value"]["value"], Value::from(0));
    assert_eq!(instructions[1]["operand_kind"], Value::String("I32".into()));
    assert_eq!(instructions[1]["operand_value"]["value"], Value::from(1));
    assert!(value["warnings"].is_array());
    assert_schema(SchemaKind::Disasm, &value);
}

#[test]
fn disasm_can_fail_on_unknown_opcodes() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("unknown.nef");
    std::fs::write(&nef_path, build_nef_with_unknown_opcode()).unwrap();

    neo_decompiler_cmd()
        .arg("disasm")
        .arg(&nef_path)
        .assert()
        .success()
        .stdout(contains("UNKNOWN_0xFF"))
        .stdout(contains("0001: RET"));

    neo_decompiler_cmd()
        .arg("disasm")
        .arg("--fail-on-unknown-opcodes")
        .arg(&nef_path)
        .assert()
        .failure()
        .stderr(contains("unknown opcode 0xFF"));
}
