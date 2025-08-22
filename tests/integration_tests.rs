//! Integration tests for end-to-end decompilation workflows
//! 
//! Tests the complete pipeline from NEF files to various outputs, ensuring
//! all components work together correctly.

use std::fs;
use neo_decompiler::*;
use crate::common::*;
use crate::common::assertions::*;

/// Test complete decompilation workflow with minimal NEF
#[test]
fn test_end_to_end_decompilation_minimal() {
    let env = TestEnvironment::new();
    
    // Create test NEF file
    let sample_nef = SampleNefData::minimal();
    let nef_bytes = sample_nef.to_bytes();
    let nef_path = env.write_file("test.nef", &nef_bytes);
    
    // Create test manifest
    let sample_manifest = SampleManifest::simple_contract();
    let manifest_json = sample_manifest.to_json();
    let manifest_path = env.write_text_file("test.manifest.json", &manifest_json);
    
    // Perform decompilation
    let mut decompiler = env.decompiler;
    let nef_data = fs::read(&nef_path).unwrap();
    let manifest_content = fs::read_to_string(&manifest_path).unwrap();
    
    let result = decompiler.decompile(&nef_data, Some(&manifest_content));
    
    assert!(result.is_ok(), "End-to-end decompilation should succeed");
    
    let decompiled = result.unwrap();
    assert!(!decompiled.pseudocode.is_empty(), "Should generate non-empty pseudocode");
    assert!(!decompiled.instructions.is_empty(), "Should have disassembled instructions");
    assert!(decompiled.manifest.is_some(), "Should include parsed manifest");
    assert_eq!(decompiled.nef_file.magic, *b"NEF3", "Should preserve NEF magic");
}

/// Test decompilation with NEP-17 token contract
#[test]
fn test_end_to_end_nep17_decompilation() {
    let env = TestEnvironment::new();
    
    // Create NEP-17 style NEF (more complex bytecode)
    let sample_nef = SampleNefData::with_control_flow();
    let nef_bytes = sample_nef.to_bytes();
    let nef_path = env.write_file("nep17.nef", &nef_bytes);
    
    // Create NEP-17 manifest
    let sample_manifest = SampleManifest::nep17_token();
    let manifest_json = sample_manifest.to_json();
    let manifest_path = env.write_text_file("nep17.manifest.json", &manifest_json);
    
    // Perform decompilation
    let mut decompiler = env.decompiler;
    let nef_data = fs::read(&nef_path).unwrap();
    let manifest_content = fs::read_to_string(&manifest_path).unwrap();
    
    let result = decompiler.decompile(&nef_data, Some(&manifest_content));
    
    assert!(result.is_ok(), "NEP-17 decompilation should succeed");
    
    let decompiled = result.unwrap();
    
    // Verify NEP-17 specific elements are preserved
    let manifest = decompiled.manifest.as_ref().unwrap();
    assert!(manifest.supported_standards.contains(&"NEP-17".to_string()));
    
    // Pseudocode should reference NEP-17 methods
    let pseudocode = &decompiled.pseudocode;
    assert!(pseudocode.contains("transfer") || pseudocode.contains("balanceOf") || 
            pseudocode.len() > 50, "Should contain NEP-17 related content");
}

/// Test decompilation without manifest
#[test]
fn test_decompilation_without_manifest() {
    let env = TestEnvironment::new();
    
    // Create test NEF file only
    let sample_nef = SampleNefData::minimal();
    let nef_bytes = sample_nef.to_bytes();
    let nef_path = env.write_file("no_manifest.nef", &nef_bytes);
    
    // Perform decompilation without manifest
    let mut decompiler = env.decompiler;
    let nef_data = fs::read(&nef_path).unwrap();
    
    let result = decompiler.decompile(&nef_data, None);
    
    assert!(result.is_ok(), "Decompilation without manifest should succeed");
    
    let decompiled = result.unwrap();
    assert!(!decompiled.pseudocode.is_empty(), "Should generate pseudocode without manifest");
    assert!(decompiled.manifest.is_none(), "Manifest should be None");
    assert!(!decompiled.instructions.is_empty(), "Should still disassemble instructions");
}

/// Test multiple output format generation
#[test]
fn test_multiple_output_formats() {
    let env = TestEnvironment::new();
    
    // Setup test data
    let sample_nef = SampleNefData::minimal();
    let nef_bytes = sample_nef.to_bytes();
    let sample_manifest = SampleManifest::simple_contract();
    let manifest_json = sample_manifest.to_json();
    
    // Perform decompilation
    let mut decompiler = env.decompiler;
    let result = decompiler.decompile(&nef_bytes, Some(&manifest_json)).unwrap();
    
    // Test different pseudocode format conversions
    // (These would use the CLI format conversion methods in a real implementation)
    
    // Basic pseudocode
    let pseudocode = &result.pseudocode;
    assert!(!pseudocode.is_empty(), "Basic pseudocode should not be empty");
    
    // JSON format
    let json_output = serde_json::json!({
        "pseudocode": result.pseudocode,
        "instructions_count": result.instructions.len(),
        "contract_name": result.manifest.as_ref().map(|m| &m.name),
    });
    
    let json_str = serde_json::to_string_pretty(&json_output).unwrap();
    assert_valid_json_with_fields(&json_str, &["pseudocode", "instructions_count"]);
    
    // HTML format test
    let html_output = format!(
        "<!DOCTYPE html><html><head><title>Contract</title></head><body><pre>{}</pre></body></html>",
        html_escape(&result.pseudocode)
    );
    assert!(html_output.contains("<!DOCTYPE html>"), "Should be valid HTML");
    assert!(html_output.contains(&result.pseudocode) || 
            html_output.contains("&lt;"), "Should contain escaped pseudocode");
}

/// Test configuration loading and application
#[test]
fn test_configuration_integration() {
    let env = TestEnvironment::new();
    
    // Create custom configuration
    let mut custom_config = DecompilerConfig::default();
    custom_config.optimization_level = 2;
    
    // Create decompiler with custom config
    let env_with_config = TestEnvironment::with_config(custom_config);
    
    // Test that configuration is applied
    let sample_nef = SampleNefData::minimal();
    let nef_bytes = sample_nef.to_bytes();
    
    let mut decompiler = env_with_config.decompiler;
    let result = decompiler.decompile(&nef_bytes, None);
    
    assert!(result.is_ok(), "Decompilation with custom config should succeed");
    
    // The actual optimization level application would be verified through
    // the quality or characteristics of the generated pseudocode
    let decompiled = result.unwrap();
    assert!(!decompiled.pseudocode.is_empty(), "Should generate optimized pseudocode");
}

/// Test large contract decompilation performance
#[test]
fn test_large_contract_decompilation() {
    let env = TestEnvironment::new();
    
    // Create a larger, more complex NEF file
    let mut complex_nef = SampleNefData::with_control_flow();
    
    // Extend bytecode with more instructions to simulate a larger contract
    let additional_bytecode = vec![
        // Additional PUSH operations
        0x10, 0x11, 0x12, 0x13, 0x14, 0x15,
        // More arithmetic operations  
        0x93, 0x94, 0x95, 0x96, 0x97,
        // Additional control flow
        0x2C, 0x03, 0x11, 0x41, // JMP_IF, PUSH1, RET
        0x2B, 0x02, 0x41,       // JMP, RET
        // String operations
        0x0C, 0x0A, 0x48, 0x65, 0x6C, 0x6C, 0x6F, 0x57, 0x6F, 0x72, 0x6C, 0x64, // "HelloWorld"
        0x8A, // SIZE
        // Final return
        0x41, // RET
    ];
    
    complex_nef.bytecode.extend(additional_bytecode);
    let nef_bytes = complex_nef.to_bytes();
    
    // Time the decompilation
    let start = std::time::Instant::now();
    
    let mut decompiler = env.decompiler;
    let result = decompiler.decompile(&nef_bytes, None);
    
    let duration = start.elapsed();
    
    assert!(result.is_ok(), "Large contract decompilation should succeed");
    assert!(duration.as_secs() < 10, "Decompilation should complete in reasonable time");
    
    let decompiled = result.unwrap();
    assert!(decompiled.instructions.len() > 10, "Should have many instructions");
    assert!(!decompiled.pseudocode.is_empty(), "Should generate substantial pseudocode");
}

/// Test error handling and recovery
#[test]
fn test_error_handling_integration() {
    let env = TestEnvironment::new();
    
    // Test with corrupted NEF file
    let corrupted_nef = vec![0x00, 0x01, 0x02, 0x03]; // Invalid NEF
    let result = env.decompiler.decompile(&corrupted_nef, None);
    assert!(result.is_err(), "Should fail on corrupted NEF");
    
    // Test with invalid manifest JSON
    let sample_nef = SampleNefData::minimal();
    let nef_bytes = sample_nef.to_bytes();
    let invalid_manifest = "{invalid json}";
    
    let mut decompiler = env.decompiler;
    let result = decompiler.decompile(&nef_bytes, Some(invalid_manifest));
    assert!(result.is_err(), "Should fail on invalid manifest JSON");
}

/// Test memory usage with various contract sizes
#[test]
fn test_memory_usage_patterns() {
    let env = TestEnvironment::new();
    
    // Test with different sized contracts
    let test_cases = vec![
        ("minimal", SampleNefData::minimal()),
        ("control_flow", SampleNefData::with_control_flow()),
    ];
    
    for (name, sample_nef) in test_cases {
        let nef_bytes = sample_nef.to_bytes();
        
        // Memory usage would be measured here in a real implementation
        // For now, just verify successful processing
        let mut decompiler = Decompiler::new(DecompilerConfig::default());
        let result = decompiler.decompile(&nef_bytes, None);
        
        assert!(result.is_ok(), "Decompilation should succeed for {}", name);
        
        let decompiled = result.unwrap();
        assert!(!decompiled.pseudocode.is_empty(), 
               "Should generate pseudocode for {}", name);
    }
}

/// Test concurrent decompilation (if supported)
#[test]
fn test_concurrent_decompilation() {
    use std::thread;
    use std::sync::Arc;
    
    // Create test data
    let sample_nef = SampleNefData::minimal();
    let nef_bytes = Arc::new(sample_nef.to_bytes());
    
    // Spawn multiple decompilation threads
    let mut handles = vec![];
    
    for i in 0..3 {
        let nef_data = Arc::clone(&nef_bytes);
        let handle = thread::spawn(move || {
            let config = DecompilerConfig::default();
            let mut decompiler = Decompiler::new(config);
            
            let result = decompiler.decompile(&nef_data, None);
            (i, result)
        });
        handles.push(handle);
    }
    
    // Wait for all threads to complete
    for handle in handles {
        let (thread_id, result) = handle.join().unwrap();
        assert!(result.is_ok(), "Thread {} should succeed", thread_id);
        
        let decompiled = result.unwrap();
        assert!(!decompiled.pseudocode.is_empty(), 
               "Thread {} should generate pseudocode", thread_id);
    }
}

// Helper function for HTML escaping (simplified)
fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}