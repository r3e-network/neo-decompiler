//! Common test utilities and helpers
//! 
//! Shared functionality used across all test modules

use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use neo_decompiler::{Decompiler, DecompilerConfig, NEFParser, ManifestParser};

/// Test harness for creating temporary test environments
pub struct TestEnvironment {
    pub temp_dir: TempDir,
    pub decompiler: Decompiler,
}

impl TestEnvironment {
    /// Create a new test environment with default configuration
    pub fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = DecompilerConfig::default();
        let decompiler = Decompiler::new(config);
        
        Self {
            temp_dir,
            decompiler,
        }
    }

    /// Create a new test environment with custom configuration
    pub fn with_config(config: DecompilerConfig) -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let decompiler = Decompiler::new(config);
        
        Self {
            temp_dir,
            decompiler,
        }
    }

    /// Get the path to the temporary directory
    pub fn temp_path(&self) -> &Path {
        self.temp_dir.path()
    }

    /// Write data to a file in the temp directory
    pub fn write_file(&self, name: &str, content: &[u8]) -> PathBuf {
        let path = self.temp_path().join(name);
        fs::write(&path, content).expect("Failed to write test file");
        path
    }

    /// Write text to a file in the temp directory
    pub fn write_text_file(&self, name: &str, content: &str) -> PathBuf {
        let path = self.temp_path().join(name);
        fs::write(&path, content).expect("Failed to write test file");
        path
    }
}

/// Sample NEF data for testing
pub struct SampleNefData {
    pub magic: [u8; 4],
    pub compiler: [u8; 64],
    pub source_url: String,
    pub tokens: Vec<u8>,
    pub bytecode: Vec<u8>,
    pub checksum: u32,
}

impl SampleNefData {
    /// Create a minimal valid NEF file
    pub fn minimal() -> Self {
        Self {
            magic: *b"NEF3",
            compiler: {
                let mut compiler = [0u8; 64];
                let compiler_str = b"test-compiler-v1.0";
                compiler[..compiler_str.len()].copy_from_slice(compiler_str);
                compiler
            },
            source_url: "https://example.com/test".to_string(),
            tokens: vec![],
            bytecode: vec![
                0x0C, 0x05, 0x48, 0x65, 0x6C, 0x6C, 0x6F,  // PUSHDATA1 "Hello"
                0x0C, 0x05, 0x57, 0x6F, 0x72, 0x6C, 0x64,  // PUSHDATA1 "World"
                0x8A,                                         // SIZE
                0x62, 0x7D, 0xF6, 0xE2,                      // SYSCALL System.Runtime.Log
                0x41,                                         // RET
            ],
            checksum: 0x12345678,
        }
    }

    /// Create a more complex NEF with control flow
    pub fn with_control_flow() -> Self {
        Self {
            magic: *b"NEF3",
            compiler: {
                let mut compiler = [0u8; 64];
                let compiler_str = b"test-compiler-v1.0";
                compiler[..compiler_str.len()].copy_from_slice(compiler_str);
                compiler
            },
            source_url: "https://example.com/test-complex".to_string(),
            tokens: vec![],
            bytecode: vec![
                0x10,                                    // PUSH1
                0x11,                                    // PUSH2
                0x93,                                    // ADD
                0x15,                                    // PUSH3
                0x9F,                                    // GT
                0x2C, 0x05,                             // JMP_IF 5
                0x0C, 0x04, 0x54, 0x72, 0x75, 0x65,    // PUSHDATA1 "True"
                0x2B, 0x05,                             // JMP 5
                0x0C, 0x05, 0x46, 0x61, 0x6C, 0x73, 0x65, // PUSHDATA1 "False"
                0x41,                                    // RET
            ],
            checksum: 0x87654321,
        }
    }

    /// Serialize to NEF format
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut nef_data = Vec::new();
        
        // Magic
        nef_data.extend_from_slice(&self.magic);
        
        // Compiler (64 bytes)
        nef_data.extend_from_slice(&self.compiler);
        
        // Source URL
        let source_bytes = self.source_url.as_bytes();
        nef_data.extend_from_slice(&(source_bytes.len() as u16).to_le_bytes());
        nef_data.extend_from_slice(source_bytes);
        
        // Reserved (1 byte)
        nef_data.push(0);
        
        // Tokens
        nef_data.extend_from_slice(&(self.tokens.len() as u16).to_le_bytes());
        nef_data.extend_from_slice(&self.tokens);
        
        // Reserved (2 bytes)
        nef_data.extend_from_slice(&[0, 0]);
        
        // Bytecode
        nef_data.extend_from_slice(&(self.bytecode.len() as u32).to_le_bytes());
        nef_data.extend_from_slice(&self.bytecode);
        
        // Checksum
        nef_data.extend_from_slice(&self.checksum.to_le_bytes());
        
        nef_data
    }
}

/// Sample contract manifest for testing
pub struct SampleManifest {
    pub name: String,
    pub supported_standards: Vec<String>,
    pub abi: ManifestAbi,
    pub permissions: Vec<ManifestPermission>,
    pub trusts: Vec<String>,
    pub extra: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct ManifestAbi {
    pub methods: Vec<ManifestMethod>,
    pub events: Vec<ManifestEvent>,
}

#[derive(Debug, Clone)]
pub struct ManifestMethod {
    pub name: String,
    pub parameters: Vec<ManifestParameter>,
    pub return_type: String,
    pub offset: u32,
    pub safe: bool,
}

#[derive(Debug, Clone)]
pub struct ManifestEvent {
    pub name: String,
    pub parameters: Vec<ManifestParameter>,
}

#[derive(Debug, Clone)]
pub struct ManifestParameter {
    pub name: String,
    pub param_type: String,
}

#[derive(Debug, Clone)]
pub struct ManifestPermission {
    pub contract: String,
    pub methods: Vec<String>,
}

impl SampleManifest {
    /// Create a basic NEP-17 token manifest
    pub fn nep17_token() -> Self {
        Self {
            name: "TestToken".to_string(),
            supported_standards: vec!["NEP-17".to_string()],
            abi: ManifestAbi {
                methods: vec![
                    ManifestMethod {
                        name: "symbol".to_string(),
                        parameters: vec![],
                        return_type: "String".to_string(),
                        offset: 0,
                        safe: true,
                    },
                    ManifestMethod {
                        name: "decimals".to_string(),
                        parameters: vec![],
                        return_type: "Integer".to_string(),
                        offset: 10,
                        safe: true,
                    },
                    ManifestMethod {
                        name: "totalSupply".to_string(),
                        parameters: vec![],
                        return_type: "Integer".to_string(),
                        offset: 20,
                        safe: true,
                    },
                    ManifestMethod {
                        name: "balanceOf".to_string(),
                        parameters: vec![
                            ManifestParameter {
                                name: "account".to_string(),
                                param_type: "Hash160".to_string(),
                            }
                        ],
                        return_type: "Integer".to_string(),
                        offset: 30,
                        safe: true,
                    },
                    ManifestMethod {
                        name: "transfer".to_string(),
                        parameters: vec![
                            ManifestParameter {
                                name: "from".to_string(),
                                param_type: "Hash160".to_string(),
                            },
                            ManifestParameter {
                                name: "to".to_string(),
                                param_type: "Hash160".to_string(),
                            },
                            ManifestParameter {
                                name: "amount".to_string(),
                                param_type: "Integer".to_string(),
                            },
                            ManifestParameter {
                                name: "data".to_string(),
                                param_type: "Any".to_string(),
                            },
                        ],
                        return_type: "Boolean".to_string(),
                        offset: 40,
                        safe: false,
                    },
                ],
                events: vec![
                    ManifestEvent {
                        name: "Transfer".to_string(),
                        parameters: vec![
                            ManifestParameter {
                                name: "from".to_string(),
                                param_type: "Hash160".to_string(),
                            },
                            ManifestParameter {
                                name: "to".to_string(),
                                param_type: "Hash160".to_string(),
                            },
                            ManifestParameter {
                                name: "amount".to_string(),
                                param_type: "Integer".to_string(),
                            },
                        ],
                    }
                ],
            },
            permissions: vec![
                ManifestPermission {
                    contract: "*".to_string(),
                    methods: vec!["*".to_string()],
                }
            ],
            trusts: vec![],
            extra: None,
        }
    }

    /// Create a simple contract manifest
    pub fn simple_contract() -> Self {
        Self {
            name: "SimpleContract".to_string(),
            supported_standards: vec![],
            abi: ManifestAbi {
                methods: vec![
                    ManifestMethod {
                        name: "main".to_string(),
                        parameters: vec![],
                        return_type: "String".to_string(),
                        offset: 0,
                        safe: true,
                    }
                ],
                events: vec![],
            },
            permissions: vec![],
            trusts: vec![],
            extra: None,
        }
    }

    /// Serialize to JSON format
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(&serde_json::json!({
            "name": self.name,
            "supportedstandards": self.supported_standards,
            "abi": {
                "methods": self.abi.methods.iter().map(|m| serde_json::json!({
                    "name": m.name,
                    "parameters": m.parameters.iter().map(|p| serde_json::json!({
                        "name": p.name,
                        "type": p.param_type
                    })).collect::<Vec<_>>(),
                    "returntype": m.return_type,
                    "offset": m.offset,
                    "safe": m.safe
                })).collect::<Vec<_>>(),
                "events": self.abi.events.iter().map(|e| serde_json::json!({
                    "name": e.name,
                    "parameters": e.parameters.iter().map(|p| serde_json::json!({
                        "name": p.name,
                        "type": p.param_type
                    })).collect::<Vec<_>>()
                })).collect::<Vec<_>>()
            },
            "permissions": self.permissions.iter().map(|p| serde_json::json!({
                "contract": p.contract,
                "methods": p.methods
            })).collect::<Vec<_>>(),
            "trusts": self.trusts,
            "extra": self.extra
        })).expect("Failed to serialize manifest")
    }
}

/// Test assertion helpers
pub mod assertions {
    use std::path::Path;
    use std::fs;
    
    /// Assert that a file exists and is not empty
    pub fn assert_file_exists_and_non_empty(path: &Path) {
        assert!(path.exists(), "File should exist: {:?}", path);
        let content = fs::read(path).expect("Should be able to read file");
        assert!(!content.is_empty(), "File should not be empty: {:?}", path);
    }
    
    /// Assert that a file contains specific text
    pub fn assert_file_contains(path: &Path, expected: &str) {
        assert!(path.exists(), "File should exist: {:?}", path);
        let content = fs::read_to_string(path).expect("Should be able to read file as string");
        assert!(content.contains(expected), 
                "File should contain '{}'. Actual content: {}", expected, content);
    }
    
    /// Assert that JSON is valid and contains expected structure
    pub fn assert_valid_json_with_fields(json_str: &str, required_fields: &[&str]) {
        let json: serde_json::Value = serde_json::from_str(json_str)
            .expect("Should be valid JSON");
        
        if let serde_json::Value::Object(map) = json {
            for field in required_fields {
                assert!(map.contains_key(*field), 
                       "JSON should contain field '{}'. Available fields: {:?}", 
                       field, map.keys().collect::<Vec<_>>());
            }
        } else {
            panic!("JSON should be an object");
        }
    }
    
    /// Assert that pseudocode contains expected patterns
    pub fn assert_pseudocode_contains_patterns(pseudocode: &str, patterns: &[&str]) {
        for pattern in patterns {
            assert!(pseudocode.contains(pattern),
                   "Pseudocode should contain pattern '{}'. Actual: {}", pattern, pseudocode);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sample_nef_data_minimal() {
        let sample = SampleNefData::minimal();
        let bytes = sample.to_bytes();
        
        assert!(bytes.len() > 4, "NEF data should be larger than just magic");
        assert_eq!(&bytes[0..4], b"NEF3", "Should start with NEF3 magic");
    }

    #[test]
    fn test_sample_manifest_nep17() {
        let manifest = SampleManifest::nep17_token();
        let json = manifest.to_json();
        
        // Should be valid JSON
        let _: serde_json::Value = serde_json::from_str(&json)
            .expect("Manifest JSON should be valid");
        
        // Should contain NEP-17 standard
        assert!(json.contains("NEP-17"));
        assert!(json.contains("transfer"));
        assert!(json.contains("Transfer"));
    }

    #[test]
    fn test_environment_setup() {
        let env = TestEnvironment::new();
        
        assert!(env.temp_path().exists());
        assert!(env.temp_path().is_dir());
        
        // Test file writing
        let test_file = env.write_text_file("test.txt", "hello world");
        assert!(test_file.exists());
        
        let content = fs::read_to_string(&test_file).unwrap();
        assert_eq!(content, "hello world");
    }
}