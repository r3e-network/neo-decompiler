//! Basic usage examples for the Neo N3 decompiler library
//! 
//! This file demonstrates common usage patterns and serves as executable
//! documentation for the decompiler API.

use neo_decompiler::{Decompiler, DecompilerConfig};
use std::fs;

/// Example: Basic decompilation workflow
fn basic_decompilation_example() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Basic Decompilation Example ===");
    
    // Create default configuration
    let config = DecompilerConfig::default();
    let mut decompiler = Decompiler::new(config);
    
    // Read NEF file (you would use actual file here)
    let sample_nef = create_sample_nef();
    
    // Perform decompilation
    let result = decompiler.decompile(&sample_nef, None)?;
    
    println!("✅ Decompilation successful!");
    println!("📄 Generated pseudocode ({} chars):", result.pseudocode.len());
    println!("{}", result.pseudocode);
    println!("🔢 Instructions processed: {}", result.instructions.len());
    
    Ok(())
}

/// Example: Decompilation with manifest
fn decompilation_with_manifest_example() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== Decompilation with Manifest Example ===");
    
    let config = DecompilerConfig::default();
    let mut decompiler = Decompiler::new(config);
    
    let sample_nef = create_sample_nef();
    let sample_manifest = create_sample_manifest();
    
    let result = decompiler.decompile(&sample_nef, Some(&sample_manifest))?;
    
    println!("✅ Decompilation with manifest successful!");
    
    if let Some(manifest) = &result.manifest {
        println!("📋 Contract: {}", manifest.name);
        println!("🔧 Methods: {}", manifest.abi.methods.len());
        println!("📡 Events: {}", manifest.abi.events.len());
        
        for method in &manifest.abi.methods {
            println!("  - {} ({} params)", method.name, method.parameters.len());
        }
    }
    
    println!("🔍 Pseudocode preview:");
    println!("{}", &result.pseudocode[..result.pseudocode.len().min(200)]);
    if result.pseudocode.len() > 200 {
        println!("... (truncated)");
    }
    
    Ok(())
}

/// Example: Custom configuration
fn custom_configuration_example() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== Custom Configuration Example ===");
    
    // Create custom configuration
    let mut config = DecompilerConfig::default();
    config.optimization_level = 2; // Higher optimization
    // Add other custom settings as available in the actual implementation
    
    let mut decompiler = Decompiler::new(config);
    
    let sample_nef = create_sample_nef();
    let result = decompiler.decompile(&sample_nef, None)?;
    
    println!("✅ Decompilation with custom config successful!");
    println!("⚡ Optimization level: 2");
    println!("📊 Generated {} instructions", result.instructions.len());
    
    Ok(())
}

/// Example: Error handling and validation
fn error_handling_example() {
    println!("\n=== Error Handling Example ===");
    
    let config = DecompilerConfig::default();
    let mut decompiler = Decompiler::new(config);
    
    // Try to decompile invalid data
    let invalid_nef = vec![0x00, 0x01, 0x02, 0x03]; // Invalid NEF
    
    match decompiler.decompile(&invalid_nef, None) {
        Ok(_) => println!("🤔 Unexpected success with invalid data"),
        Err(e) => {
            println!("✅ Correctly handled invalid NEF file");
            println!("❌ Error: {}", e);
            
            // Print error chain
            let mut source = e.source();
            while let Some(err) = source {
                println!("  Caused by: {}", err);
                source = err.source();
            }
        }
    }
    
    // Try with invalid manifest
    let sample_nef = create_sample_nef();
    let invalid_manifest = "{invalid json}";
    
    match decompiler.decompile(&sample_nef, Some(invalid_manifest)) {
        Ok(_) => println!("🤔 Unexpected success with invalid manifest"),
        Err(e) => {
            println!("✅ Correctly handled invalid manifest");
            println!("❌ Error: {}", e);
        }
    }
}

/// Example: Working with individual components
fn component_usage_example() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== Component Usage Example ===");
    
    let config = DecompilerConfig::default();
    
    // 1. Parse NEF file
    let nef_parser = neo_decompiler::NEFParser::new();
    let sample_nef_bytes = create_sample_nef();
    let nef_file = nef_parser.parse(&sample_nef_bytes)?;
    
    println!("✅ NEF parsing successful");
    println!("🏷️ Magic: {:?}", std::str::from_utf8(&nef_file.magic).unwrap_or("N/A"));
    println!("📦 Bytecode size: {} bytes", nef_file.bytecode.len());
    
    // 2. Disassemble bytecode
    let disassembler = neo_decompiler::Disassembler::new(&config);
    let instructions = disassembler.disassemble(&nef_file.bytecode)?;
    
    println!("✅ Disassembly successful");
    println!("📝 Instructions: {}", instructions.len());
    
    for (i, instruction) in instructions.iter().enumerate().take(5) {
        println!("  {}: {:?}", i, instruction.opcode);
    }
    
    if instructions.len() > 5 {
        println!("  ... ({} more)", instructions.len() - 5);
    }
    
    // 3. Lift to IR
    let lifter = neo_decompiler::IRLifter::new(&config);
    let ir_function = lifter.lift_to_ir(&instructions)?;
    
    println!("✅ IR lifting successful");
    println!("🧱 Basic blocks: {}", ir_function.basic_blocks.len());
    println!("🔢 Variables: {}", ir_function.variables.len());
    
    // 4. Generate pseudocode
    let generator = neo_decompiler::PseudocodeGenerator::new(&config);
    let pseudocode = generator.generate(&ir_function)?;
    
    println!("✅ Pseudocode generation successful");
    println!("📄 Output length: {} characters", pseudocode.len());
    
    Ok(())
}

/// Example: Performance measurement
fn performance_measurement_example() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== Performance Measurement Example ===");
    
    use std::time::Instant;
    
    let config = DecompilerConfig::default();
    let sample_nef = create_sample_nef();
    
    // Measure decompilation time
    let start = Instant::now();
    let mut decompiler = Decompiler::new(config);
    let result = decompiler.decompile(&sample_nef, None)?;
    let duration = start.elapsed();
    
    println!("⏱️ Decompilation Performance:");
    println!("  Time: {:?}", duration);
    println!("  Input size: {} bytes", sample_nef.len());
    println!("  Instructions: {}", result.instructions.len());
    println!("  Output size: {} chars", result.pseudocode.len());
    println!("  Throughput: {:.2} KB/s", 
             (sample_nef.len() as f64) / (duration.as_secs_f64() * 1024.0));
    
    Ok(())
}

/// Helper function to create a sample NEF file for testing
fn create_sample_nef() -> Vec<u8> {
    let mut nef_data = Vec::new();
    
    // NEF magic
    nef_data.extend_from_slice(b"NEF3");
    
    // Compiler (64 bytes)
    let mut compiler = [0u8; 64];
    let compiler_str = b"example-compiler-v1.0";
    compiler[..compiler_str.len()].copy_from_slice(compiler_str);
    nef_data.extend_from_slice(&compiler);
    
    // Source URL
    let source_url = b"https://example.com/contract";
    nef_data.extend_from_slice(&(source_url.len() as u16).to_le_bytes());
    nef_data.extend_from_slice(source_url);
    
    // Reserved byte
    nef_data.push(0);
    
    // Tokens (empty)
    nef_data.extend_from_slice(&[0u8; 2]); // Length
    
    // Reserved bytes
    nef_data.extend_from_slice(&[0u8; 2]);
    
    // Bytecode
    let bytecode = vec![
        0x11, 0x12, 0x93, // PUSH1, PUSH2, ADD
        0x13, 0x9F,       // PUSH3, GT
        0x2C, 0x05,       // JMP_IF 5
        0x0C, 0x04, 0x54, 0x72, 0x75, 0x65, // PUSHDATA1 "True"
        0x41,             // RET
        0x0C, 0x05, 0x46, 0x61, 0x6C, 0x73, 0x65, // PUSHDATA1 "False"
        0x41,             // RET
    ];
    
    nef_data.extend_from_slice(&(bytecode.len() as u32).to_le_bytes());
    nef_data.extend_from_slice(&bytecode);
    
    // Checksum
    nef_data.extend_from_slice(&0x12345678u32.to_le_bytes());
    
    nef_data
}

/// Helper function to create a sample manifest
fn create_sample_manifest() -> String {
    r#"{
        "name": "SampleContract",
        "supportedstandards": [],
        "abi": {
            "methods": [
                {
                    "name": "main",
                    "parameters": [],
                    "returntype": "Boolean",
                    "offset": 0,
                    "safe": true
                },
                {
                    "name": "getValue",
                    "parameters": [
                        {
                            "name": "key",
                            "type": "String"
                        }
                    ],
                    "returntype": "Any",
                    "offset": 50,
                    "safe": true
                }
            ],
            "events": [
                {
                    "name": "ValueChanged",
                    "parameters": [
                        {
                            "name": "key",
                            "type": "String"
                        },
                        {
                            "name": "value",
                            "type": "Any"
                        }
                    ]
                }
            ]
        },
        "permissions": [
            {
                "contract": "*",
                "methods": "*"
            }
        ],
        "trusts": [],
        "extra": {
            "Author": "Neo Example",
            "Description": "Sample contract for testing"
        }
    }"#.to_string()
}

/// Main function running all examples
fn main() {
    println!("🚀 Neo N3 Decompiler - Basic Usage Examples");
    println!("===========================================");
    
    // Run examples and handle errors
    let examples = [
        ("Basic Decompilation", basic_decompilation_example as fn() -> Result<(), Box<dyn std::error::Error>>),
        ("With Manifest", decompilation_with_manifest_example),
        ("Custom Configuration", custom_configuration_example),
        ("Component Usage", component_usage_example),
        ("Performance Measurement", performance_measurement_example),
    ];
    
    for (name, example_fn) in examples.iter() {
        match example_fn() {
            Ok(()) => println!("✅ {} completed successfully", name),
            Err(e) => println!("❌ {} failed: {}", name, e),
        }
    }
    
    // Error handling example (doesn't return Result)
    error_handling_example();
    
    println!("\n🎉 All examples completed!");
    println!("\nFor more information:");
    println!("- Run with: cargo run --example basic_usage");
    println!("- Documentation: cargo doc --open");
    println!("- Tests: cargo test");
}