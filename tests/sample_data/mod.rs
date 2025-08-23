//! Sample NEF files and test data for comprehensive testing
//!
//! This module provides realistic sample contracts based on common Neo N3
//! patterns and standards, useful for integration testing and examples.

use crate::common::*;
use serde_json;
use std::fs;
use std::path::Path;

/// Sample NEP-17 token contract with complete implementation
pub struct Nep17TokenSample {
    pub contract_name: String,
    pub symbol: String,
    pub decimals: u8,
    pub total_supply: u64,
}

impl Nep17TokenSample {
    pub fn default() -> Self {
        Self {
            contract_name: "SampleToken".to_string(),
            symbol: "SMPL".to_string(),
            decimals: 8,
            total_supply: 100_000_000,
        }
    }

    /// Generate realistic NEP-17 bytecode
    pub fn generate_bytecode(&self) -> Vec<u8> {
        let mut bytecode = Vec::new();

        // Contract initialization
        bytecode.extend_from_slice(&[
            // Initialize storage prefix for token balances
            0x0C, 0x07, 0x62, 0x61, 0x6C, 0x61, 0x6E, 0x63, 0x65, // PUSHDATA1 "balance"
            0x0C, 0x14, // PUSHDATA1 20 bytes (Hash160 placeholder)
        ]);
        bytecode.extend_from_slice(&[0x00; 20]); // Zero hash160
        bytecode.extend_from_slice(&[
            0x8C, // CAT
            0x10, // PUSH0 (initial balance)
            0x62, 0x01, 0x84, 0x14, // SYSCALL Storage.Put
        ]);

        // Symbol method implementation
        bytecode.extend_from_slice(&[
            0x0C, 0x06, 0x73, 0x79, 0x6D, 0x62, 0x6F, 0x6C, // PUSHDATA1 "symbol"
            0x41, 0x9D, 0x7A, 0x97, // SYSCALL CheckWitness
            0x2C, 0x0F, // JMP_IF skip
        ]);

        // Push symbol string
        let symbol_bytes = self.symbol.as_bytes();
        bytecode.push(0x0C); // PUSHDATA1
        bytecode.push(symbol_bytes.len() as u8);
        bytecode.extend_from_slice(symbol_bytes);
        bytecode.push(0x41); // RET

        // Decimals method
        bytecode.extend_from_slice(&[
            0x0C, 0x08, 0x64, 0x65, 0x63, 0x69, 0x6D, 0x61, 0x6C, 0x73, // "decimals"
        ]);
        bytecode.push(0x10 + self.decimals); // PUSH(decimals)
        bytecode.push(0x41); // RET

        // TotalSupply method
        bytecode.extend_from_slice(&[
            0x0C, 0x0B, 0x74, 0x6F, 0x74, 0x61, 0x6C, 0x53, 0x75, 0x70, 0x70, 0x6C,
            0x79, // "totalSupply"
        ]);

        // Push total supply as integer
        let supply_bytes = self.total_supply.to_le_bytes();
        bytecode.push(0x0C); // PUSHDATA1
        bytecode.push(supply_bytes.len() as u8);
        bytecode.extend_from_slice(&supply_bytes);
        bytecode.push(0x41); // RET

        // BalanceOf method
        bytecode.extend_from_slice(&[
            0x0C, 0x09, 0x62, 0x61, 0x6C, 0x61, 0x6E, 0x63, 0x65, 0x4F, 0x66, // "balanceOf"
            0x6B, // DUP (account parameter)
            0x0C, 0x07, 0x62, 0x61, 0x6C, 0x61, 0x6E, 0x63, 0x65, // "balance" prefix
            0x6C, // ROT
            0x8C, // CAT
            0x62, 0x7D, 0xDA, 0x17, // SYSCALL Storage.Get
            0x41, // RET
        ]);

        // Transfer method (simplified)
        bytecode.extend_from_slice(&[
            0x0C, 0x08, 0x74, 0x72, 0x61, 0x6E, 0x73, 0x66, 0x65, 0x72, // "transfer"
            // Parameter validation
            0x6B, // DUP (from)
            0x41, 0x9D, 0x7A, 0x97, // CheckWitness
            0x2C, 0x05, // JMP_IF continue
            0x10, // PUSH0 (false)
            0x41, // RET
            // Transfer logic (simplified)
            0x11, // PUSH1 (true)
            0x41, // RET
        ]);

        bytecode
    }

    /// Generate corresponding manifest
    pub fn generate_manifest(&self) -> String {
        serde_json::to_string_pretty(&serde_json::json!({
            "name": self.contract_name,
            "supportedstandards": ["NEP-17"],
            "abi": {
                "methods": [
                    {
                        "name": "symbol",
                        "parameters": [],
                        "returntype": "String",
                        "offset": 0,
                        "safe": true
                    },
                    {
                        "name": "decimals",
                        "parameters": [],
                        "returntype": "Integer",
                        "offset": 50,
                        "safe": true
                    },
                    {
                        "name": "totalSupply",
                        "parameters": [],
                        "returntype": "Integer",
                        "offset": 100,
                        "safe": true
                    },
                    {
                        "name": "balanceOf",
                        "parameters": [
                            {
                                "name": "account",
                                "type": "Hash160"
                            }
                        ],
                        "returntype": "Integer",
                        "offset": 150,
                        "safe": true
                    },
                    {
                        "name": "transfer",
                        "parameters": [
                            {
                                "name": "from",
                                "type": "Hash160"
                            },
                            {
                                "name": "to",
                                "type": "Hash160"
                            },
                            {
                                "name": "amount",
                                "type": "Integer"
                            },
                            {
                                "name": "data",
                                "type": "Any"
                            }
                        ],
                        "returntype": "Boolean",
                        "offset": 200,
                        "safe": false
                    }
                ],
                "events": [
                    {
                        "name": "Transfer",
                        "parameters": [
                            {
                                "name": "from",
                                "type": "Hash160"
                            },
                            {
                                "name": "to",
                                "type": "Hash160"
                            },
                            {
                                "name": "amount",
                                "type": "Integer"
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
                "Author": "Neo Test Suite",
                "Description": "Sample NEP-17 token for testing"
            }
        }))
        .expect("Failed to generate manifest JSON")
    }

    /// Create complete NEF file
    pub fn generate_nef(&self) -> SampleNefData {
        let bytecode = self.generate_bytecode();

        SampleNefData {
            magic: *b"NEF3",
            compiler: {
                let mut compiler = [0u8; 64];
                let compiler_str = b"neo-test-compiler-v3.0.1";
                compiler[..compiler_str.len()].copy_from_slice(compiler_str);
                compiler
            },
            source_url: format!(
                "https://github.com/neo-project/{}",
                self.contract_name.to_lowercase()
            ),
            tokens: vec![],
            bytecode,
            checksum: calculate_nef_checksum(&bytecode),
        }
    }
}

/// Sample NEP-11 NFT contract
pub struct Nep11NftSample {
    pub contract_name: String,
    pub symbol: String,
}

impl Nep11NftSample {
    pub fn default() -> Self {
        Self {
            contract_name: "SampleNFT".to_string(),
            symbol: "SNFT".to_string(),
        }
    }

    pub fn generate_bytecode(&self) -> Vec<u8> {
        let mut bytecode = Vec::new();

        // NFT-specific methods
        bytecode.extend_from_slice(&[
            // Symbol method
            0x0C, 0x06, 0x73, 0x79, 0x6D, 0x62, 0x6F, 0x6C, // "symbol"
        ]);

        let symbol_bytes = self.symbol.as_bytes();
        bytecode.push(0x0C);
        bytecode.push(symbol_bytes.len() as u8);
        bytecode.extend_from_slice(symbol_bytes);
        bytecode.push(0x41); // RET

        // TokensOf method
        bytecode.extend_from_slice(&[
            0x0C, 0x08, 0x74, 0x6F, 0x6B, 0x65, 0x6E, 0x73, 0x4F, 0x66, // "tokensOf"
            // Implementation placeholder
            0x40, 0x08, // INITSLOT 0, 8 (local variables)
            0x0C, 0x10, // PUSHDATA1 empty array
        ]);
        bytecode.extend_from_slice(&[0x00; 16]); // Empty array data
        bytecode.push(0x41); // RET

        // OwnerOf method
        bytecode.extend_from_slice(&[
            0x0C, 0x07, 0x6F, 0x77, 0x6E, 0x65, 0x72, 0x4F, 0x66, // "ownerOf"
            // Token ID parameter handling
            0x6B, // DUP
            0x0C, 0x05, 0x6F, 0x77, 0x6E, 0x65, 0x72, // "owner" prefix
            0x6C, // ROT
            0x8C, // CAT
            0x62, 0x7D, 0xDA, 0x17, // Storage.Get
            0x41, // RET
        ]);

        // Transfer method
        bytecode.extend_from_slice(&[
            0x0C, 0x08, 0x74, 0x72, 0x61, 0x6E, 0x73, 0x66, 0x65, 0x72, // "transfer"
            0x11, // PUSH1 (simplified - always return true)
            0x41, // RET
        ]);

        bytecode
    }

    pub fn generate_manifest(&self) -> String {
        serde_json::to_string_pretty(&serde_json::json!({
            "name": self.contract_name,
            "supportedstandards": ["NEP-11"],
            "abi": {
                "methods": [
                    {
                        "name": "symbol",
                        "parameters": [],
                        "returntype": "String",
                        "offset": 0,
                        "safe": true
                    },
                    {
                        "name": "decimals",
                        "parameters": [],
                        "returntype": "Integer",
                        "offset": 20,
                        "safe": true
                    },
                    {
                        "name": "totalSupply",
                        "parameters": [],
                        "returntype": "Integer",
                        "offset": 30,
                        "safe": true
                    },
                    {
                        "name": "balanceOf",
                        "parameters": [
                            {
                                "name": "owner",
                                "type": "Hash160"
                            }
                        ],
                        "returntype": "Integer",
                        "offset": 40,
                        "safe": true
                    },
                    {
                        "name": "tokensOf",
                        "parameters": [
                            {
                                "name": "owner",
                                "type": "Hash160"
                            }
                        ],
                        "returntype": "Array",
                        "offset": 60,
                        "safe": true
                    },
                    {
                        "name": "ownerOf",
                        "parameters": [
                            {
                                "name": "tokenId",
                                "type": "ByteArray"
                            }
                        ],
                        "returntype": "Hash160",
                        "offset": 80,
                        "safe": true
                    },
                    {
                        "name": "transfer",
                        "parameters": [
                            {
                                "name": "to",
                                "type": "Hash160"
                            },
                            {
                                "name": "tokenId",
                                "type": "ByteArray"
                            },
                            {
                                "name": "data",
                                "type": "Any"
                            }
                        ],
                        "returntype": "Boolean",
                        "offset": 100,
                        "safe": false
                    }
                ],
                "events": [
                    {
                        "name": "Transfer",
                        "parameters": [
                            {
                                "name": "from",
                                "type": "Hash160"
                            },
                            {
                                "name": "to",
                                "type": "Hash160"
                            },
                            {
                                "name": "amount",
                                "type": "Integer"
                            },
                            {
                                "name": "tokenId",
                                "type": "ByteArray"
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
                "Author": "Neo Test Suite",
                "Description": "Sample NEP-11 NFT for testing"
            }
        }))
        .expect("Failed to generate NEP-11 manifest")
    }

    pub fn generate_nef(&self) -> SampleNefData {
        let bytecode = self.generate_bytecode();

        SampleNefData {
            magic: *b"NEF3",
            compiler: {
                let mut compiler = [0u8; 64];
                let compiler_str = b"neo-test-compiler-v3.0.1";
                compiler[..compiler_str.len()].copy_from_slice(compiler_str);
                compiler
            },
            source_url: format!(
                "https://github.com/neo-project/{}",
                self.contract_name.to_lowercase()
            ),
            tokens: vec![],
            bytecode,
            checksum: calculate_nef_checksum(&bytecode),
        }
    }
}

/// Complex multi-function contract sample
pub struct ComplexContractSample {
    pub name: String,
    pub functions: Vec<ContractFunction>,
}

pub struct ContractFunction {
    pub name: String,
    pub parameters: Vec<String>,
    pub has_loops: bool,
    pub has_conditions: bool,
    pub calls_syscalls: bool,
}

impl ComplexContractSample {
    pub fn default() -> Self {
        Self {
            name: "ComplexContract".to_string(),
            functions: vec![
                ContractFunction {
                    name: "calculateHash".to_string(),
                    parameters: vec!["data".to_string()],
                    has_loops: false,
                    has_conditions: true,
                    calls_syscalls: true,
                },
                ContractFunction {
                    name: "processArray".to_string(),
                    parameters: vec!["items".to_string(), "threshold".to_string()],
                    has_loops: true,
                    has_conditions: true,
                    calls_syscalls: false,
                },
                ContractFunction {
                    name: "verifySignatures".to_string(),
                    parameters: vec!["message".to_string(), "signatures".to_string()],
                    has_loops: true,
                    has_conditions: true,
                    calls_syscalls: true,
                },
            ],
        }
    }

    pub fn generate_bytecode(&self) -> Vec<u8> {
        let mut bytecode = Vec::new();

        for func in &self.functions {
            // Function name check
            let name_bytes = func.name.as_bytes();
            bytecode.push(0x0C); // PUSHDATA1
            bytecode.push(name_bytes.len() as u8);
            bytecode.extend_from_slice(name_bytes);

            // Function implementation
            if func.has_conditions {
                bytecode.extend_from_slice(&[
                    0x11, 0x12, 0x9F, // PUSH1, PUSH2, GT
                    0x2C, 0x08, // JMP_IF 8 bytes
                    0x0C, 0x05, 0x45, 0x72, 0x72, 0x6F, 0x72, // "Error"
                    0x3A, // THROW
                ]);
            }

            if func.has_loops {
                bytecode.extend_from_slice(&[
                    0x15, // PUSH5 (loop counter)
                    // Loop start
                    0x6B, // DUP
                    0x10, // PUSH0
                    0x9F, // GT
                    0x2C, 0x0C, // JMP_IF 12 bytes (exit loop)
                    // Loop body
                    0x6B, // DUP
                    0x11, // PUSH1
                    0x94, // SUB
                    0x0C, 0x04, 0x64, 0x6F, 0x6E, 0x65, // "done"
                    0x2B, 0xF0, // JMP back to loop start (-16)
                    // Exit loop
                    0x75, // DROP
                ]);
            }

            if func.calls_syscalls {
                bytecode.extend_from_slice(&[
                    0x0C, 0x0A, 0x48, 0x65, 0x6C, 0x6C, 0x6F, 0x57, 0x6F, 0x72, 0x6C,
                    0x64, // "HelloWorld"
                    0x62, 0x7D, 0xF6, 0xE2, // System.Runtime.Log
                    0x62, 0xE6, 0x33, 0x8C, // Crypto.SHA256
                ]);
            }

            // Function return
            bytecode.extend_from_slice(&[
                0x11, // PUSH1 (success)
                0x41, // RET
            ]);
        }

        bytecode
    }

    pub fn generate_manifest(&self) -> String {
        let methods: Vec<_> = self
            .functions
            .iter()
            .enumerate()
            .map(|(i, func)| {
                serde_json::json!({
                    "name": func.name,
                    "parameters": func.parameters.iter().map(|p| serde_json::json!({
                        "name": p,
                        "type": "Any"
                    })).collect::<Vec<_>>(),
                    "returntype": "Any",
                    "offset": i * 100,
                    "safe": !func.calls_syscalls
                })
            })
            .collect();

        serde_json::to_string_pretty(&serde_json::json!({
            "name": self.name,
            "supportedstandards": [],
            "abi": {
                "methods": methods,
                "events": []
            },
            "permissions": [
                {
                    "contract": "*",
                    "methods": "*"
                }
            ],
            "trusts": [],
            "extra": {
                "Author": "Neo Test Suite",
                "Description": "Complex contract with multiple function types"
            }
        }))
        .expect("Failed to generate complex contract manifest")
    }

    pub fn generate_nef(&self) -> SampleNefData {
        let bytecode = self.generate_bytecode();

        SampleNefData {
            magic: *b"NEF3",
            compiler: {
                let mut compiler = [0u8; 64];
                let compiler_str = b"neo-test-compiler-v3.1.0";
                compiler[..compiler_str.len()].copy_from_slice(compiler_str);
                compiler
            },
            source_url: format!(
                "https://github.com/neo-project/{}",
                self.name.to_lowercase()
            ),
            tokens: vec![],
            bytecode,
            checksum: calculate_nef_checksum(&bytecode),
        }
    }
}

/// Utility functions for sample data

/// Calculate a simple checksum for NEF files (placeholder implementation)
fn calculate_nef_checksum(bytecode: &[u8]) -> u32 {
    let mut checksum = 0u32;
    for (i, &byte) in bytecode.iter().enumerate() {
        checksum = checksum.wrapping_add((byte as u32) * ((i as u32) + 1));
    }
    checksum
}

/// Save sample contracts to files for testing
pub fn save_samples_to_directory<P: AsRef<Path>>(
    directory: P,
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = directory.as_ref();
    fs::create_dir_all(dir)?;

    // NEP-17 Token
    let nep17 = Nep17TokenSample::default();
    let nef_data = nep17.generate_nef();
    let nef_bytes = nef_data.to_bytes();
    fs::write(dir.join("nep17_token.nef"), &nef_bytes)?;
    fs::write(
        dir.join("nep17_token.manifest.json"),
        nep17.generate_manifest(),
    )?;

    // NEP-11 NFT
    let nep11 = Nep11NftSample::default();
    let nef_data = nep11.generate_nef();
    let nef_bytes = nef_data.to_bytes();
    fs::write(dir.join("nep11_nft.nef"), &nef_bytes)?;
    fs::write(
        dir.join("nep11_nft.manifest.json"),
        nep11.generate_manifest(),
    )?;

    // Complex Contract
    let complex = ComplexContractSample::default();
    let nef_data = complex.generate_nef();
    let nef_bytes = nef_data.to_bytes();
    fs::write(dir.join("complex_contract.nef"), &nef_bytes)?;
    fs::write(
        dir.join("complex_contract.manifest.json"),
        complex.generate_manifest(),
    )?;

    // Minimal test cases
    let minimal = SampleNefData::minimal();
    fs::write(dir.join("minimal.nef"), minimal.to_bytes())?;
    let minimal_manifest = SampleManifest::simple_contract();
    fs::write(
        dir.join("minimal.manifest.json"),
        minimal_manifest.to_json(),
    )?;

    // Control flow test
    let control_flow = SampleNefData::with_control_flow();
    fs::write(dir.join("control_flow.nef"), control_flow.to_bytes())?;

    Ok(())
}

/// Load all sample contracts from a directory
pub fn load_samples_from_directory<P: AsRef<Path>>(
    directory: P,
) -> Result<Vec<(Vec<u8>, Option<String>)>, Box<dyn std::error::Error>> {
    let dir = directory.as_ref();
    let mut samples = Vec::new();

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if let Some(extension) = path.extension() {
            if extension == "nef" {
                let nef_data = fs::read(&path)?;

                // Look for corresponding manifest
                let manifest_path = path.with_extension("manifest.json");
                let manifest_data = if manifest_path.exists() {
                    Some(fs::read_to_string(&manifest_path)?)
                } else {
                    None
                };

                samples.push((nef_data, manifest_data));
            }
        }
    }

    Ok(samples)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_nep17_sample_generation() {
        let nep17 = Nep17TokenSample::default();
        let bytecode = nep17.generate_bytecode();
        let manifest = nep17.generate_manifest();
        let nef = nep17.generate_nef();

        assert!(!bytecode.is_empty(), "Should generate bytecode");
        assert!(manifest.contains("NEP-17"), "Should be NEP-17 compliant");
        assert_eq!(nef.magic, *b"NEF3", "Should have correct NEF magic");
        assert_eq!(
            nef.bytecode, bytecode,
            "NEF should contain generated bytecode"
        );
    }

    #[test]
    fn test_nep11_sample_generation() {
        let nep11 = Nep11NftSample::default();
        let bytecode = nep11.generate_bytecode();
        let manifest = nep11.generate_manifest();

        assert!(!bytecode.is_empty(), "Should generate bytecode");
        assert!(manifest.contains("NEP-11"), "Should be NEP-11 compliant");
        assert!(
            manifest.contains("tokensOf"),
            "Should have NFT-specific methods"
        );
    }

    #[test]
    fn test_complex_contract_generation() {
        let complex = ComplexContractSample::default();
        let bytecode = complex.generate_bytecode();
        let manifest = complex.generate_manifest();

        assert!(!bytecode.is_empty(), "Should generate bytecode");
        assert!(
            manifest.contains("calculateHash"),
            "Should include all functions"
        );
        assert!(
            manifest.contains("processArray"),
            "Should include all functions"
        );
        assert!(
            manifest.contains("verifySignatures"),
            "Should include all functions"
        );
    }

    #[test]
    fn test_save_and_load_samples() {
        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path();

        // Save samples
        save_samples_to_directory(dir_path).unwrap();

        // Verify files were created
        assert!(dir_path.join("nep17_token.nef").exists());
        assert!(dir_path.join("nep17_token.manifest.json").exists());
        assert!(dir_path.join("nep11_nft.nef").exists());
        assert!(dir_path.join("complex_contract.nef").exists());
        assert!(dir_path.join("minimal.nef").exists());

        // Load samples back
        let samples = load_samples_from_directory(dir_path).unwrap();
        assert!(samples.len() >= 4, "Should load multiple samples");

        // Verify sample structure
        for (nef_data, manifest_data) in samples {
            assert!(!nef_data.is_empty(), "NEF data should not be empty");
            assert!(nef_data.starts_with(b"NEF"), "Should have NEF magic");

            if let Some(manifest) = manifest_data {
                let _: serde_json::Value =
                    serde_json::from_str(&manifest).expect("Manifest should be valid JSON");
            }
        }
    }
}
