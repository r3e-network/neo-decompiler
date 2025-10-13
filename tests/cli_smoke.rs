use assert_cmd::Command;
use predicates::str::contains;
use tempfile::tempdir;

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
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("contract.nef");
    std::fs::write(&nef_path, build_sample_nef()).unwrap();

    Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("info")
        .arg(&nef_path)
        .assert()
        .success()
        .stdout(contains("Method tokens: 1"));
}

#[test]
fn disasm_command_outputs_instructions() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("contract.nef");
    std::fs::write(&nef_path, build_sample_nef()).unwrap();

    Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("disasm")
        .arg(&nef_path)
        .assert()
        .success()
        .stdout(contains("0000: PUSH0"));
}

#[test]
fn decompile_command_outputs_high_level_by_default() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("contract.nef");
    std::fs::write(&nef_path, build_sample_nef()).unwrap();

    Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("decompile")
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

    Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("decompile")
        .arg("--format")
        .arg("pseudocode")
        .arg(&nef_path)
        .assert()
        .success()
        .stdout(contains("ADD"));
}

#[test]
fn decompile_command_uses_manifest_when_provided() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("contract.nef");
    let manifest_path = dir.path().join("custom.manifest.json");

    std::fs::write(&nef_path, build_sample_nef()).unwrap();
    std::fs::write(&manifest_path, SAMPLE_MANIFEST).unwrap();

    Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("--manifest")
        .arg(&manifest_path)
        .arg("decompile")
        .arg(&nef_path)
        .assert()
        .success()
        .stdout(contains("contract SampleToken"));
}

#[test]
fn tokens_command_lists_entries() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("contract.nef");
    std::fs::write(&nef_path, build_sample_nef()).unwrap();

    Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("tokens")
        .arg(&nef_path)
        .assert()
        .success()
        .stdout(contains("method=foo"));
}

#[test]
fn tokens_command_handles_empty() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("contract.nef");
    std::fs::write(&nef_path, build_nef_with_no_tokens()).unwrap();

    Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("tokens")
        .arg(&nef_path)
        .assert()
        .success()
        .stdout(contains("no method tokens"));
}

#[test]
fn info_command_loads_manifest_when_available() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("contract.nef");
    let manifest_path = dir.path().join("contract.manifest.json");

    std::fs::write(&nef_path, build_sample_nef()).unwrap();
    std::fs::write(&manifest_path, SAMPLE_MANIFEST).unwrap();

    Command::cargo_bin("neo-decompiler")
        .unwrap()
        .arg("info")
        .arg(&nef_path)
        .assert()
        .success()
        .stdout(contains("Manifest contract: SampleToken"));
}

const SAMPLE_MANIFEST: &str = r#"
{
    "name": "SampleToken",
    "supportedstandards": ["NEP-17"],
    "features": { "storage": true, "payable": false },
    "abi": {
        "methods": [
            {
                "name": "symbol",
                "parameters": [],
                "returntype": "String",
                "offset": 0,
                "safe": true
            }
        ],
        "events": []
    },
    "permissions": [],
    "trusts": "*"
}
"#;
