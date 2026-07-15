//! NEF token and manifest-trust metadata comments for generated C#.

use std::fmt::Write;

use crate::native_contracts;
use crate::nef::{describe_call_flags, NefFile};
use crate::util;

pub(super) fn write_method_tokens_comment(output: &mut String, nef: &NefFile) {
    if nef.method_tokens.is_empty() {
        return;
    }
    writeln!(output, "        // method tokens declared in NEF:").unwrap();
    for token in &nef.method_tokens {
        let hint = native_contracts::describe_method_token(&token.hash, &token.method);
        let contract_note = hint
            .as_ref()
            .map(|h| format!(" ({})", h.formatted_label(&token.method)))
            .unwrap_or_default();
        writeln!(
            output,
            "        //   {}{} hash={} params={} returns={} flags=0x{:02X} ({})",
            token.method,
            contract_note,
            util::format_hash(&token.hash),
            token.parameters_count,
            token.has_return_value,
            token.call_flags,
            describe_call_flags(token.call_flags)
        )
        .unwrap();
        if let Some(hint) = hint {
            if !hint.has_exact_method() {
                writeln!(
                    output,
                    "        //   warning: native contract {} does not expose method {}",
                    hint.contract, token.method
                )
                .unwrap();
            }
        }
    }
}

pub(super) fn write_trusts_comment(output: &mut String, described: &str) {
    let stripped = described
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'));
    let entries = match stripped {
        Some(inner) if !inner.is_empty() => inner.split(", ").collect::<Vec<_>>(),
        _ => {
            writeln!(output, "        // trusts = {}", described).unwrap();
            return;
        }
    };
    if entries.len() <= 1 {
        writeln!(output, "        // trusts = {}", described).unwrap();
        return;
    }
    writeln!(output, "        // trusts:").unwrap();
    for entry in &entries {
        writeln!(output, "        //   {}", entry).unwrap();
    }
}
