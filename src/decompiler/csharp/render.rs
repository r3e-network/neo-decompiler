//! C# skeleton renderer.
//!
//! The renderer produces a Neo SmartContract Framework-compatible skeleton
//! (methods, events, and manifest metadata) and optionally includes lifted
//! pseudo-bodies when method offsets are available.

use crate::instruction::Instruction;
use crate::manifest::ContractManifest;
use crate::nef::NefFile;

use super::super::helpers::sanitize_identifier;

mod body;
mod events;
mod header;
mod methods;

/// Render a C# skeleton with lifted bodies when possible.
pub(crate) fn render_csharp(
    nef: &NefFile,
    instructions: &[Instruction],
    manifest: Option<&ContractManifest>,
) -> String {
    let mut output = String::new();
    header::write_preamble(&mut output);

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

    header::write_contract_open(&mut output, &contract_name, nef, manifest);

    if let Some(manifest) = manifest {
        events::write_events(&mut output, manifest);
        methods::write_manifest_methods(&mut output, manifest, instructions);
    } else {
        methods::write_fallback_entry(&mut output, instructions);
    }

    header::write_contract_close(&mut output);
    output
}
