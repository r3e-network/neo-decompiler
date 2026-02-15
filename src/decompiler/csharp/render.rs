//! C# skeleton renderer.
//!
//! The renderer produces a Neo SmartContract Framework-compatible skeleton
//! (methods, events, and manifest metadata) and optionally includes lifted
//! pseudo-bodies when method offsets are available.

use crate::instruction::Instruction;
use crate::manifest::ContractManifest;
use crate::native_contracts;
use crate::nef::NefFile;

use super::super::helpers::extract_contract_name;
use super::helpers::sanitize_csharp_identifier;

mod body;
mod events;
mod header;
mod methods;

pub(crate) struct CSharpRender {
    pub(crate) source: String,
    pub(crate) warnings: Vec<String>,
}

/// Render a C# skeleton with lifted bodies when possible.
pub(crate) fn render_csharp(
    nef: &NefFile,
    instructions: &[Instruction],
    manifest: Option<&ContractManifest>,
) -> CSharpRender {
    let mut output = String::new();
    let mut warnings = Vec::new();
    header::write_preamble(&mut output);

    let contract_name = extract_contract_name(manifest, sanitize_csharp_identifier);

    // Pre-resolve CALLT method-token labels.
    let callt_labels: Vec<String> = nef
        .method_tokens
        .iter()
        .map(|token| {
            if let Some(hint) = native_contracts::describe_method_token(&token.hash, &token.method)
            {
                hint.formatted_label(&token.method)
            } else {
                token.method.clone()
            }
        })
        .collect();

    header::write_contract_open(&mut output, &contract_name, nef, manifest);

    if let Some(manifest) = manifest {
        events::write_events(&mut output, manifest);
        methods::write_manifest_methods(
            &mut output,
            manifest,
            instructions,
            &callt_labels,
            &mut warnings,
        );
    } else {
        methods::write_fallback_entry(&mut output, instructions, &callt_labels, &mut warnings);
    }

    header::write_contract_close(&mut output);
    CSharpRender {
        source: output,
        warnings,
    }
}
