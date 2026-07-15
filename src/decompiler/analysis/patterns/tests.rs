//! Pattern-identification tests.

use super::*;
use crate::nef::NefHeader;

fn nef(compiler: &str, source: &str) -> NefFile {
    NefFile {
        header: NefHeader {
            magic: *b"NEF3",
            compiler: compiler.to_string(),
            source: source.to_string(),
        },
        method_tokens: Vec::new(),
        script: Vec::new(),
        checksum: 0,
    }
}

#[test]
fn manifest_standard_is_high_confidence() {
    let manifest: ContractManifest = serde_json::from_str(
        r#"{"name":"Token","abi":{"methods":[],"events":[]},"supportedstandards":["NEP-17"]}"#,
    )
    .expect("manifest fixture");
    let info = identify_patterns(&nef("Neo.Compiler.CSharp 3", ""), &[], Some(&manifest));
    assert_eq!(info.standards, vec!["NEP-17"]);
    assert_eq!(info.language.as_deref(), Some("C#"));
    assert_eq!(info.confidence, PatternConfidence::High);
    assert!(info
        .evidence
        .iter()
        .any(|entry| entry.source == "nef.header.compiler"));
}

#[test]
fn weak_metadata_does_not_claim_a_standard() {
    let info = identify_patterns(&nef("", "contract.py"), &[], None);
    assert!(info.standards.is_empty());
    assert_eq!(info.language, None);
    assert_eq!(info.confidence, PatternConfidence::Low);
    assert!(info
        .evidence
        .iter()
        .any(|entry| entry.source == "nef.header.source"));
}

#[test]
fn csharp_source_paths_infer_only_the_supported_target() {
    for source in [r"C:\contracts\Token.cs", "/contracts/Token.csproj"] {
        let info = identify_patterns(&nef("", source), &[], None);
        assert_eq!(info.language.as_deref(), Some("C#"));
    }
}

#[test]
fn unsupported_source_metadata_is_not_claimed_as_a_renderer() {
    for (compiler, source) in [
        ("boa 1", "contract.py"),
        ("neo-go 1", "contract.go"),
        ("neo-rustc 1", "contract.rs"),
        ("neo-java-compiler 1", "contract.java"),
        ("neo-javascript-compiler 1", "contract.ts"),
        ("Neo.Compiler.Rust 1", "contract.rs"),
        ("Neo.Compiler.Java 1", "contract.java"),
    ] {
        let info = identify_patterns(&nef(compiler, source), &[], None);
        assert_eq!(info.language, None, "metadata {compiler:?} {source:?}");
    }
}

#[test]
fn short_csharp_compiler_tags_infer_language() {
    for compiler in ["cs", "cs__", "cs 3.7", "CSharp", "Neo.Compiler.CSharp 3"] {
        let info = identify_patterns(&nef(compiler, ""), &[], None);
        assert_eq!(
            info.language.as_deref(),
            Some("C#"),
            "compiler tag {compiler:?}"
        );
    }
}

#[test]
fn compiler_tags_require_explicit_csharp_tokens() {
    for compiler in ["notcsharp", "CSharpX", "Neo.Compiler.CSharpX", "cscompiler"] {
        let info = identify_patterns(&nef(compiler, ""), &[], None);
        assert_eq!(info.language, None, "compiler tag {compiler:?}");
    }
}

#[test]
fn backward_jump_reports_loops_pattern() {
    let info = identify_patterns(
        &nef("", ""),
        &[Instruction::new(0, OpCode::Jmp, Some(Operand::Jump(-4)))],
        None,
    );
    assert!(info.patterns.iter().any(|pattern| pattern == "loops"));
    assert!(info.evidence.iter().any(|entry| {
        entry.source == "bytecode.control_flow" && entry.value == "backward jump"
    }));
}

#[test]
fn events_manifest_reports_events_pattern_with_evidence() {
    let manifest: ContractManifest = serde_json::from_str(
        r#"{
                "name":"Events",
                "abi":{
                    "methods":[{"name":"main","parameters":[],"returntype":"Integer","offset":0}],
                    "events":[
                        {"name":"Transfer","parameters":[
                            {"name":"from","type":"Hash160"},
                            {"name":"to","type":"Hash160"},
                            {"name":"amount","type":"Integer"}
                        ]},
                        {"name":"Notify","parameters":[{"name":"value","type":"Any"}]}
                    ]
                }
            }"#,
    )
    .expect("manifest fixture");
    let info = identify_patterns(&nef("evnt", ""), &[], Some(&manifest));
    assert!(info.patterns.iter().any(|pattern| pattern == "events"));
    assert!(info
        .evidence
        .iter()
        .any(|entry| { entry.source == "manifest.abi.events" && entry.value == "2" }));
    assert!(
        info.confidence == PatternConfidence::Medium || info.confidence == PatternConfidence::High
    );
}

#[test]
fn crypto_syscalls_report_signature_and_multisig_patterns() {
    let info = identify_patterns(
        &nef("", ""),
        &[Instruction::new(
            0,
            OpCode::Syscall,
            Some(Operand::Syscall(0x3ADCD09E)),
        )],
        None,
    );
    assert_eq!(info.patterns, vec!["multisig", "signature_verification"]);
    assert!(info.evidence.iter().any(|entry| {
        entry.source == "syscall" && entry.value == "System.Crypto.CheckMultisig"
    }));
}

#[test]
fn check_witness_reports_authorization_pattern() {
    let info = identify_patterns(
        &nef("", ""),
        &[Instruction::new(
            0,
            OpCode::Syscall,
            Some(Operand::Syscall(0x8CEC27F8)),
        )],
        None,
    );
    assert_eq!(info.patterns, vec!["authorization"]);
    assert!(info.evidence.iter().any(|entry| {
        entry.source == "syscall" && entry.value == "System.Runtime.CheckWitness"
    }));
}

#[test]
fn caller_and_signer_syscalls_report_context_patterns() {
    let info = identify_patterns(
        &nef("", ""),
        &[
            Instruction::new(0, OpCode::Syscall, Some(Operand::Syscall(0x3C6E5339))),
            Instruction::new(5, OpCode::Syscall, Some(Operand::Syscall(0x8B18F1AC))),
        ],
        None,
    );
    assert_eq!(
        info.patterns,
        vec!["caller_context", "signer_introspection"]
    );
    assert!(info.evidence.iter().any(|entry| {
        entry.source == "syscall" && entry.value == "System.Runtime.GetCallingScriptHash"
    }));
    assert!(info.evidence.iter().any(|entry| {
        entry.source == "syscall" && entry.value == "System.Runtime.CurrentSigners"
    }));
}

#[test]
fn storage_runtime_and_account_syscalls_report_behavior_patterns() {
    let hashes = [
        0x31E8_5D92, // System.Storage.Get
        0x0AE3_0C39, // System.Storage.Local.Put
        0xEDC5_582F, // System.Storage.Delete
        0x9AB8_30DF, // System.Storage.Find
        0x9CED_089C, // System.Iterator.Next
        0xDC92_494C, // System.Runtime.GetAddressVersion
        0x09E9_336A, // System.Contract.CreateMultisigAccount
        0xBC8C_5AC3, // System.Runtime.BurnGas
    ];
    let instructions = hashes
        .iter()
        .enumerate()
        .map(|(index, hash)| {
            Instruction::new(index * 5, OpCode::Syscall, Some(Operand::Syscall(*hash)))
        })
        .collect::<Vec<_>>();

    let info = identify_patterns(&nef("", ""), &instructions, None);

    assert_eq!(
        info.patterns,
        vec![
            "account_creation",
            "gas_management",
            "iterator_usage",
            "runtime_context",
            "storage",
            "storage_deletes",
            "storage_iteration",
            "storage_reads",
            "storage_writes",
        ]
    );
    assert_eq!(
        info.evidence
            .iter()
            .filter(|entry| entry.source == "syscall")
            .count(),
        12
    );
    assert!(info
        .evidence
        .iter()
        .any(|entry| { entry.source == "syscall" && entry.value == "System.Storage.Local.Put" }));
    assert!(info.evidence.iter().any(|entry| {
        entry.source == "syscall" && entry.value == "System.Runtime.GetAddressVersion"
    }));
}

#[test]
fn wildcard_permissions_are_reported_as_behavior_evidence() {
    let manifest: ContractManifest = serde_json::from_str(
            r#"{"name":"C","abi":{"methods":[],"events":[]},"permissions":[{"contract":"*","methods":"*"}]}"#,
        )
        .expect("manifest fixture");
    let info = identify_patterns(&nef("", ""), &[], Some(&manifest));
    assert_eq!(
        info.patterns,
        vec!["call_permissions", "wildcard_permissions"]
    );
}

#[test]
fn abi_events_are_reported_as_a_contract_pattern() {
    let manifest: ContractManifest = serde_json::from_str(
        r#"{"name":"C","abi":{"methods":[],"events":[{"name":"Updated","parameters":[]}]}}"#,
    )
    .expect("manifest fixture");
    let info = identify_patterns(&nef("", ""), &[], Some(&manifest));
    assert_eq!(info.patterns, vec!["events"]);
    assert!(info
        .evidence
        .iter()
        .any(|entry| { entry.source == "manifest.abi.events" && entry.value == "1" }));
}

#[test]
fn transfer_event_and_method_report_token_transfer_behavior() {
    let manifest: ContractManifest = serde_json::from_str(
            r#"{"name":"Token","abi":{"methods":[{"name":"transfer","returntype":"Boolean"}],"events":[{"name":"Transfer","parameters":[]}]}}"#,
        )
        .expect("manifest fixture");
    let info = identify_patterns(&nef("", ""), &[], Some(&manifest));
    assert_eq!(info.patterns, vec!["events", "token_transfers"]);
    assert!(info.evidence.iter().any(|entry| {
        entry.source == "manifest.abi.methods" && entry.value == "transfer + Transfer"
    }));
}

#[test]
fn owner_and_transfer_methods_report_ownership_pattern() {
    let manifest: ContractManifest = serde_json::from_str(
            r#"{"name":"C","abi":{"methods":[{"name":"owner","parameters":[],"returntype":"Hash160"},{"name":"transferOwnership","parameters":[],"returntype":"Boolean"}],"events":[]}}"#,
        )
        .expect("manifest fixture");
    let info = identify_patterns(&nef("", ""), &[], Some(&manifest));
    assert_eq!(info.patterns, vec!["ownership"]);
}

#[test]
fn token_lifecycle_methods_report_conservative_behavior_patterns() {
    let manifest: ContractManifest = serde_json::from_str(
            r#"{"name":"Token","abi":{"methods":[{"name":"mint","returntype":"Any"},{"name":"burn","returntype":"Any"},{"name":"pause","returntype":"Any"},{"name":"unpause","returntype":"Any"}],"events":[]}}"#,
        )
        .expect("manifest fixture");
    let info = identify_patterns(&nef("", ""), &[], Some(&manifest));
    assert_eq!(info.patterns, vec!["burning", "minting", "pausable"]);
    assert!(info
        .evidence
        .iter()
        .any(|entry| { entry.source == "manifest.abi.methods" && entry.value == "pause,unpause" }));
}

#[test]
fn royalty_info_reports_nep24_and_royalties_patterns() {
    let manifest: ContractManifest = serde_json::from_str(
            r#"{"name":"Royalty","abi":{"methods":[{"name":"royaltyInfo","parameters":[],"returntype":"Array"}],"events":[]}}"#,
        )
        .expect("manifest fixture");
    let info = identify_patterns(&nef("", ""), &[], Some(&manifest));
    assert_eq!(info.standards, vec!["NEP-24"]);
    assert_eq!(info.patterns, vec!["NEP-24", "royalties"]);
    assert!(info
        .evidence
        .iter()
        .any(|entry| { entry.source == "manifest.abi.methods" && entry.value == "royaltyInfo" }));
}

#[test]
fn token_payment_callbacks_report_receiver_behavior_without_standard_guess() {
    let manifest: ContractManifest = serde_json::from_str(
            r#"{"name":"Receiver","abi":{"methods":[{"name":"onNEP17Payment","parameters":[],"returntype":"Void"}],"events":[]}}"#,
        )
        .expect("manifest fixture");
    let info = identify_patterns(&nef("", ""), &[], Some(&manifest));
    assert_eq!(info.standards, Vec::<String>::new());
    assert_eq!(info.patterns, vec!["token_receiver"]);
    assert!(info.evidence.iter().any(|entry| {
        entry.source == "manifest.abi.methods" && entry.value == "onNEP17Payment"
    }));
}

#[test]
fn method_tokens_and_calls_are_reported_without_standard_guesses() {
    let nef = NefFile {
        method_tokens: vec![crate::nef::MethodToken {
            hash: [0; 20],
            method: "transfer".to_string(),
            parameters_count: 0,
            has_return_value: false,
            call_flags: 0,
        }],
        ..nef("", "")
    };
    let info = identify_patterns(
        &nef,
        &[Instruction::new(0, OpCode::CallT, Some(Operand::U16(0)))],
        None,
    );
    assert_eq!(info.patterns, vec!["external_calls", "method_tokens"]);
    assert!(info.standards.is_empty());
}

#[test]
fn native_oracle_method_tokens_report_oracle_behavior() {
    let nef = NefFile {
        method_tokens: vec![crate::nef::MethodToken {
            hash: [
                0x58, 0x87, 0x17, 0x11, 0x7E, 0x0A, 0xA8, 0x10, 0x72, 0xAF, 0xAB, 0x71, 0xD2, 0xDD,
                0x89, 0xFE, 0x7C, 0x4B, 0x92, 0xFE,
            ],
            method: "Request".to_string(),
            parameters_count: 0,
            has_return_value: true,
            call_flags: 0x0F,
        }],
        ..nef("", "")
    };
    let info = identify_patterns(&nef, &[], None);
    assert_eq!(
        info.patterns,
        vec!["method_tokens", "native_contract_calls", "oracle"]
    );
    assert!(info
        .evidence
        .iter()
        .any(|entry| entry.value == "OracleContract::Request"));
}

#[test]
fn native_contract_management_update_reports_upgradeability() {
    let nef = NefFile {
        method_tokens: vec![crate::nef::MethodToken {
            hash: [
                0xFD, 0xA3, 0xFA, 0x43, 0x46, 0xEA, 0x53, 0x2A, 0x25, 0x8F, 0xC4, 0x97, 0xDD, 0xAD,
                0xDB, 0x64, 0x37, 0xC9, 0xFD, 0xFF,
            ],
            method: "Update".to_string(),
            parameters_count: 0,
            has_return_value: false,
            call_flags: 0x0F,
        }],
        ..nef("", "")
    };
    let info = identify_patterns(&nef, &[], None);
    assert_eq!(
        info.patterns,
        vec![
            "contract_lifecycle",
            "contract_management",
            "method_tokens",
            "native_contract_calls",
            "upgradeable"
        ]
    );
}

#[test]
fn native_role_management_method_tokens_report_role_management() {
    let nef = NefFile {
        method_tokens: vec![crate::nef::MethodToken {
            hash: [
                0xE2, 0x95, 0xE3, 0x91, 0x54, 0x4C, 0x17, 0x8A, 0xD9, 0x4F, 0x03, 0xEC, 0x4D, 0xCD,
                0xFF, 0x78, 0x53, 0x4E, 0xCF, 0x49,
            ],
            method: "DesignateAsRole".to_string(),
            parameters_count: 0,
            has_return_value: false,
            call_flags: 0x0F,
        }],
        ..nef("", "")
    };
    let info = identify_patterns(&nef, &[], None);
    assert_eq!(
        info.patterns,
        vec!["method_tokens", "native_contract_calls", "role_management"]
    );
}

#[test]
fn native_policy_method_tokens_report_policy_management() {
    let nef = NefFile {
        method_tokens: vec![crate::nef::MethodToken {
            hash: [
                0x7B, 0xC6, 0x81, 0xC0, 0xA1, 0xF7, 0x1D, 0x54, 0x34, 0x57, 0xB6, 0x8B, 0xBA, 0x8D,
                0x5F, 0x9F, 0xDD, 0x4E, 0x5E, 0xCC,
            ],
            method: "BlockAccount".to_string(),
            parameters_count: 0,
            has_return_value: false,
            call_flags: 0x0F,
        }],
        ..nef("", "")
    };
    let info = identify_patterns(&nef, &[], None);
    assert!(info.patterns.contains(&"policy_management".to_string()));
}

#[test]
fn native_method_tokens_report_fine_grained_behavior_patterns() {
    let cases = [
        (
            [
                0x1B, 0xF5, 0x75, 0xAB, 0x11, 0x89, 0x68, 0x84, 0x13, 0x61, 0x0A, 0x35, 0xA1, 0x28,
                0x86, 0xCD, 0xE0, 0xB6, 0x6C, 0x72,
            ],
            "Sha256",
            "cryptography",
        ),
        (
            [
                0xC0, 0xEF, 0x39, 0xCE, 0xE0, 0xE4, 0xE9, 0x25, 0xC6, 0xC2, 0xA0, 0x6A, 0x79, 0xE1,
                0x44, 0x0D, 0xD8, 0x6F, 0xCE, 0xAC,
            ],
            "JsonSerialize",
            "serialization",
        ),
        (
            [
                0xC0, 0xEF, 0x39, 0xCE, 0xE0, 0xE4, 0xE9, 0x25, 0xC6, 0xC2, 0xA0, 0x6A, 0x79, 0xE1,
                0x44, 0x0D, 0xD8, 0x6F, 0xCE, 0xAC,
            ],
            "StringSplit",
            "string_operations",
        ),
        (
            [
                0xBE, 0xF2, 0x04, 0x31, 0x40, 0x36, 0x2A, 0x77, 0xC1, 0x50, 0x99, 0xC7, 0xE6, 0x4C,
                0x12, 0xF7, 0x00, 0xB6, 0x65, 0xDA,
            ],
            "GetBlock",
            "blockchain_queries",
        ),
        (
            [
                0xF5, 0x63, 0xEA, 0x40, 0xBC, 0x28, 0x3D, 0x4D, 0x0E, 0x05, 0xC4, 0x8E, 0xA3, 0x05,
                0xB3, 0xF2, 0xA0, 0x73, 0x40, 0xEF,
            ],
            "Transfer",
            "native_token_calls",
        ),
        (
            [
                0xFD, 0xA3, 0xFA, 0x43, 0x46, 0xEA, 0x53, 0x2A, 0x25, 0x8F, 0xC4, 0x97, 0xDD, 0xAD,
                0xDB, 0x64, 0x37, 0xC9, 0xFD, 0xFF,
            ],
            "Deploy",
            "contract_lifecycle",
        ),
        (
            [
                0xFD, 0xA3, 0xFA, 0x43, 0x46, 0xEA, 0x53, 0x2A, 0x25, 0x8F, 0xC4, 0x97, 0xDD, 0xAD,
                0xDB, 0x64, 0x37, 0xC9, 0xFD, 0xFF,
            ],
            "GetContract",
            "contract_queries",
        ),
    ];

    for (hash, method, pattern) in cases {
        let nef = NefFile {
            method_tokens: vec![crate::nef::MethodToken {
                hash,
                method: method.to_string(),
                parameters_count: 0,
                has_return_value: false,
                call_flags: 0x0F,
            }],
            ..nef("", "")
        };
        let info = identify_patterns(&nef, &[], None);
        assert!(
            info.patterns.iter().any(|candidate| candidate == pattern),
            "{method} should identify {pattern}: {:?}",
            info.patterns
        );
        assert!(info.evidence.iter().any(|entry| {
            entry.source == "nef.method_tokens.pattern"
                && entry.value.contains(pattern)
                && entry.value.contains(method)
        }));
    }
}
