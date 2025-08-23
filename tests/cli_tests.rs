//! CLI interface tests
//! 
//! Tests the command-line interface using assert_cmd to verify correct behavior
//! of all CLI commands and arguments.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;
use crate::common::*;

/// Helper to create a test command
fn neo_decompile_cmd() -> Command {
    Command::cargo_bin("neo-decompiler").unwrap()
}

/// Test basic help output
#[test]
fn test_help_output() {
    neo_decompile_cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Neo N3 Smart Contract Decompiler"))
        .stdout(predicate::str::contains("disasm"))
        .stdout(predicate::str::contains("decompile"))
        .stdout(predicate::str::contains("analyze"));
}

/// Test version output
#[test]
fn test_version_output() {
    neo_decompile_cmd()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("0.1.0"));
}

/// Test disasm command with sample NEF
#[test]
fn test_disasm_command() {
    let temp_dir = TempDir::new().unwrap();
    let sample_nef = SampleNefData::minimal();
    let nef_bytes = sample_nef.to_bytes();
    let nef_path = temp_dir.path().join("test.nef");
    fs::write(&nef_path, &nef_bytes).unwrap();
    
    neo_decompile_cmd()
        .arg("disasm")
        .arg(&nef_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("PUSH").or(predicate::str::is_match(r"[0-9a-fA-F]+:").unwrap()));
}

/// Test disasm command with output file
#[test] 
fn test_disasm_with_output_file() {
    let temp_dir = TempDir::new().unwrap();
    let sample_nef = SampleNefData::minimal();
    let nef_bytes = sample_nef.to_bytes();
    let nef_path = temp_dir.path().join("test.nef");
    let output_path = temp_dir.path().join("output.asm");
    
    fs::write(&nef_path, &nef_bytes).unwrap();
    
    neo_decompile_cmd()
        .arg("disasm")
        .arg(&nef_path)
        .arg("-o")
        .arg(&output_path)
        .assert()
        .success();
    
    // Verify output file was created
    assert!(output_path.exists(), "Output file should be created");
    let content = fs::read_to_string(&output_path).unwrap();
    assert!(!content.is_empty(), "Output file should not be empty");
}

/// Test disasm command with various flags
#[test]
fn test_disasm_with_flags() {
    let temp_dir = TempDir::new().unwrap();
    let sample_nef = SampleNefData::minimal();
    let nef_bytes = sample_nef.to_bytes();
    let nef_path = temp_dir.path().join("test.nef");
    fs::write(&nef_path, &nef_bytes).unwrap();
    
    // Test with --stats flag
    neo_decompile_cmd()
        .arg("disasm")
        .arg(&nef_path)
        .arg("--stats")
        .assert()
        .success()
        .stdout(predicate::str::contains("Statistics").or(predicate::str::contains("size")));
    
    // Test with --bytes flag
    neo_decompile_cmd()
        .arg("disasm")
        .arg(&nef_path)
        .arg("--bytes")
        .assert()
        .success();
    
    // Test with --comments flag
    neo_decompile_cmd()
        .arg("disasm")
        .arg(&nef_path)
        .arg("--comments")
        .assert()
        .success();
}

/// Test cfg command
#[test]
fn test_cfg_command() {
    let temp_dir = TempDir::new().unwrap();
    let sample_nef = SampleNefData::with_control_flow();
    let nef_bytes = sample_nef.to_bytes();
    let nef_path = temp_dir.path().join("test.nef");
    fs::write(&nef_path, &nef_bytes).unwrap();
    
    neo_decompile_cmd()
        .arg("cfg")
        .arg(&nef_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("digraph").or(predicate::str::contains("CFG")));
}

/// Test cfg command with JSON output
#[test]
fn test_cfg_json_format() {
    let temp_dir = TempDir::new().unwrap();
    let sample_nef = SampleNefData::minimal();
    let nef_bytes = sample_nef.to_bytes();
    let nef_path = temp_dir.path().join("test.nef");
    let output_path = temp_dir.path().join("cfg.json");
    
    fs::write(&nef_path, &nef_bytes).unwrap();
    
    neo_decompile_cmd()
        .arg("cfg")
        .arg(&nef_path)
        .arg("-f")
        .arg("json")
        .arg("-o")
        .arg(&output_path)
        .assert()
        .success();
    
    // Verify JSON output
    assert!(output_path.exists());
    let content = fs::read_to_string(&output_path).unwrap();
    let _json: serde_json::Value = serde_json::from_str(&content)
        .expect("Output should be valid JSON");
}

/// Test decompile command
#[test]
fn test_decompile_command() {
    let temp_dir = TempDir::new().unwrap();
    let sample_nef = SampleNefData::minimal();
    let nef_bytes = sample_nef.to_bytes();
    let nef_path = temp_dir.path().join("test.nef");
    fs::write(&nef_path, &nef_bytes).unwrap();
    
    neo_decompile_cmd()
        .arg("decompile")
        .arg(&nef_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("function").or(predicate::str::len(10..)));
}

/// Test decompile command with manifest
#[test]
fn test_decompile_with_manifest() {
    let temp_dir = TempDir::new().unwrap();
    
    let sample_nef = SampleNefData::minimal();
    let nef_bytes = sample_nef.to_bytes();
    let nef_path = temp_dir.path().join("test.nef");
    fs::write(&nef_path, &nef_bytes).unwrap();
    
    let sample_manifest = SampleManifest::simple_contract();
    let manifest_json = sample_manifest.to_json();
    let manifest_path = temp_dir.path().join("test.manifest.json");
    fs::write(&manifest_path, &manifest_json).unwrap();
    
    neo_decompile_cmd()
        .arg("decompile")
        .arg(&nef_path)
        .arg("-m")
        .arg(&manifest_path)
        .assert()
        .success()
        .stdout(predicate::str::len(10..)); // Should produce substantial output
}

/// Test decompile command with different output formats
#[test]
fn test_decompile_output_formats() {
    let temp_dir = TempDir::new().unwrap();
    let sample_nef = SampleNefData::minimal();
    let nef_bytes = sample_nef.to_bytes();
    let nef_path = temp_dir.path().join("test.nef");
    fs::write(&nef_path, &nef_bytes).unwrap();
    
    // Test Python format
    neo_decompile_cmd()
        .arg("decompile")
        .arg(&nef_path)
        .arg("-f")
        .arg("python")
        .assert()
        .success();
    
    // Test C format
    neo_decompile_cmd()
        .arg("decompile")
        .arg(&nef_path)
        .arg("-f")
        .arg("c")
        .assert()
        .success();
    
    // Test JSON format
    let json_output_path = temp_dir.path().join("output.json");
    neo_decompile_cmd()
        .arg("decompile")
        .arg(&nef_path)
        .arg("-f")
        .arg("json")
        .arg("-o")
        .arg(&json_output_path)
        .assert()
        .success();
    
    // Verify JSON is valid
    assert!(json_output_path.exists());
    let content = fs::read_to_string(&json_output_path).unwrap();
    let _json: serde_json::Value = serde_json::from_str(&content)
        .expect("JSON output should be valid");
}

/// Test decompile with optimization levels
#[test]
fn test_decompile_optimization_levels() {
    let temp_dir = TempDir::new().unwrap();
    let sample_nef = SampleNefData::minimal();
    let nef_bytes = sample_nef.to_bytes();
    let nef_path = temp_dir.path().join("test.nef");
    fs::write(&nef_path, &nef_bytes).unwrap();
    
    // Test different optimization levels
    for opt_level in 0..=3 {
        neo_decompile_cmd()
            .arg("decompile")
            .arg(&nef_path)
            .arg("--optimization")
            .arg(opt_level.to_string())
            .assert()
            .success();
    }
}

/// Test analyze command
#[test]
fn test_analyze_command() {
    let temp_dir = TempDir::new().unwrap();
    let sample_nef = SampleNefData::minimal();
    let nef_bytes = sample_nef.to_bytes();
    let nef_path = temp_dir.path().join("test.nef");
    fs::write(&nef_path, &nef_bytes).unwrap();
    
    neo_decompile_cmd()
        .arg("analyze")
        .arg(&nef_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("{").or(predicate::str::contains("Analysis")));
}

/// Test analyze command with different analysis types
#[test]
fn test_analyze_types() {
    let temp_dir = TempDir::new().unwrap();
    let sample_nef = SampleNefData::minimal();
    let nef_bytes = sample_nef.to_bytes();
    let nef_path = temp_dir.path().join("test.nef");
    fs::write(&nef_path, &nef_bytes).unwrap();
    
    // Test security analysis
    neo_decompile_cmd()
        .arg("analyze")
        .arg(&nef_path)
        .arg("--security")
        .assert()
        .success();
    
    // Test NEP compliance
    neo_decompile_cmd()
        .arg("analyze")
        .arg(&nef_path)
        .arg("--nep-compliance")
        .assert()
        .success();
    
    // Test performance analysis
    neo_decompile_cmd()
        .arg("analyze")
        .arg(&nef_path)
        .arg("--performance")
        .assert()
        .success();
    
    // Test all analysis types
    neo_decompile_cmd()
        .arg("analyze")
        .arg(&nef_path)
        .arg("--all")
        .assert()
        .success();
}

/// Test info command
#[test]
fn test_info_command() {
    let temp_dir = TempDir::new().unwrap();
    let sample_nef = SampleNefData::minimal();
    let nef_bytes = sample_nef.to_bytes();
    let nef_path = temp_dir.path().join("test.nef");
    fs::write(&nef_path, &nef_bytes).unwrap();
    
    neo_decompile_cmd()
        .arg("info")
        .arg(&nef_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Contract Information").or(predicate::str::contains("Size")));
}

/// Test info command with manifest
#[test]
fn test_info_with_manifest() {
    let temp_dir = TempDir::new().unwrap();
    
    let sample_nef = SampleNefData::minimal();
    let nef_bytes = sample_nef.to_bytes();
    let nef_path = temp_dir.path().join("test.nef");
    fs::write(&nef_path, &nef_bytes).unwrap();
    
    let sample_manifest = SampleManifest::nep17_token();
    let manifest_json = sample_manifest.to_json();
    let manifest_path = temp_dir.path().join("test.manifest.json");
    fs::write(&manifest_path, &manifest_json).unwrap();
    
    neo_decompile_cmd()
        .arg("info")
        .arg(&nef_path)
        .arg("-m")
        .arg(&manifest_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("TestToken").or(predicate::str::contains("NEP-17")));
}

/// Test info command with different output formats
#[test]
fn test_info_output_formats() {
    let temp_dir = TempDir::new().unwrap();
    let sample_nef = SampleNefData::minimal();
    let nef_bytes = sample_nef.to_bytes();
    let nef_path = temp_dir.path().join("test.nef");
    fs::write(&nef_path, &nef_bytes).unwrap();
    
    // Test JSON format
    let json_output_path = temp_dir.path().join("info.json");
    neo_decompile_cmd()
        .arg("info")
        .arg(&nef_path)
        .arg("-f")
        .arg("json")
        .arg("--redirect")
        .arg(&json_output_path)
        .assert()
        .success();
    
    // Note: The actual CLI might output to stdout, so we test that it succeeds
    neo_decompile_cmd()
        .arg("info")
        .arg(&nef_path)
        .arg("-f")
        .arg("json")
        .assert()
        .success()
        .stdout(predicate::str::starts_with("{"));
}

/// Test config commands
#[test]
fn test_config_commands() {
    let temp_dir = TempDir::new().unwrap();
    
    // Test config show
    neo_decompile_cmd()
        .arg("config")
        .arg("show")
        .assert()
        .success();
    
    // Test config generate
    let config_path = temp_dir.path().join("generated.toml");
    neo_decompile_cmd()
        .arg("config")
        .arg("generate")
        .arg("-o")
        .arg(&config_path)
        .assert()
        .success();
    
    assert!(config_path.exists(), "Config file should be generated");
    
    // Test config validate
    neo_decompile_cmd()
        .arg("config")
        .arg("validate")
        .arg(&config_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("valid"));
}

/// Test init command
#[test]
fn test_init_command() {
    let temp_dir = TempDir::new().unwrap();
    let project_dir = temp_dir.path().join("test_project");
    
    neo_decompile_cmd()
        .arg("init")
        .arg(&project_dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("Initialized"));
    
    // Verify files were created
    assert!(project_dir.join("decompiler.toml").exists());
    assert!(project_dir.join("README_DECOMPILER.md").exists());
}

/// Test init command with force flag
#[test]
fn test_init_force_overwrite() {
    let temp_dir = TempDir::new().unwrap();
    let project_dir = temp_dir.path().join("force_test");
    
    // Create directory and file first
    fs::create_dir_all(&project_dir).unwrap();
    fs::write(project_dir.join("decompiler.toml"), "existing content").unwrap();
    
    // Should fail without force
    neo_decompile_cmd()
        .arg("init")
        .arg(&project_dir)
        .assert()
        .failure()
        .stderr(predicate::str::contains("exist"));
    
    // Should succeed with force
    neo_decompile_cmd()
        .arg("init")
        .arg(&project_dir)
        .arg("--force")
        .assert()
        .success();
}

/// Test global verbose flag
#[test]
fn test_verbose_flag() {
    let temp_dir = TempDir::new().unwrap();
    let sample_nef = SampleNefData::minimal();
    let nef_bytes = sample_nef.to_bytes();
    let nef_path = temp_dir.path().join("test.nef");
    fs::write(&nef_path, &nef_bytes).unwrap();
    
    // Test different verbosity levels
    neo_decompile_cmd()
        .arg("-v")
        .arg("disasm")
        .arg(&nef_path)
        .assert()
        .success();
    
    neo_decompile_cmd()
        .arg("-vv")
        .arg("disasm")
        .arg(&nef_path)
        .assert()
        .success();
    
    neo_decompile_cmd()
        .arg("-vvv")
        .arg("disasm")
        .arg(&nef_path)
        .assert()
        .success();
}

/// Test quiet flag
#[test]
fn test_quiet_flag() {
    let temp_dir = TempDir::new().unwrap();
    let sample_nef = SampleNefData::minimal();
    let nef_bytes = sample_nef.to_bytes();
    let nef_path = temp_dir.path().join("test.nef");
    fs::write(&nef_path, &nef_bytes).unwrap();
    
    neo_decompile_cmd()
        .arg("--quiet")
        .arg("disasm")
        .arg(&nef_path)
        .assert()
        .success();
}

/// Test error handling for invalid files
#[test]
fn test_invalid_file_handling() {
    // Test with non-existent file
    neo_decompile_cmd()
        .arg("disasm")
        .arg("nonexistent.nef")
        .assert()
        .failure()
        .stderr(predicate::str::contains("No such file").or(predicate::str::contains("not found")));
    
    // Test with invalid NEF file
    let temp_dir = TempDir::new().unwrap();
    let invalid_nef_path = temp_dir.path().join("invalid.nef");
    fs::write(&invalid_nef_path, b"not a nef file").unwrap();
    
    neo_decompile_cmd()
        .arg("disasm")
        .arg(&invalid_nef_path)
        .assert()
        .failure()
        .stderr(predicate::str::contains("error").or(predicate::str::contains("invalid")));
}

/// Test multi-format output with decompile
#[test]
fn test_multi_format_output() {
    let temp_dir = TempDir::new().unwrap();
    let sample_nef = SampleNefData::minimal();
    let nef_bytes = sample_nef.to_bytes();
    let nef_path = temp_dir.path().join("test.nef");
    fs::write(&nef_path, &nef_bytes).unwrap();
    
    let output_path = temp_dir.path().join("output.pseudo");
    
    neo_decompile_cmd()
        .arg("decompile")
        .arg(&nef_path)
        .arg("-o")
        .arg(&output_path)
        .arg("--multi-format")
        .assert()
        .success();
    
    // With multi-format, additional files should be created
    // (This depends on the implementation creating .py, .c, .html files)
    assert!(output_path.exists(), "Primary output should exist");
}

/// Test performance metrics flag
#[test]
fn test_performance_metrics() {
    let temp_dir = TempDir::new().unwrap();
    let sample_nef = SampleNefData::minimal();
    let nef_bytes = sample_nef.to_bytes();
    let nef_path = temp_dir.path().join("test.nef");
    fs::write(&nef_path, &nef_bytes).unwrap();
    
    neo_decompile_cmd()
        .arg("decompile")
        .arg(&nef_path)
        .arg("--metrics")
        .assert()
        .success()
        .stderr(predicate::str::contains("Performance Metrics").or(predicate::str::contains("time")));
}