use std::fmt::Write;

use crate::decompiler::analysis::patterns::{PatternConfidence, PatternInfo};
use crate::manifest::ContractManifest;
use crate::nef::NefFile;
use crate::util;

use super::super::super::helpers::format_permission_entry;
use super::super::helpers::escape_csharp_string;
use super::structured::plan::CSharpContractSymbols;

mod helpers;
mod metadata;

pub(super) use helpers::{
    write_assert_message_helper, write_bare_throw_helper, write_tagged_opcode_helpers,
    write_unpack_packstruct_helper, write_unresolved_call_helper, write_vm_exception_type,
};

pub(super) fn write_preamble(output: &mut String) {
    writeln!(output, "using System;").unwrap();
    writeln!(output, "using System.Numerics;").unwrap();
    writeln!(output, "using Neo.SmartContract.Framework;").unwrap();
    writeln!(output, "using Neo.SmartContract.Framework.Attributes;").unwrap();
    writeln!(output, "using Neo.SmartContract.Framework.Services;").unwrap();
    writeln!(output, "using Neo.SmartContract.Framework.Native;").unwrap();
    // The native catalog uses Neo's canonical contract labels while the
    // framework types retain the shorter CLR names for these contracts.
    writeln!(
        output,
        "using LedgerContract = Neo.SmartContract.Framework.Native.Ledger;"
    )
    .unwrap();
    writeln!(
        output,
        "using NeoToken = Neo.SmartContract.Framework.Native.NEO;"
    )
    .unwrap();
    writeln!(
        output,
        "using GasToken = Neo.SmartContract.Framework.Native.GAS;"
    )
    .unwrap();
    writeln!(
        output,
        "using OracleContract = Neo.SmartContract.Framework.Native.Oracle;"
    )
    .unwrap();
    writeln!(
        output,
        "using PolicyContract = Neo.SmartContract.Framework.Native.Policy;"
    )
    .unwrap();
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
        // Valid Neo N3 manifests carry an empty `features` object; only a
        // malformed manifest has content here, surfaced verbatim.
        if !manifest.features.is_empty() {
            writeln!(output, "        // features:").unwrap();
            for (key, value) in &manifest.features {
                writeln!(output, "        //   {key} = {value}").unwrap();
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
            metadata::write_trusts_comment(output, &trusts.describe());
        }
    } else {
        writeln!(output, "        // manifest not provided").unwrap();
    }

    metadata::write_method_tokens_comment(output, nef);

    writeln!(output).unwrap();
}

/// Render the inferred contract standards, behavior patterns, and source
/// language as comments so a generated C# file remains self-describing even
/// when it is viewed without the JSON analysis report.
pub(super) fn write_pattern_comments(output: &mut String, info: &PatternInfo) {
    let mut wrote = false;
    if !info.standards.is_empty() {
        let standards = info
            .standards
            .iter()
            .map(|value| escape_csharp_string(value))
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(output, "        // inferred standards: {standards}").unwrap();
        wrote = true;
    }
    if !info.patterns.is_empty() {
        let patterns = info
            .patterns
            .iter()
            .map(|value| escape_csharp_string(value))
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(output, "        // inferred patterns: {patterns}").unwrap();
        wrote = true;
    }
    if let Some(language) = info.language.as_deref() {
        writeln!(
            output,
            "        // inferred language: {}",
            escape_csharp_string(language)
        )
        .unwrap();
        wrote = true;
    }
    if info.confidence != PatternConfidence::Unknown {
        writeln!(
            output,
            "        // pattern confidence: {}",
            pattern_confidence_label(info.confidence)
        )
        .unwrap();
        wrote = true;
    }
    if wrote {
        writeln!(output).unwrap();
    }
}

fn pattern_confidence_label(confidence: PatternConfidence) -> &'static str {
    match confidence {
        PatternConfidence::High => "high",
        PatternConfidence::Medium => "medium",
        PatternConfidence::Low => "low",
        PatternConfidence::Unknown => "unknown",
    }
}

pub(super) fn write_static_fields(output: &mut String, symbols: &CSharpContractSymbols) {
    for field in &symbols.static_fields {
        writeln!(
            output,
            "        private static {} {};",
            field.csharp_type, field.name
        )
        .unwrap();
    }
    if !symbols.static_fields.is_empty() {
        writeln!(output).unwrap();
    }
}

pub(super) fn write_contract_close(output: &mut String) {
    writeln!(output, "    }}").unwrap();
    writeln!(output, "}}").unwrap();
}
