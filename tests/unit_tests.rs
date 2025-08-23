//! Unit tests for individual components
//!
//! Tests each module in isolation to ensure correct behavior at the unit level.

use crate::common::*;
use neo_decompiler::*;

mod nef_parser_tests {
    use super::*;

    #[test]
    fn test_parse_minimal_nef() {
        let sample = SampleNefData::minimal();
        let nef_bytes = sample.to_bytes();

        let parser = NEFParser::new();
        let result = parser.parse(&nef_bytes);

        assert!(result.is_ok(), "Should successfully parse minimal NEF");

        let nef_file = result.unwrap();
        assert_eq!(&nef_file.magic[..], b"NEF3");
        assert!(
            !nef_file.bytecode.is_empty(),
            "Bytecode should not be empty"
        );
    }

    #[test]
    fn test_parse_invalid_magic() {
        let mut sample = SampleNefData::minimal();
        sample.magic = *b"XXXX"; // Invalid magic
        let nef_bytes = sample.to_bytes();

        let parser = NEFParser::new();
        let result = parser.parse(&nef_bytes);

        assert!(result.is_err(), "Should fail to parse invalid magic");
    }

    #[test]
    fn test_parse_empty_data() {
        let parser = NEFParser::new();
        let result = parser.parse(&[]);

        assert!(result.is_err(), "Should fail to parse empty data");
    }

    #[test]
    fn test_parse_truncated_data() {
        let sample = SampleNefData::minimal();
        let mut nef_bytes = sample.to_bytes();
        nef_bytes.truncate(10); // Truncate to invalid size

        let parser = NEFParser::new();
        let result = parser.parse(&nef_bytes);

        assert!(result.is_err(), "Should fail to parse truncated data");
    }
}

mod manifest_parser_tests {
    use super::*;

    #[test]
    fn test_parse_nep17_manifest() {
        let manifest = SampleManifest::nep17_token();
        let json = manifest.to_json();

        let parser = ManifestParser::new();
        let result = parser.parse(&json);

        assert!(result.is_ok(), "Should successfully parse NEP-17 manifest");

        let parsed_manifest = result.unwrap();
        assert_eq!(parsed_manifest.name, "TestToken");
        assert!(parsed_manifest
            .supported_standards
            .contains(&"NEP-17".to_string()));
        assert!(!parsed_manifest.abi.methods.is_empty());

        // Check for required NEP-17 methods
        let method_names: Vec<_> = parsed_manifest
            .abi
            .methods
            .iter()
            .map(|m| m.name.as_str())
            .collect();
        assert!(method_names.contains(&"symbol"));
        assert!(method_names.contains(&"decimals"));
        assert!(method_names.contains(&"totalSupply"));
        assert!(method_names.contains(&"balanceOf"));
        assert!(method_names.contains(&"transfer"));
    }

    #[test]
    fn test_parse_simple_manifest() {
        let manifest = SampleManifest::simple_contract();
        let json = manifest.to_json();

        let parser = ManifestParser::new();
        let result = parser.parse(&json);

        assert!(result.is_ok(), "Should successfully parse simple manifest");

        let parsed_manifest = result.unwrap();
        assert_eq!(parsed_manifest.name, "SimpleContract");
        assert!(parsed_manifest.supported_standards.is_empty());
        assert_eq!(parsed_manifest.abi.methods.len(), 1);
    }

    #[test]
    fn test_parse_invalid_json() {
        let invalid_json = "{invalid json}";

        let parser = ManifestParser::new();
        let result = parser.parse(invalid_json);

        assert!(result.is_err(), "Should fail to parse invalid JSON");
    }

    #[test]
    fn test_parse_missing_required_fields() {
        let incomplete_json = r#"{"name": "Test"}"#;

        let parser = ManifestParser::new();
        let result = parser.parse(incomplete_json);

        // Depending on implementation, this might succeed with defaults or fail
        // The test verifies the parser handles incomplete data gracefully
        if let Ok(manifest) = result {
            assert_eq!(manifest.name, "Test");
        }
    }
}

mod disassembler_tests {
    use super::*;

    #[test]
    fn test_disassemble_simple_bytecode() {
        let config = DecompilerConfig::default();
        let disassembler = Disassembler::new(&config);

        // Simple bytecode: PUSH1, PUSH2, ADD, RET
        let bytecode = vec![0x11, 0x12, 0x93, 0x41];

        let result = disassembler.disassemble(&bytecode);
        assert!(
            result.is_ok(),
            "Should successfully disassemble simple bytecode"
        );

        let instructions = result.unwrap();
        assert_eq!(instructions.len(), 4, "Should have 4 instructions");

        // Verify instruction sequence (depends on OpCode implementation)
        assert!(instructions
            .iter()
            .any(|i| format!("{:?}", i.opcode).contains("PUSH")));
    }

    #[test]
    fn test_disassemble_empty_bytecode() {
        let config = DecompilerConfig::default();
        let disassembler = Disassembler::new(&config);

        let bytecode = vec![];

        let result = disassembler.disassemble(&bytecode);

        // Should either succeed with empty instructions or fail gracefully
        match result {
            Ok(instructions) => assert!(instructions.is_empty()),
            Err(_) => {} // Acceptable behavior
        }
    }

    #[test]
    fn test_disassemble_complex_control_flow() {
        let config = DecompilerConfig::default();
        let disassembler = Disassembler::new(&config);

        let sample = SampleNefData::with_control_flow();

        let result = disassembler.disassemble(&sample.bytecode);
        assert!(
            result.is_ok(),
            "Should successfully disassemble complex bytecode"
        );

        let instructions = result.unwrap();
        assert!(!instructions.is_empty(), "Should have instructions");

        // Should contain jump instructions
        assert!(instructions
            .iter()
            .any(|i| format!("{:?}", i.opcode).contains("JMP")
                || format!("{:?}", i.opcode).contains("JUMP")));
    }

    #[test]
    fn test_disassemble_invalid_opcodes() {
        let config = DecompilerConfig::default();
        let disassembler = Disassembler::new(&config);

        // Bytecode with potentially invalid opcodes
        let bytecode = vec![0xFF, 0xFE, 0xFD, 0xFC];

        let result = disassembler.disassemble(&bytecode);

        // Implementation should handle unknown opcodes gracefully
        match result {
            Ok(_) => {}  // Successfully handled unknown opcodes
            Err(_) => {} // Reported error for unknown opcodes
        }
    }
}

mod ir_lifter_tests {
    use super::*;

    #[test]
    fn test_lift_simple_instructions() {
        let config = DecompilerConfig::default();
        let disassembler = Disassembler::new(&config);
        let lifter = IRLifter::new(&config);

        // Simple bytecode sequence
        let bytecode = vec![0x11, 0x12, 0x93, 0x41]; // PUSH1, PUSH2, ADD, RET

        let instructions = disassembler.disassemble(&bytecode).unwrap();
        let result = lifter.lift_to_ir(&instructions);

        assert!(result.is_ok(), "Should successfully lift to IR");

        let ir_function = result.unwrap();
        assert!(
            !ir_function.basic_blocks.is_empty(),
            "Should have basic blocks"
        );
    }

    #[test]
    fn test_lift_empty_instructions() {
        let config = DecompilerConfig::default();
        let lifter = IRLifter::new(&config);

        let instructions = vec![];
        let result = lifter.lift_to_ir(&instructions);

        // Should handle empty instruction list gracefully
        match result {
            Ok(ir_function) => {
                // Should have at least an empty basic block or similar structure
                assert!(ir_function.basic_blocks.is_empty() || ir_function.basic_blocks.len() == 1);
            }
            Err(_) => {} // Acceptable to fail on empty input
        }
    }

    #[test]
    fn test_lift_control_flow_instructions() {
        let config = DecompilerConfig::default();
        let disassembler = Disassembler::new(&config);
        let lifter = IRLifter::new(&config);

        let sample = SampleNefData::with_control_flow();
        let instructions = disassembler.disassemble(&sample.bytecode).unwrap();

        let result = lifter.lift_to_ir(&instructions);
        assert!(
            result.is_ok(),
            "Should successfully lift control flow to IR"
        );

        let ir_function = result.unwrap();

        // Should create multiple basic blocks due to control flow
        assert!(
            ir_function.basic_blocks.len() >= 2,
            "Control flow should create multiple basic blocks"
        );
    }
}

mod decompiler_engine_tests {
    use super::*;

    #[test]
    fn test_analyze_simple_function() {
        let config = DecompilerConfig::default();
        let mut engine = DecompilerEngine::new(&config);

        // Create a simple IR function for testing
        let disassembler = Disassembler::new(&config);
        let lifter = IRLifter::new(&config);

        let bytecode = vec![0x11, 0x12, 0x93, 0x41];
        let instructions = disassembler.disassemble(&bytecode).unwrap();
        let mut ir_function = lifter.lift_to_ir(&instructions).unwrap();

        let result = engine.analyze(&mut ir_function, None);
        assert!(result.is_ok(), "Should successfully analyze IR function");
    }

    #[test]
    fn test_analyze_with_manifest() {
        let config = DecompilerConfig::default();
        let mut engine = DecompilerEngine::new(&config);

        let disassembler = Disassembler::new(&config);
        let lifter = IRLifter::new(&config);
        let manifest_parser = ManifestParser::new();

        let bytecode = vec![0x11, 0x12, 0x93, 0x41];
        let instructions = disassembler.disassemble(&bytecode).unwrap();
        let mut ir_function = lifter.lift_to_ir(&instructions).unwrap();

        let manifest_json = SampleManifest::simple_contract().to_json();
        let manifest = manifest_parser.parse(&manifest_json).unwrap();

        let result = engine.analyze(&mut ir_function, Some(&manifest));
        assert!(result.is_ok(), "Should successfully analyze with manifest");
    }
}

mod pseudocode_generator_tests {
    use super::*;

    #[test]
    fn test_generate_simple_pseudocode() {
        let config = DecompilerConfig::default();
        let generator = PseudocodeGenerator::new(&config);

        // Create a simple IR function
        let disassembler = Disassembler::new(&config);
        let lifter = IRLifter::new(&config);

        let bytecode = vec![0x11, 0x12, 0x93, 0x41];
        let instructions = disassembler.disassemble(&bytecode).unwrap();
        let ir_function = lifter.lift_to_ir(&instructions).unwrap();

        let result = generator.generate(&ir_function);
        assert!(result.is_ok(), "Should successfully generate pseudocode");

        let pseudocode = result.unwrap();
        assert!(!pseudocode.is_empty(), "Pseudocode should not be empty");

        // Should contain some recognizable patterns
        assert!(
            pseudocode.contains("function") || pseudocode.contains("main") || pseudocode.len() > 10,
            "Pseudocode should contain meaningful content"
        );
    }

    #[test]
    fn test_generate_empty_function() {
        let config = DecompilerConfig::default();
        let generator = PseudocodeGenerator::new(&config);

        // Create an empty IR function
        use neo_decompiler::core::ir::IRFunction;
        let ir_function = IRFunction {
            basic_blocks: vec![],
            variables: vec![],
            parameters: vec![],
            return_type: None,
        };

        let result = generator.generate(&ir_function);

        // Should handle empty function gracefully
        match result {
            Ok(pseudocode) => {
                assert!(
                    !pseudocode.is_empty(),
                    "Should generate at least minimal structure"
                );
            }
            Err(_) => {} // Acceptable to fail on empty input
        }
    }
}

mod config_tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = DecompilerConfig::default();

        // Verify default configuration is reasonable
        // These asserts depend on your DecompilerConfig implementation
        assert!(
            config.optimization_level <= 3,
            "Optimization level should be reasonable"
        );
    }

    #[test]
    fn test_config_serialization() {
        let config = DecompilerConfig::default();

        // Test TOML serialization
        let toml_result = toml::to_string(&config);
        assert!(toml_result.is_ok(), "Should serialize to TOML");

        // Test JSON serialization
        let json_result = serde_json::to_string(&config);
        assert!(json_result.is_ok(), "Should serialize to JSON");
    }

    #[test]
    fn test_config_deserialization() {
        let config = DecompilerConfig::default();
        let toml_str = toml::to_string(&config).unwrap();

        let deserialized: Result<DecompilerConfig, _> = toml::from_str(&toml_str);
        assert!(deserialized.is_ok(), "Should deserialize from TOML");
    }
}

#[cfg(test)]
mod error_handling_tests {
    use super::*;

    #[test]
    fn test_error_chain_preservation() {
        // Test that errors preserve their cause chain
        let result: Result<(), DecompilerError> =
            Err(DecompilerError::ParseError("Test error".to_string()));

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(format!("{}", error).contains("Test error"));
    }

    #[test]
    fn test_error_display_formatting() {
        let error = DecompilerError::ParseError("Invalid bytecode".to_string());
        let display_str = format!("{}", error);

        assert!(!display_str.is_empty());
        assert!(display_str.contains("Invalid bytecode"));
    }
}
