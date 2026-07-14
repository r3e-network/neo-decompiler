use std::collections::BTreeSet;

use super::PatternEvidence;

pub(super) fn infer_syscall_patterns(
    name: &str,
    patterns: &mut BTreeSet<String>,
    evidence: &mut Vec<PatternEvidence>,
) {
    if name.starts_with("System.Storage.") {
        add(patterns, evidence, "storage", name);
    }
    if matches!(name, "System.Storage.Get" | "System.Storage.Local.Get") {
        add(patterns, evidence, "storage_reads", name);
    }
    if matches!(name, "System.Storage.Put" | "System.Storage.Local.Put") {
        add(patterns, evidence, "storage_writes", name);
    }
    if matches!(
        name,
        "System.Storage.Delete" | "System.Storage.Local.Delete"
    ) {
        add(patterns, evidence, "storage_deletes", name);
    }
    if matches!(name, "System.Storage.Find" | "System.Storage.Local.Find") {
        add(patterns, evidence, "storage_iteration", name);
    }
    if matches!(name, "System.Iterator.Next" | "System.Iterator.Value") {
        add(patterns, evidence, "iterator_usage", name);
    }
    if matches!(name, "System.Runtime.Notify" | "System.Runtime.Log") {
        add(patterns, evidence, "notifications", name);
    }
    if matches!(
        name,
        "System.Crypto.CheckSig" | "System.Crypto.CheckMultisig"
    ) {
        patterns.insert("signature_verification".to_string());
        if name == "System.Crypto.CheckMultisig" {
            patterns.insert("multisig".to_string());
        }
        evidence.push(PatternEvidence {
            source: "syscall".to_string(),
            value: name.to_string(),
        });
    }
    if name == "System.Runtime.CheckWitness" {
        add(patterns, evidence, "authorization", name);
    }
    if name == "System.Runtime.GetCallingScriptHash" {
        add(patterns, evidence, "caller_context", name);
    }
    if name == "System.Runtime.CurrentSigners" {
        add(patterns, evidence, "signer_introspection", name);
    }
    if matches!(
        name,
        "System.Runtime.GetAddressVersion"
            | "System.Runtime.GetEntryScriptHash"
            | "System.Runtime.GetExecutingScriptHash"
            | "System.Runtime.GetInvocationCounter"
            | "System.Runtime.GetNetwork"
            | "System.Runtime.GetNotifications"
            | "System.Runtime.GetRandom"
            | "System.Runtime.GetScriptContainer"
            | "System.Runtime.GetTime"
            | "System.Runtime.GetTrigger"
            | "System.Runtime.Platform"
    ) {
        add(patterns, evidence, "runtime_context", name);
    }
    if matches!(
        name,
        "System.Contract.CreateMultisigAccount" | "System.Contract.CreateStandardAccount"
    ) {
        add(patterns, evidence, "account_creation", name);
    }
    if matches!(name, "System.Runtime.BurnGas" | "System.Runtime.GasLeft") {
        add(patterns, evidence, "gas_management", name);
    }
}

fn add(
    patterns: &mut BTreeSet<String>,
    evidence: &mut Vec<PatternEvidence>,
    pattern: &str,
    syscall: &str,
) {
    patterns.insert(pattern.to_string());
    evidence.push(PatternEvidence {
        source: "syscall".to_string(),
        value: syscall.to_string(),
    });
}
