use std::fmt::Write;

use crate::manifest::ContractManifest;
use crate::native_contracts;
use crate::nef::{describe_call_flags, NefFile};
use crate::util;

use super::super::super::helpers::format_permission_entry;
use super::super::helpers::escape_csharp_string;

pub(super) fn write_preamble(output: &mut String) {
    writeln!(output, "using System;").unwrap();
    writeln!(output, "using System.Numerics;").unwrap();
    writeln!(output, "using Neo.SmartContract.Framework;").unwrap();
    writeln!(output, "using Neo.SmartContract.Framework.Attributes;").unwrap();
    writeln!(output, "using Neo.SmartContract.Framework.Services;").unwrap();
    writeln!(output).unwrap();
}

pub(super) fn write_contract_open(
    output: &mut String,
    contract_name: &str,
    nef: &NefFile,
    manifest: Option<&ContractManifest>,
) {
    writeln!(output, "namespace NeoDecompiler.Generated {{").unwrap();
    if let Some(manifest) = manifest {
        // Emit all ManifestExtra fields from the extra object. Strings,
        // numbers, and booleans are stringified into the attribute
        // value; nested objects/arrays/null have no canonical short
        // form, so we drop them rather than emit ambiguous output. Same
        // policy the high-level renderer applies.
        if let Some(serde_json::Value::Object(map)) = manifest.extra.as_ref() {
            for (key, value) in map {
                if let Some(rendered) = crate::decompiler::helpers::render_extra_scalar(value) {
                    writeln!(
                        output,
                        "    [ManifestExtra(\"{}\", \"{}\")]",
                        escape_csharp_string(key),
                        escape_csharp_string(&rendered)
                    )
                    .unwrap();
                }
            }
        }
        // Emit SupportedStandards as a proper attribute.
        if !manifest.supported_standards.is_empty() {
            let standards = manifest
                .supported_standards
                .iter()
                .map(|s| format!("\"{}\"", escape_csharp_string(s)))
                .collect::<Vec<_>>()
                .join(", ");
            writeln!(output, "    [SupportedStandards({standards})]").unwrap();
        }
    }
    writeln!(output, "    public class {contract_name} : SmartContract").unwrap();
    writeln!(output, "    {{").unwrap();

    let script_hash = nef.script_hash();
    writeln!(
        output,
        "        // script hash (little-endian): {}",
        util::format_hash(&script_hash)
    )
    .unwrap();
    writeln!(
        output,
        "        // script hash (big-endian): {}",
        util::format_hash_be(&script_hash)
    )
    .unwrap();
    if !nef.header.compiler.is_empty() {
        writeln!(output, "        // compiler: {}", nef.header.compiler).unwrap();
    }
    if !nef.header.source.is_empty() {
        writeln!(output, "        // source: {}", nef.header.source).unwrap();
    }

    if let Some(manifest) = manifest {
        if manifest.features.storage || manifest.features.payable {
            writeln!(output, "        // features:").unwrap();
            if manifest.features.storage {
                writeln!(output, "        //   storage = true").unwrap();
            }
            if manifest.features.payable {
                writeln!(output, "        //   payable = true").unwrap();
            }
        }
        if !manifest.groups.is_empty() {
            // The `groups` field carries pubkey/signature pairs that
            // authorise signed updates of the contract. Neo
            // SmartContract Framework has no source-level attribute
            // for this — the pairs are set at deployment time — so
            // surface them as a comment block (parity with the
            // high-level renderer's `groups { pubkey=... }` block).
            // Show only the pubkey: the signature is opaque base64
            // and adds no human-readable value.
            writeln!(output, "        // groups:").unwrap();
            for group in &manifest.groups {
                writeln!(output, "        //   pubkey={}", group.pubkey).unwrap();
            }
        }
        if !manifest.permissions.is_empty() {
            writeln!(output, "        // permissions:").unwrap();
            for permission in &manifest.permissions {
                writeln!(
                    output,
                    "        //   {}",
                    format_permission_entry(permission)
                )
                .unwrap();
            }
        }
        if let Some(trusts) = manifest.trusts.as_ref() {
            write_trusts_comment(output, &trusts.describe());
        }
    } else {
        writeln!(output, "        // manifest not provided").unwrap();
    }

    write_method_tokens_comment(output, nef);

    writeln!(output).unwrap();
}

/// Render NEF method tokens as a header comment block (parity with the
/// high-level renderer's `// method tokens declared in NEF` block).
///
/// Method tokens are call-time metadata — every CALLT instruction in
/// the body resolves to one of these entries — so even though Neo
/// SmartContract Framework does not have a source-level declaration
/// for them, surfacing the table helps a reader cross-reference
/// `XYZ()` calls against their underlying native contract / call
/// flags. Mirrors the high-level layout: one header line, one entry
/// per token, plus a `// warning: ...` line if the entry's contract
/// hash is recognised but the named method is not.
fn write_method_tokens_comment(output: &mut String, nef: &NefFile) {
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

/// Render the manifest `trusts` value as a header comment.
///
/// Wildcard (`*`) and empty (`[]`) values render on one line — they
/// have no internal structure worth breaking out. The typed-list
/// form `[hash:0x..., group:02..., ...]` (produced by
/// `ManifestTrusts::describe()` when the manifest uses the
/// structured `{"hashes": [...], "groups": [...]}` shape) breaks
/// onto multiple lines so a contract with many trust entries reads
/// like the existing `// permissions:` block instead of stretching
/// off the right margin. Anything else (including raw JSON
/// fallback) renders verbatim on a single line.
fn write_trusts_comment(output: &mut String, described: &str) {
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

pub(super) fn write_contract_close(output: &mut String) {
    writeln!(output, "    }}").unwrap();
    writeln!(output, "}}").unwrap();
}
