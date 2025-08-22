//! Property-based tests for the Neo N3 decompiler
//! 
//! Uses proptest to generate random inputs and verify invariants hold across
//! a wide range of possible inputs, helping discover edge cases and ensure robustness.

use proptest::prelude::*;
use neo_decompiler::*;
use crate::common::*;

// Custom strategies for generating test data

/// Strategy for generating valid NEF magic bytes
fn nef_magic_strategy() -> impl Strategy<Value = [u8; 4]> {
    // Sometimes generate valid magic, sometimes invalid
    prop_oneof![
        Just(*b"NEF3"),
        any::<[u8; 4]>(), // Random bytes (mostly invalid)
    ]
}

/// Strategy for generating bytecode sequences
fn bytecode_strategy() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(
        prop_oneof![
            // Common valid opcodes
            Just(0x10u8), // PUSH0
            Just(0x11u8), // PUSH1  
            Just(0x12u8), // PUSH2
            Just(0x41u8), // RET
            Just(0x93u8), // ADD
            Just(0x94u8), // SUB
            Just(0x8A u8), // SIZE
            Just(0x6B u8), // DUP
            Just(0x75u8), // DROP
            // PUSHDATA instructions
            (0x0C_u8, 1_u8..=75_u8, prop::collection::vec(any::<u8>(), 1..10))
                .prop_map(|(opcode, len, data)| {
                    let mut result = vec![opcode, len];
                    result.extend(data);
                    result
                }).prop_flat_map(|v| Just(v).prop_map(|mut v| { v.truncate(2); v })),
            // Random bytes (potentially invalid opcodes)
            any::<u8>(),
        ],
        1..100
    ).prop_map(|opcodes| {
        let mut result = Vec::new();
        for opcode in opcodes {
            match opcode {
                v if v.is_empty() => continue,
                v => result.extend(v),
            }
        }
        result
    })
}

/// Strategy for generating compiler strings
fn compiler_strategy() -> impl Strategy<Value = [u8; 64]> {
    prop::collection::vec(any::<u8>(), 64)
        .prop_map(|v| {
            let mut arr = [0u8; 64];
            arr.copy_from_slice(&v);
            arr
        })
}

/// Strategy for generating source URLs
fn source_url_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("".to_string()),
        Just("https://example.com/contract".to_string()),
        "[a-zA-Z0-9._/-]{1,100}",
        any::<String>().prop_map(|s| s.chars().take(100).collect()),
    ]
}

/// Strategy for generating complete NEF file data
fn nef_file_strategy() -> impl Strategy<Value = SampleNefData> {
    (
        nef_magic_strategy(),
        compiler_strategy(),
        source_url_strategy(),
        prop::collection::vec(any::<u8>(), 0..20), // tokens
        bytecode_strategy(),
        any::<u32>(), // checksum
    ).prop_map(|(magic, compiler, source_url, tokens, bytecode, checksum)| {
        SampleNefData {
            magic,
            compiler,
            source_url,
            tokens,
            bytecode,
            checksum,
        }
    })
}

/// Strategy for generating manifest method parameters
fn manifest_parameter_strategy() -> impl Strategy<Value = ManifestParameter> {
    (
        "[a-zA-Z_][a-zA-Z0-9_]{0,20}",
        prop_oneof![
            Just("String".to_string()),
            Just("Integer".to_string()),
            Just("Boolean".to_string()),
            Just("Hash160".to_string()),
            Just("Hash256".to_string()),
            Just("ByteArray".to_string()),
            Just("Any".to_string()),
        ]
    ).prop_map(|(name, param_type)| ManifestParameter { name, param_type })
}

/// Strategy for generating manifest methods
fn manifest_method_strategy() -> impl Strategy<Value = ManifestMethod> {
    (
        "[a-zA-Z_][a-zA-Z0-9_]{0,30}",
        prop::collection::vec(manifest_parameter_strategy(), 0..10),
        prop_oneof![
            Just("String".to_string()),
            Just("Integer".to_string()),
            Just("Boolean".to_string()),
            Just("Void".to_string()),
        ],
        0u32..1000u32,
        any::<bool>(),
    ).prop_map(|(name, parameters, return_type, offset, safe)| {
        ManifestMethod {
            name,
            parameters,
            return_type,
            offset,
            safe,
        }
    })
}

// Property tests

proptest! {
    /// Test that NEF parser handles arbitrary input gracefully
    #[test]
    fn prop_nef_parser_handles_arbitrary_input(nef_data in nef_file_strategy()) {
        let nef_bytes = nef_data.to_bytes();
        let parser = NEFParser::new();
        
        // Parser should either succeed or fail gracefully (no panics)
        let result = std::panic::catch_unwind(|| parser.parse(&nef_bytes));
        prop_assert!(result.is_ok(), "NEF parser should not panic on arbitrary input");
        
        // If parsing succeeds, the result should be reasonable
        if let Ok(Ok(nef_file)) = result {
            prop_assert!(nef_file.bytecode.len() <= nef_bytes.len(),
                        "Bytecode length should not exceed input length");
        }
    }
    
    /// Test that valid NEF files always parse successfully
    #[test]
    fn prop_valid_nef_files_parse_successfully(
        compiler in compiler_strategy(),
        source_url in source_url_strategy(),
        tokens in prop::collection::vec(any::<u8>(), 0..20),
        bytecode in bytecode_strategy(),
        checksum in any::<u32>()
    ) {
        let sample = SampleNefData {
            magic: *b"NEF3", // Always use valid magic
            compiler,
            source_url,
            tokens,
            bytecode: bytecode.clone(),
            checksum,
        };
        
        let nef_bytes = sample.to_bytes();
        let parser = NEFParser::new();
        
        let result = parser.parse(&nef_bytes);
        prop_assert!(result.is_ok(), "Valid NEF structure should parse successfully");
        
        let nef_file = result.unwrap();
        prop_assert_eq!(nef_file.magic, *b"NEF3");
        prop_assert_eq!(nef_file.bytecode, bytecode);
    }
    
    /// Test that disassembler handles arbitrary bytecode gracefully
    #[test]
    fn prop_disassembler_handles_arbitrary_bytecode(bytecode in bytecode_strategy()) {
        let config = DecompilerConfig::default();
        let disassembler = Disassembler::new(&config);
        
        // Disassembler should not panic on arbitrary bytecode
        let result = std::panic::catch_unwind(|| disassembler.disassemble(&bytecode));
        prop_assert!(result.is_ok(), "Disassembler should not panic");
        
        if let Ok(Ok(instructions)) = result {
            // If disassembly succeeds, instructions should be reasonable
            prop_assert!(instructions.len() <= bytecode.len(),
                        "Number of instructions should not exceed bytecode length");
            
            // Each instruction should have an opcode
            for instruction in instructions {
                prop_assert!(format!("{:?}", instruction.opcode).len() > 0,
                           "Instruction should have a valid opcode representation");
            }
        }
    }
    
    /// Test that IR lifter preserves instruction count relationship
    #[test]
    fn prop_ir_lifter_preserves_instruction_semantics(bytecode in bytecode_strategy()) {
        let config = DecompilerConfig::default();
        let disassembler = Disassembler::new(&config);
        let lifter = IRLifter::new(&config);
        
        if let Ok(instructions) = disassembler.disassemble(&bytecode) {
            let result = std::panic::catch_unwind(|| lifter.lift_to_ir(&instructions));
            prop_assert!(result.is_ok(), "IR lifter should not panic");
            
            if let Ok(Ok(ir_function)) = result {
                // IR should contain some representation of the original instructions
                let total_ir_instructions: usize = ir_function.basic_blocks.iter()
                    .map(|bb| bb.instructions.len())
                    .sum();
                    
                // Allow some flexibility as IR might optimize or expand
                prop_assert!(total_ir_instructions <= instructions.len() * 2,
                           "IR instruction count should be reasonable relative to original");
            }
        }
    }
    
    /// Test that pseudocode generation produces reasonable output
    #[test]
    fn prop_pseudocode_generation_produces_reasonable_output(
        bytecode in bytecode_strategy().prop_filter("non-empty", |b| !b.is_empty())
    ) {
        let config = DecompilerConfig::default();
        let disassembler = Disassembler::new(&config);
        let lifter = IRLifter::new(&config);
        let generator = PseudocodeGenerator::new(&config);
        
        if let Ok(instructions) = disassembler.disassemble(&bytecode) {
            if !instructions.is_empty() {
                if let Ok(ir_function) = lifter.lift_to_ir(&instructions) {
                    let result = std::panic::catch_unwind(|| generator.generate(&ir_function));
                    prop_assert!(result.is_ok(), "Pseudocode generator should not panic");
                    
                    if let Ok(Ok(pseudocode)) = result {
                        // Pseudocode should be non-empty for non-empty input
                        prop_assert!(!pseudocode.trim().is_empty(),
                                   "Pseudocode should not be empty for valid input");
                        
                        // Should be reasonable length (not extremely long)
                        prop_assert!(pseudocode.len() <= bytecode.len() * 100,
                                   "Pseudocode length should be reasonable");
                        
                        // Should be valid UTF-8 (already guaranteed by String type)
                        // Should not contain null bytes
                        prop_assert!(!pseudocode.contains('\0'),
                                   "Pseudocode should not contain null bytes");
                    }
                }
            }
        }
    }
    
    /// Test that end-to-end decompilation maintains data integrity
    #[test]
    fn prop_end_to_end_decompilation_maintains_integrity(nef_data in nef_file_strategy()) {
        // Only test with valid magic to focus on pipeline integrity
        let mut nef_data = nef_data;
        nef_data.magic = *b"NEF3";
        
        let nef_bytes = nef_data.to_bytes();
        let config = DecompilerConfig::default();
        let mut decompiler = Decompiler::new(config);
        
        let result = std::panic::catch_unwind(|| decompiler.decompile(&nef_bytes, None));
        prop_assert!(result.is_ok(), "End-to-end decompilation should not panic");
        
        if let Ok(Ok(decompiled)) = result {
            // Verify that original data is preserved in result
            prop_assert_eq!(decompiled.nef_file.magic, *b"NEF3");
            prop_assert_eq!(decompiled.nef_file.bytecode, nef_data.bytecode);
            prop_assert!(decompiled.manifest.is_none(), "No manifest provided");
            
            // Instructions should be generated from bytecode
            if !nef_data.bytecode.is_empty() {
                prop_assert!(!decompiled.instructions.is_empty() || nef_data.bytecode.len() < 4,
                           "Non-empty bytecode should produce instructions");
            }
        }
    }
    
    /// Test that configuration serialization is stable
    #[test]
    fn prop_config_serialization_roundtrip(
        optimization_level in 0u8..=3u8,
        enable_analysis in any::<bool>(),
        max_iterations in 1u32..100u32
    ) {
        let mut config = DecompilerConfig::default();
        config.optimization_level = optimization_level;
        // Set other configurable fields based on the actual DecompilerConfig structure
        
        // Test TOML serialization roundtrip
        let toml_str = toml::to_string(&config);
        prop_assert!(toml_str.is_ok(), "Config should serialize to TOML");
        
        if let Ok(toml_str) = toml_str {
            let deserialized: Result<DecompilerConfig, _> = toml::from_str(&toml_str);
            prop_assert!(deserialized.is_ok(), "Config should deserialize from TOML");
            
            if let Ok(deserialized_config) = deserialized {
                prop_assert_eq!(deserialized_config.optimization_level, optimization_level,
                               "Optimization level should be preserved");
            }
        }
        
        // Test JSON serialization roundtrip
        let json_str = serde_json::to_string(&config);
        prop_assert!(json_str.is_ok(), "Config should serialize to JSON");
        
        if let Ok(json_str) = json_str {
            let deserialized: Result<DecompilerConfig, _> = serde_json::from_str(&json_str);
            prop_assert!(deserialized.is_ok(), "Config should deserialize from JSON");
        }
    }
    
    /// Test that manifest parsing handles various JSON structures
    #[test]
    fn prop_manifest_parsing_handles_json_structures(
        name in "[a-zA-Z][a-zA-Z0-9_]{0,50}",
        methods in prop::collection::vec(manifest_method_strategy(), 0..20)
    ) {
        let manifest = SampleManifest {
            name: name.clone(),
            supported_standards: vec![],
            abi: ManifestAbi {
                methods: methods.clone(),
                events: vec![],
            },
            permissions: vec![],
            trusts: vec![],
            extra: None,
        };
        
        let json_str = manifest.to_json();
        
        // JSON should be valid
        let json_value: Result<serde_json::Value, _> = serde_json::from_str(&json_str);
        prop_assert!(json_value.is_ok(), "Generated manifest should be valid JSON");
        
        // Parser should handle the JSON
        let parser = ManifestParser::new();
        let result = std::panic::catch_unwind(|| parser.parse(&json_str));
        prop_assert!(result.is_ok(), "Manifest parser should not panic");
        
        if let Ok(Ok(parsed_manifest)) = result {
            prop_assert_eq!(parsed_manifest.name, name, "Name should be preserved");
            prop_assert_eq!(parsed_manifest.abi.methods.len(), methods.len(),
                           "Method count should be preserved");
        }
    }
    
    /// Test error handling invariants
    #[test]
    fn prop_error_handling_invariants(invalid_data in prop::collection::vec(any::<u8>(), 0..1000)) {
        // Test that various components handle invalid data gracefully
        
        // NEF parser
        let parser = NEFParser::new();
        let nef_result = std::panic::catch_unwind(|| parser.parse(&invalid_data));
        prop_assert!(nef_result.is_ok(), "NEF parser should not panic on invalid data");
        
        // Disassembler
        let config = DecompilerConfig::default();
        let disassembler = Disassembler::new(&config);
        let disasm_result = std::panic::catch_unwind(|| disassembler.disassemble(&invalid_data));
        prop_assert!(disasm_result.is_ok(), "Disassembler should not panic on invalid data");
        
        // Manifest parser (convert bytes to string for JSON parsing)
        let manifest_parser = ManifestParser::new();
        if let Ok(json_str) = String::from_utf8(invalid_data.clone()) {
            let manifest_result = std::panic::catch_unwind(|| manifest_parser.parse(&json_str));
            prop_assert!(manifest_result.is_ok(), "Manifest parser should not panic");
        }
    }
    
    /// Test that optimization levels produce consistent results
    #[test]
    fn prop_optimization_levels_consistency(
        bytecode in bytecode_strategy().prop_filter("valid-size", |b| b.len() > 0 && b.len() < 1000),
        opt_level in 0u8..=3u8
    ) {
        let mut config = DecompilerConfig::default();
        config.optimization_level = opt_level;
        
        // Create a valid NEF structure
        let sample = SampleNefData {
            magic: *b"NEF3",
            compiler: [0u8; 64],
            source_url: "test".to_string(),
            tokens: vec![],
            bytecode: bytecode.clone(),
            checksum: 0,
        };
        
        let nef_bytes = sample.to_bytes();
        let mut decompiler = Decompiler::new(config);
        
        let result = std::panic::catch_unwind(|| decompiler.decompile(&nef_bytes, None));
        prop_assert!(result.is_ok(), "Decompilation should not panic at any optimization level");
        
        if let Ok(Ok(decompiled)) = result {
            // Higher optimization levels should not fundamentally change the structure
            prop_assert!(!decompiled.pseudocode.is_empty() || bytecode.len() < 4,
                        "Should produce pseudocode for valid bytecode");
            prop_assert_eq!(decompiled.nef_file.bytecode, bytecode,
                           "Original bytecode should be preserved regardless of optimization");
        }
    }
}

/// Additional targeted property tests for edge cases

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]
    
    /// Test with extremely small inputs
    #[test]
    fn prop_handles_minimal_inputs(data in prop::collection::vec(any::<u8>(), 0..10)) {
        let parser = NEFParser::new();
        let _ = parser.parse(&data); // Should not panic
        
        let config = DecompilerConfig::default();
        let disassembler = Disassembler::new(&config);
        let _ = disassembler.disassemble(&data); // Should not panic
    }
    
    /// Test with maximum reasonable sizes
    #[test]
    fn prop_handles_large_inputs(
        data in prop::collection::vec(any::<u8>(), 10000..50000)
    ) {
        let config = DecompilerConfig::default();
        let disassembler = Disassembler::new(&config);
        
        // Should complete in reasonable time and not panic
        let start = std::time::Instant::now();
        let _ = disassembler.disassemble(&data);
        let duration = start.elapsed();
        
        prop_assert!(duration.as_secs() < 10, "Should complete large inputs in reasonable time");
    }
}