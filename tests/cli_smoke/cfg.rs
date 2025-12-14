use predicates::str::contains;
use tempfile::tempdir;

use crate::common::{build_nef_with_unknown_opcode, build_sample_nef, neo_decompiler_cmd};

#[test]
fn cfg_command_outputs_dot() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("contract.nef");
    std::fs::write(&nef_path, build_sample_nef()).unwrap();

    neo_decompiler_cmd()
        .arg("cfg")
        .arg(&nef_path)
        .assert()
        .success()
        .stdout(contains("digraph CFG"))
        .stdout(contains("BB0"));
}

#[test]
fn cfg_can_fail_on_unknown_opcodes() {
    let dir = tempdir().expect("tempdir");
    let nef_path = dir.path().join("unknown.nef");
    std::fs::write(&nef_path, build_nef_with_unknown_opcode()).unwrap();

    neo_decompiler_cmd()
        .arg("cfg")
        .arg(&nef_path)
        .assert()
        .success()
        .stdout(contains("digraph CFG"));

    neo_decompiler_cmd()
        .arg("cfg")
        .arg("--fail-on-unknown-opcodes")
        .arg(&nef_path)
        .assert()
        .failure()
        .stderr(contains("unknown opcode 0xFF"));
}
