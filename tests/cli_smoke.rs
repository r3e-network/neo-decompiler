use std::io::Write;

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::NamedTempFile;

fn build_sample_nef() -> Vec<u8> {
    let script = [0x10, 0x11, 0x9E, 0x40];
    let mut data = Vec::new();
    data.extend_from_slice(b"NEF3");
    let mut compiler = [0u8; 32];
    compiler[..4].copy_from_slice(b"test");
    data.extend_from_slice(&compiler);
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(&(script.len() as u32).to_le_bytes());
    // single method token
    data.push(1);
    data.extend_from_slice(&[0x11; 20]);
    data.push(3); // method name length
    data.extend_from_slice(b"foo");
    data.push(2); // params
    data.push(0x21); // return type
    data.push(0x0F); // call flags
    data.extend_from_slice(&script);
    let checksum = neo_decompiler::nef::NefParser::calculate_checksum(&data);
    data.extend_from_slice(&checksum.to_le_bytes());
    data
}

fn build_nef_with_no_tokens() -> Vec<u8> {
    let script = [0x40];
    let mut data = Vec::new();
    data.extend_from_slice(b"NEF3");
    let mut compiler = [0u8; 32];
    compiler[..4].copy_from_slice(b"test");
    data.extend_from_slice(&compiler);
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(&(script.len() as u32).to_le_bytes());
    data.push(0);
    data.extend_from_slice(&script);
    let checksum = neo_decompiler::nef::NefParser::calculate_checksum(&data);
    data.extend_from_slice(&checksum.to_le_bytes());
    data
}

#[test]
fn info_command_prints_header() {
    let mut file = NamedTempFile::new().expect("tempfile");
    file.write_all(&build_sample_nef()).unwrap();

    Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("info")
        .arg(file.path())
        .assert()
        .success()
        .stdout(contains("Method tokens: 1"));
}

#[test]
fn disasm_command_outputs_instructions() {
    let mut file = NamedTempFile::new().expect("tempfile");
    file.write_all(&build_sample_nef()).unwrap();

    Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("disasm")
        .arg(file.path())
        .assert()
        .success()
        .stdout(contains("0000: PUSH0"));
}

#[test]
fn decompile_command_outputs_pseudocode() {
    let mut file = NamedTempFile::new().expect("tempfile");
    file.write_all(&build_sample_nef()).unwrap();

    Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("decompile")
        .arg(file.path())
        .assert()
        .success()
        .stdout(contains("ADD"));
}

#[test]
fn tokens_command_lists_entries() {
    let mut file = NamedTempFile::new().expect("tempfile");
    file.write_all(&build_sample_nef()).unwrap();

    Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("tokens")
        .arg(file.path())
        .assert()
        .success()
        .stdout(contains("method=foo"));
}

#[test]
fn tokens_command_handles_empty() {
    let mut file = NamedTempFile::new().expect("tempfile");
    file.write_all(&build_nef_with_no_tokens()).unwrap();

    Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("tokens")
        .arg(file.path())
        .assert()
        .success()
        .stdout(contains("no method tokens"));
}
