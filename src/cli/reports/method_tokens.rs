use serde::Serialize;

use crate::native_contracts;
use crate::nef::{call_flag_labels, describe_call_flags, MethodToken};
use crate::util;

pub(in crate::cli) fn format_method_token_line(index: usize, token: &MethodToken) -> String {
    let report = build_method_token_report(token);
    let contract_label = report
        .native_contract
        .as_ref()
        .map(|entry| format!(" ({})", entry.label))
        .unwrap_or_default();
    let warning = report
        .warning
        .as_ref()
        .map(|w| format!(" // warning: {w}"))
        .unwrap_or_default();
    format!(
        "#{index}: hash={}{} method={} params={} returns={} flags=0x{:02X} ({}){}",
        util::format_hash(&token.hash),
        contract_label,
        token.method,
        token.parameters_count,
        token.has_return_value,
        token.call_flags,
        describe_call_flags(token.call_flags),
        warning
    )
}

pub(in crate::cli) fn build_method_token_report(token: &MethodToken) -> MethodTokenReport {
    let hint = native_contracts::describe_method_token(&token.hash, &token.method);
    let warning = hint.as_ref().and_then(|h| {
        if h.has_exact_method() {
            None
        } else {
            Some(format!(
                "native contract {} does not expose method {}",
                h.contract, token.method
            ))
        }
    });
    let native_contract = hint.as_ref().map(|h| NativeContractReport {
        contract: h.contract.to_string(),
        method: h.canonical_method.map(ToString::to_string),
        label: h.formatted_label(&token.method),
    });

    MethodTokenReport {
        method: token.method.clone(),
        hash_le: util::format_hash(&token.hash),
        hash_be: util::format_hash_be(&token.hash),
        parameters: token.parameters_count,
        returns: token.has_return_value,
        call_flags: token.call_flags,
        call_flag_labels: call_flag_labels(token.call_flags),
        returns_value: token.has_return_value,
        native_contract,
        warning,
    }
}

pub(in crate::cli) fn collect_warnings(tokens: &[MethodTokenReport]) -> Vec<String> {
    tokens
        .iter()
        .filter_map(|report| report.warning.as_ref().map(|w| w.to_string()))
        .collect()
}

#[derive(Serialize)]
pub(in crate::cli) struct MethodTokenReport {
    method: String,
    hash_le: String,
    hash_be: String,
    parameters: u16,
    returns: bool,
    call_flags: u8,
    call_flag_labels: Vec<&'static str>,
    returns_value: bool,
    native_contract: Option<NativeContractReport>,
    warning: Option<String>,
}

#[derive(Serialize)]
pub(in crate::cli) struct NativeContractReport {
    contract: String,
    method: Option<String>,
    label: String,
}
