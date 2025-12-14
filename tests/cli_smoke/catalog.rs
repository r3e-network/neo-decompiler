use serde_json::Value;

use crate::common::neo_decompiler_cmd;

#[test]
fn catalog_command_lists_syscalls() {
    let output = neo_decompiler_cmd()
        .arg("catalog")
        .arg("syscalls")
        .output()
        .expect("catalog syscalls");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("System.Runtime.Platform"));
    assert!(stdout.contains("call_flags"));
    assert!(stdout.contains("returns_value"));
}

#[test]
fn catalog_command_supports_syscall_json_output() {
    let output = neo_decompiler_cmd()
        .arg("catalog")
        .arg("syscalls")
        .arg("--format")
        .arg("json")
        .output()
        .expect("catalog syscalls json");
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).expect("json parse");
    assert_eq!(value["kind"], Value::String("syscalls".into()));
    assert!(value["count"].as_u64().unwrap_or(0) > 0);
    let entries = value["entries"].as_array().expect("entries array");
    assert!(!entries.is_empty());
    assert!(entries[0]["name"].is_string());
    assert!(entries[0]["hash"].is_string());
    assert!(entries[0]["handler"].is_string());
    assert!(entries[0]["returns_value"].is_boolean());
}

#[test]
fn catalog_command_supports_native_contract_json_output() {
    let output = neo_decompiler_cmd()
        .arg("catalog")
        .arg("native-contracts")
        .arg("--format")
        .arg("json")
        .output()
        .expect("catalog native contracts json");
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).expect("json parse");
    assert_eq!(value["kind"], Value::String("native-contracts".into()));
    assert!(value["count"].as_u64().unwrap_or(0) > 0);
    let entries = value["entries"].as_array().expect("entries array");
    assert!(!entries.is_empty());
    assert!(entries[0]["methods"].is_array());
}

#[test]
fn catalog_command_lists_opcodes() {
    let output = neo_decompiler_cmd()
        .arg("catalog")
        .arg("opcodes")
        .output()
        .expect("catalog opcodes");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("opcodes bundled"));
    assert!(stdout.contains("PUSH0 (0x10)"));
    assert!(stdout.contains("RET (0x40)"));
}

#[test]
fn catalog_command_supports_opcode_json_output() {
    let output = neo_decompiler_cmd()
        .arg("catalog")
        .arg("opcodes")
        .arg("--format")
        .arg("json")
        .output()
        .expect("catalog opcodes json");
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).expect("json parse");
    assert_eq!(value["kind"], Value::String("opcodes".into()));
    assert!(value["count"].as_u64().unwrap_or(0) > 0);
    let entries = value["entries"].as_array().expect("entries array");
    assert!(!entries.is_empty());
    assert!(entries[0]["mnemonic"].is_string());
    assert!(entries[0]["byte"].is_string());
    assert!(entries[0]["operand_encoding"].is_string());
}
