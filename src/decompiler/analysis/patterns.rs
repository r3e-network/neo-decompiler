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
use crate::nef::NefFile;

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
        infer_abi_patterns(&names, &mut standards, &mut patterns, &mut evidence);

        if manifest
            .abi
            .events
            .iter()
            .any(|event| event.name.eq_ignore_ascii_case("Transfer"))
        {
            evidence.push(PatternEvidence {
                source: "manifest.abi.events".to_string(),
                value: "Transfer".to_string(),
            });
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
    let language = compiler
        .as_deref()
        .and_then(infer_language)
        .or_else(|| infer_language_from_source(&nef.header.source));

    let confidence = if strong_standard {
        PatternConfidence::High
    } else if !patterns.is_empty() || language.is_some() {
        PatternConfidence::Medium
    } else {
        PatternConfidence::Unknown
    };

    info.standards = standards.into_iter().collect();
    info.patterns = patterns.into_iter().collect();
    info.language = language.map(str::to_string);
    info.compiler = compiler;
    info.confidence = confidence;
    info.evidence = evidence;
    info
}

fn infer_abi_patterns(
    names: &BTreeSet<String>,
    standards: &mut BTreeSet<String>,
    patterns: &mut BTreeSet<String>,
    evidence: &mut Vec<PatternEvidence>,
) {
    let nep17 = ["symbol", "decimals", "totalSupply", "balanceOf", "transfer"]
        .iter()
        .all(|name| names.contains(&name.to_ascii_lowercase()));
    let nep11 = ["ownerOf", "tokensOf", "transfer"]
        .iter()
        .all(|name| names.contains(&name.to_ascii_lowercase()));
    if nep17 {
        standards.insert("NEP-17".to_string());
        patterns.insert("NEP-17".to_string());
        evidence.push(PatternEvidence {
            source: "manifest.abi.methods".to_string(),
            value: "symbol,decimals,totalSupply,balanceOf,transfer".to_string(),
        });
    }
    if nep11 {
        standards.insert("NEP-11".to_string());
        patterns.insert("NEP-11".to_string());
        evidence.push(PatternEvidence {
            source: "manifest.abi.methods".to_string(),
            value: "ownerOf,tokensOf,transfer".to_string(),
        });
    }
}

fn infer_language(compiler: &str) -> Option<&'static str> {
    let compiler = compiler.to_ascii_lowercase();
    if compiler.contains("csharp") || compiler.contains("neo.compiler") {
        Some("C#")
    } else if compiler.contains("boa") || compiler.contains("python") {
        Some("Python")
    } else if compiler.contains("neogo") || compiler.contains("neo-go") {
        Some("Go")
    } else if compiler.contains("typescript") || compiler.contains("javascript") {
        Some("TypeScript/JavaScript")
    } else {
        None
    }
}

fn infer_language_from_source(source: &str) -> Option<&'static str> {
    let source = source.to_ascii_lowercase();
    if source.ends_with(".cs") {
        Some("C#")
    } else if source.ends_with(".py") {
        Some("Python")
    } else if source.ends_with(".go") {
        Some("Go")
    } else if source.ends_with(".ts") || source.ends_with(".js") {
        Some("TypeScript/JavaScript")
    } else {
        None
    }
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
        assert_eq!(info.confidence, PatternConfidence::Medium);
        assert!(info
            .evidence
            .iter()
            .any(|entry| entry.source == "nef.header.source"));
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
}
