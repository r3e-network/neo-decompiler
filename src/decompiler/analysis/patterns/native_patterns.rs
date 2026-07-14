use std::collections::BTreeSet;

use super::PatternEvidence;
use crate::native_contracts::NativeMethodHint;

pub(super) fn infer_native_patterns(
    hint: &NativeMethodHint,
    label: &str,
    patterns: &mut BTreeSet<String>,
    evidence: &mut Vec<PatternEvidence>,
) {
    match hint.contract {
        "OracleContract" => add(patterns, evidence, "oracle", label),
        "Governance" => add(patterns, evidence, "governance", label),
        "RoleManagement" => add(patterns, evidence, "role_management", label),
        "PolicyContract" => add(patterns, evidence, "policy_management", label),
        "TokenManagement" => add(patterns, evidence, "token_management", label),
        "LedgerContract" => {
            add(patterns, evidence, "ledger", label);
            add(patterns, evidence, "blockchain_queries", label);
        }
        "Notary" => add(patterns, evidence, "notary", label),
        "Treasury" => add(patterns, evidence, "treasury", label),
        "CryptoLib" => add(patterns, evidence, "cryptography", label),
        "StdLib" => infer_stdlib_patterns(hint.canonical_method, label, patterns, evidence),
        "GasToken" | "NeoToken" => add(patterns, evidence, "native_token_calls", label),
        "ContractManagement" => {
            add(patterns, evidence, "contract_management", label);
            match hint.canonical_method {
                Some("Deploy") | Some("Destroy") | Some("Update") => {
                    add(patterns, evidence, "contract_lifecycle", label);
                    if hint.canonical_method == Some("Update") {
                        add(patterns, evidence, "upgradeable", label);
                    }
                }
                Some("GetContract")
                | Some("GetContractById")
                | Some("GetContractHashes")
                | Some("HasMethod")
                | Some("IsContract") => {
                    add(patterns, evidence, "contract_queries", label);
                }
                _ => {}
            }
        }
        _ => {}
    }
}

fn infer_stdlib_patterns(
    method: Option<&str>,
    label: &str,
    patterns: &mut BTreeSet<String>,
    evidence: &mut Vec<PatternEvidence>,
) {
    match method {
        Some(
            "Base58CheckDecode" | "Base58CheckEncode" | "Base58Decode" | "Base58Encode"
            | "Base64Decode" | "Base64Encode" | "Base64UrlDecode" | "Base64UrlEncode"
            | "Deserialize" | "HexDecode" | "HexEncode" | "JsonDeserialize" | "JsonSerialize"
            | "Serialize",
        ) => add(patterns, evidence, "serialization", label),
        Some("Atoi" | "Itoa" | "StrLen" | "StringSplit") => {
            add(patterns, evidence, "string_operations", label);
        }
        Some("MemoryCompare" | "MemorySearch") => {
            add(patterns, evidence, "memory_operations", label);
        }
        _ => {}
    }
}

fn add(
    patterns: &mut BTreeSet<String>,
    evidence: &mut Vec<PatternEvidence>,
    pattern: &str,
    label: &str,
) {
    patterns.insert(pattern.to_string());
    evidence.push(PatternEvidence {
        source: "nef.method_tokens.pattern".to_string(),
        value: format!("{pattern}: {label}"),
    });
}
