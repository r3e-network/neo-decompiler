use std::fmt::Write;

use crate::manifest::ContractManifest;
use crate::nef::NefFile;
use crate::util;

use super::super::super::helpers::sanitize_identifier;
use super::{manifest_summary, method_tokens};

pub(super) fn write_contract_header(
    output: &mut String,
    nef: &NefFile,
    manifest: Option<&ContractManifest>,
) {
    let contract_name = manifest
        .and_then(|m| {
            let trimmed = m.name.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        })
        .map(sanitize_identifier)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "NeoContract".to_string());

    writeln!(output, "contract {contract_name} {{").unwrap();
    let script_hash = nef.script_hash();
    writeln!(
        output,
        "    // script hash (little-endian): {}",
        util::format_hash(&script_hash)
    )
    .unwrap();
    writeln!(
        output,
        "    // script hash (big-endian): {}",
        util::format_hash_be(&script_hash)
    )
    .unwrap();

    if let Some(manifest) = manifest {
        manifest_summary::write_manifest_summary(output, manifest);
    } else {
        writeln!(output, "    // manifest not provided").unwrap();
    }

    method_tokens::write_method_tokens(output, nef);

    writeln!(output).unwrap();
}
