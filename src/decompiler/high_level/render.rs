use crate::instruction::Instruction;
use crate::manifest::ContractManifest;
use crate::nef::NefFile;

use std::collections::HashSet;

mod body;
mod entry;
mod header;
mod manifest_summary;
mod method_tokens;
mod methods;

pub(crate) struct HighLevelRender {
    pub(crate) text: String,
    pub(crate) warnings: Vec<String>,
}

/// Render the high-level pseudo-contract view.
pub(crate) fn render_high_level(
    nef: &NefFile,
    instructions: &[Instruction],
    manifest: Option<&ContractManifest>,
    inline_single_use_temps: bool,
) -> HighLevelRender {
    use std::fmt::Write;

    let mut output = String::new();
    let mut warnings = Vec::new();
    let mut used_method_names = HashSet::new();
    header::write_contract_header(&mut output, nef, manifest);

    let entry_method = entry::write_entry_method(
        &mut output,
        instructions,
        manifest,
        inline_single_use_temps,
        &mut warnings,
        &mut used_method_names,
    );
    if let Some(manifest) = manifest {
        methods::write_manifest_methods(
            &mut output,
            instructions,
            manifest,
            entry_method.as_ref(),
            inline_single_use_temps,
            &mut warnings,
            &mut used_method_names,
        );
    }
    writeln!(output, "}}").unwrap();

    HighLevelRender {
        text: output,
        warnings,
    }
}
