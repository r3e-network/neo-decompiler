use std::collections::BTreeSet;

use super::PatternEvidence;

pub(super) fn infer_abi_patterns(
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
    let has_owner_accessor = names.contains("owner") || names.contains("getowner");
    let has_ownership_operation = names.contains("verify")
        || names.contains("setowner")
        || names.contains("transferownership");
    if has_owner_accessor && has_ownership_operation {
        patterns.insert("ownership".to_string());
        evidence.push(PatternEvidence {
            source: "manifest.abi.methods".to_string(),
            value: "owner,verify/transferOwnership".to_string(),
        });
    }
    if names.contains("mint") {
        patterns.insert("minting".to_string());
        evidence.push(PatternEvidence {
            source: "manifest.abi.methods".to_string(),
            value: "mint".to_string(),
        });
    }
    if names.contains("burn") {
        patterns.insert("burning".to_string());
        evidence.push(PatternEvidence {
            source: "manifest.abi.methods".to_string(),
            value: "burn".to_string(),
        });
    }
    if names.contains("pause") && names.contains("unpause") {
        patterns.insert("pausable".to_string());
        evidence.push(PatternEvidence {
            source: "manifest.abi.methods".to_string(),
            value: "pause,unpause".to_string(),
        });
    }
    if names.contains("royaltyinfo") {
        standards.insert("NEP-24".to_string());
        patterns.insert("NEP-24".to_string());
        patterns.insert("royalties".to_string());
        evidence.push(PatternEvidence {
            source: "manifest.abi.methods".to_string(),
            value: "royaltyInfo".to_string(),
        });
    }
    for (name, label) in [
        ("onnep17payment", "onNEP17Payment"),
        ("onnep11payment", "onNEP11Payment"),
    ] {
        if names.contains(name) {
            patterns.insert("token_receiver".to_string());
            evidence.push(PatternEvidence {
                source: "manifest.abi.methods".to_string(),
                value: label.to_string(),
            });
        }
    }
}
