//! Conservative contract and source-language pattern identification.
//!
//! Manifest declarations are authoritative. Bytecode and ABI names are useful
//! hints, but they are reported with lower confidence and retained as evidence
//! so callers can distinguish detection from a guess.

use std::collections::BTreeSet;

use serde::Serialize;

use crate::instruction::{Instruction, OpCode, Operand};
use crate::manifest::ContractManifest;
use crate::manifest::{ManifestPermissionContract, ManifestPermissionMethods};
use crate::native_contracts;
use crate::nef::NefFile;

mod abi;
mod language;

/// Confidence assigned to an identified pattern or language hint.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PatternConfidence {
    /// Evidence directly declared by the manifest.
    High,
    /// Multiple independent ABI, bytecode, or metadata hints agree.
    Medium,
    /// A single weak hint is available.
    Low,
    /// No reliable pattern signal was found.
    #[default]
    Unknown,
}

/// A signal supporting one or more detected patterns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatternEvidence {
    /// Analysis surface that produced the signal.
    pub source: String,
    /// Human-readable signal value.
    pub value: String,
}

/// Best-effort contract standard and source-language summary.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatternInfo {
    /// Declared or inferred Neo standards, sorted and deduplicated.
    pub standards: Vec<String>,
    /// Contract behavior patterns such as `storage` or `notifications`.
    pub patterns: Vec<String>,
    /// Inferred source language, when compiler/source metadata supports it.
    pub language: Option<String>,
    /// Raw compiler identifier from the NEF header.
    pub compiler: Option<String>,
    /// Aggregate confidence for the summary.
    pub confidence: PatternConfidence,
    /// Signals retained for explainability.
    pub evidence: Vec<PatternEvidence>,
}

/// Identify standards, common contract shapes, and compiler language hints.
#[must_use]
pub fn identify_patterns(
    nef: &NefFile,
    instructions: &[Instruction],
    manifest: Option<&ContractManifest>,
) -> PatternInfo {
    let mut info = PatternInfo::default();
    let mut standards = BTreeSet::new();
    let mut patterns = BTreeSet::new();
    let mut evidence = Vec::new();
    let mut strong_standard = false;

    if let Some(manifest) = manifest {
        for standard in &manifest.supported_standards {
            let standard = standard.trim().to_uppercase();
            if standard.is_empty() {
                continue;
            }
            standards.insert(standard.clone());
            strong_standard = true;
            evidence.push(PatternEvidence {
                source: "manifest.supportedstandards".to_string(),
                value: standard.clone(),
            });
            if standard.starts_with("NEP-") {
                patterns.insert(standard);
            }
        }

        let names: BTreeSet<_> = manifest
            .abi
            .methods
            .iter()
            .map(|method| method.name.to_ascii_lowercase())
            .collect();
        abi::infer_abi_patterns(&names, &mut standards, &mut patterns, &mut evidence);

        if !manifest.abi.events.is_empty() {
            patterns.insert("events".to_string());
            evidence.push(PatternEvidence {
                source: "manifest.abi.events".to_string(),
                value: manifest.abi.events.len().to_string(),
            });
        }
        let has_transfer_event = manifest
            .abi
            .events
            .iter()
            .any(|event| event.name.eq_ignore_ascii_case("Transfer"));
        if has_transfer_event {
            evidence.push(PatternEvidence {
                source: "manifest.abi.events".to_string(),
                value: "Transfer".to_string(),
            });
            if names.contains("transfer") {
                patterns.insert("token_transfers".to_string());
                evidence.push(PatternEvidence {
                    source: "manifest.abi.methods".to_string(),
                    value: "transfer + Transfer".to_string(),
                });
            }
        }

        if !manifest.permissions.is_empty() {
            patterns.insert("call_permissions".to_string());
            evidence.push(PatternEvidence {
                source: "manifest.permissions".to_string(),
                value: manifest.permissions.len().to_string(),
            });
        }
        if manifest.permissions.iter().any(|permission| {
            matches!(
                &permission.contract,
                ManifestPermissionContract::Wildcard(_)
            ) || matches!(&permission.methods, ManifestPermissionMethods::Wildcard(_))
        }) {
            patterns.insert("wildcard_permissions".to_string());
            evidence.push(PatternEvidence {
                source: "manifest.permissions".to_string(),
                value: "wildcard".to_string(),
            });
        }
    }

    let syscall_names: BTreeSet<_> = instructions
        .iter()
        .filter_map(|instruction| match instruction.opcode {
            OpCode::Syscall => match instruction.operand {
                Some(Operand::Syscall(hash)) => crate::syscalls::lookup(hash).map(|info| info.name),
                _ => None,
            },
            _ => None,
        })
        .collect();
    if !nef.method_tokens.is_empty() {
        patterns.insert("method_tokens".to_string());
        evidence.push(PatternEvidence {
            source: "nef.method_tokens".to_string(),
            value: nef.method_tokens.len().to_string(),
        });
    }
    for token in &nef.method_tokens {
        if let Some(hint) = native_contracts::describe_method_token(&token.hash, &token.method)
            .filter(|hint| hint.has_exact_method())
        {
            patterns.insert("native_contract_calls".to_string());
            evidence.push(PatternEvidence {
                source: "nef.method_tokens.native".to_string(),
                value: hint.formatted_label(&token.method),
            });
            if hint.contract == "OracleContract" {
                patterns.insert("oracle".to_string());
            }
            if hint.contract == "ContractManagement" && hint.canonical_method == Some("Update") {
                patterns.insert("contract_management".to_string());
                patterns.insert("upgradeable".to_string());
            } else if hint.contract == "ContractManagement" {
                patterns.insert("contract_management".to_string());
            }
            if hint.contract == "Governance" {
                patterns.insert("governance".to_string());
            }
            if hint.contract == "RoleManagement" {
                patterns.insert("role_management".to_string());
            }
            match hint.contract {
                "PolicyContract" => {
                    patterns.insert("policy_management".to_string());
                }
                "TokenManagement" => {
                    patterns.insert("token_management".to_string());
                }
                "LedgerContract" => {
                    patterns.insert("ledger".to_string());
                }
                "Notary" => {
                    patterns.insert("notary".to_string());
                }
                "Treasury" => {
                    patterns.insert("treasury".to_string());
                }
                _ => {}
            }
        }
    }
    if instructions.iter().any(|instruction| {
        matches!(instruction.opcode, OpCode::CallA | OpCode::CallT)
            || matches!(
                instruction.operand,
                Some(Operand::Syscall(hash))
                    if crate::syscalls::lookup(hash)
                        .is_some_and(|info| info.name == "System.Contract.Call")
            )
    }) {
        patterns.insert("external_calls".to_string());
        evidence.push(PatternEvidence {
            source: "bytecode.calls".to_string(),
            value: "CALLA/CALLT/Contract.Call".to_string(),
        });
    }
    for name in syscall_names {
        if name.starts_with("System.Storage.") {
            patterns.insert("storage".to_string());
            evidence.push(PatternEvidence {
                source: "syscall".to_string(),
                value: name.to_string(),
            });
        }
        if name == "System.Runtime.Notify" || name == "System.Runtime.Log" {
            patterns.insert("notifications".to_string());
            evidence.push(PatternEvidence {
                source: "syscall".to_string(),
                value: name.to_string(),
            });
        }
        if name == "System.Crypto.CheckSig" || name == "System.Crypto.CheckMultisig" {
            patterns.insert("signature_verification".to_string());
            if name == "System.Crypto.CheckMultisig" {
                patterns.insert("multisig".to_string());
            }
            evidence.push(PatternEvidence {
                source: "syscall".to_string(),
                value: name.to_string(),
            });
        }
    }

    let compiler = (!nef.header.compiler.trim().is_empty()).then(|| nef.header.compiler.clone());
    if let Some(compiler) = compiler.as_deref() {
        evidence.push(PatternEvidence {
            source: "nef.header.compiler".to_string(),
            value: compiler.to_string(),
        });
    }
    if !nef.header.source.trim().is_empty() {
        evidence.push(PatternEvidence {
            source: "nef.header.source".to_string(),
            value: nef.header.source.clone(),
        });
    }
    let compiler_language = compiler.as_deref().and_then(language::infer_language);
    let language =
        compiler_language.or_else(|| language::infer_language_from_source(&nef.header.source));

    let confidence = if strong_standard {
        PatternConfidence::High
    } else if compiler_language.is_some()
        || patterns.contains("NEP-17")
        || patterns.contains("NEP-11")
        || (evidence.len() >= 2 && !patterns.is_empty())
    {
        PatternConfidence::Medium
    } else if !evidence.is_empty() {
        PatternConfidence::Low
    } else {
        PatternConfidence::Unknown
    };

    info.standards = standards.into_iter().collect();
    info.patterns = patterns.into_iter().collect();
    info.language = language.map(str::to_string);
    info.compiler = compiler;
    info.confidence = confidence;
    evidence.sort_by(|left, right| {
        left.source
            .cmp(&right.source)
            .then_with(|| left.value.cmp(&right.value))
    });
    info.evidence = evidence;
    info
}

#[cfg(test)]
mod tests {
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
        assert_eq!(info.language.as_deref(), Some("Python"));
        assert_eq!(info.confidence, PatternConfidence::Low);
        assert!(info
            .evidence
            .iter()
            .any(|entry| entry.source == "nef.header.source"));
    }

    #[test]
    fn source_paths_and_uri_suffixes_still_infer_language() {
        for (source, expected) in [
            (r"C:\contracts\Token.cs", "C#"),
            ("/contracts/Token.csproj", "C#"),
            ("/contracts/Token.py?build=42", "Python"),
            ("src/token.go#source", "Go"),
            ("src/token.rs#source", "Rust"),
            ("src/token.java#source", "Java"),
            ("src/token.tsx?source=embedded", "TypeScript/JavaScript"),
            ("src/token.jsx#source", "TypeScript/JavaScript"),
        ] {
            let info = identify_patterns(&nef("", source), &[], None);
            assert_eq!(info.language.as_deref(), Some(expected));
        }
    }

    #[test]
    fn rust_compiler_metadata_infers_rust_language() {
        let info = identify_patterns(&nef("neo-rustc 1", ""), &[], None);
        assert_eq!(info.language.as_deref(), Some("Rust"));
        assert_eq!(info.confidence, PatternConfidence::Medium);
    }

    #[test]
    fn java_compiler_metadata_infers_java_language() {
        let info = identify_patterns(&nef("neo-java-compiler 1", ""), &[], None);
        assert_eq!(info.language.as_deref(), Some("Java"));
        assert_eq!(info.confidence, PatternConfidence::Medium);
    }

    #[test]
    fn javascript_compiler_metadata_precedes_java_substring() {
        let info = identify_patterns(&nef("neo-javascript-compiler 1", ""), &[], None);
        assert_eq!(info.language.as_deref(), Some("TypeScript/JavaScript"));
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
        assert!(info.evidence.iter().any(|entry| {
            entry.source == "manifest.abi.methods" && entry.value == "pause,unpause"
        }));
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
        assert!(info.evidence.iter().any(|entry| {
            entry.source == "manifest.abi.methods" && entry.value == "royaltyInfo"
        }));
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
                    0x58, 0x87, 0x17, 0x11, 0x7E, 0x0A, 0xA8, 0x10, 0x72, 0xAF, 0xAB, 0x71, 0xD2,
                    0xDD, 0x89, 0xFE, 0x7C, 0x4B, 0x92, 0xFE,
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
                    0xFD, 0xA3, 0xFA, 0x43, 0x46, 0xEA, 0x53, 0x2A, 0x25, 0x8F, 0xC4, 0x97, 0xDD,
                    0xAD, 0xDB, 0x64, 0x37, 0xC9, 0xFD, 0xFF,
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
                    0xE2, 0x95, 0xE3, 0x91, 0x54, 0x4C, 0x17, 0x8A, 0xD9, 0x4F, 0x03, 0xEC, 0x4D,
                    0xCD, 0xFF, 0x78, 0x53, 0x4E, 0xCF, 0x49,
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
                    0x7B, 0xC6, 0x81, 0xC0, 0xA1, 0xF7, 0x1D, 0x54, 0x34, 0x57, 0xB6, 0x8B, 0xBA,
                    0x8D, 0x5F, 0x9F, 0xDD, 0x4E, 0x5E, 0xCC,
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
}
