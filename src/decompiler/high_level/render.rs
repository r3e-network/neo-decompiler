use crate::instruction::Instruction;
use crate::manifest::ContractManifest;
use crate::nef::NefFile;

mod body;
mod entry;
mod header;
mod manifest_summary;
mod method_tokens;
mod methods;

/// Render the high-level pseudo-contract view.
pub(crate) fn render_high_level(
    nef: &NefFile,
    instructions: &[Instruction],
    manifest: Option<&ContractManifest>,
) -> String {
    use std::fmt::Write;

    let mut output = String::new();
    header::write_contract_header(&mut output, nef, manifest);

    let entry_method = entry::write_entry_method(&mut output, instructions, manifest);
    if let Some(manifest) = manifest {
        methods::write_manifest_methods(&mut output, instructions, manifest, entry_method.as_ref());
    }
    writeln!(output, "}}").unwrap();

    output
}
