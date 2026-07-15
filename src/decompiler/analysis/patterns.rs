//! Conservative contract and C# target metadata identification.
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
mod native_patterns;
mod syscall_patterns;

/// Confidence assigned to an identified pattern or C# target hint.
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

/// Best-effort contract standard and C# target summary.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatternInfo {
    /// Declared or inferred Neo standards, sorted and deduplicated.
    pub standards: Vec<String>,
    /// Contract behavior patterns such as `storage`, `storage_writes`, or
    /// `notifications`.
    pub patterns: Vec<String>,
    /// Inferred source target. This is currently limited to C# because C# is
    /// the only generated source backend.
    pub language: Option<String>,
    /// Raw compiler identifier from the NEF header.
    pub compiler: Option<String>,
    /// Aggregate confidence for the summary.
    pub confidence: PatternConfidence,
    /// Signals retained for explainability.
    pub evidence: Vec<PatternEvidence>,
}

/// Identify standards, common contract shapes, and C# compiler hints.
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
            let label = hint.formatted_label(&token.method);
            evidence.push(PatternEvidence {
                source: "nef.method_tokens.native".to_string(),
                value: label.clone(),
            });
            native_patterns::infer_native_patterns(&hint, &label, &mut patterns, &mut evidence);
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
        syscall_patterns::infer_syscall_patterns(name, &mut patterns, &mut evidence);
    }

    // Backward relative jumps are a reliable structural signal for recovered
    // loop / iteration shapes (while/for/do-while), independent of rendering.
    if instructions.iter().any(|instruction| {
        matches!(
            instruction.opcode,
            OpCode::Jmp
                | OpCode::Jmpif
                | OpCode::Jmpifnot
                | OpCode::JmpEq
                | OpCode::JmpNe
                | OpCode::JmpLt
                | OpCode::JmpLe
                | OpCode::JmpGt
                | OpCode::JmpGe
                | OpCode::Jmp_L
                | OpCode::Jmpif_L
                | OpCode::Jmpifnot_L
                | OpCode::JmpEq_L
                | OpCode::JmpNe_L
                | OpCode::JmpLt_L
                | OpCode::JmpLe_L
                | OpCode::JmpGt_L
                | OpCode::JmpGe_L
        ) && match &instruction.operand {
            Some(Operand::Jump(delta)) => *delta < 0,
            Some(Operand::Jump32(delta)) => *delta < 0,
            _ => false,
        }
    }) {
        patterns.insert("loops".to_string());
        evidence.push(PatternEvidence {
            source: "bytecode.control_flow".to_string(),
            value: "backward jump".to_string(),
        });
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
#[path = "patterns/tests.rs"]
mod tests;
