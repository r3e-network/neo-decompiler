use std::fmt::Write;

use crate::instruction::Instruction;
use crate::manifest::{ContractManifest, ManifestMethod};

use super::super::super::helpers::{next_method_offset, sanitize_identifier};
use super::super::helpers::{
    collect_csharp_parameters, escape_csharp_string, format_csharp_parameters,
    format_manifest_type_csharp, format_method_signature,
};
use super::body;

pub(super) fn write_manifest_methods(
    output: &mut String,
    manifest: &ContractManifest,
    instructions: &[Instruction],
) {
    let mut sorted_methods: Vec<&ManifestMethod> = manifest.abi.methods.iter().collect();
    sorted_methods.sort_by_key(|m| m.offset.unwrap_or(u32::MAX));

    let (with_offsets, without_offsets): (Vec<_>, Vec<_>) =
        sorted_methods.into_iter().partition(|m| m.offset.is_some());

    for (idx, method) in with_offsets.iter().enumerate() {
        let start = method.offset.unwrap_or(0) as usize;
        let end = with_offsets
            .get(idx + 1)
            .and_then(|m| m.offset)
            .map(|v| v as usize)
            .unwrap_or_else(|| {
                next_method_offset(manifest, method.offset)
                    .unwrap_or_else(|| instructions.last().map(|i| i.offset + 1).unwrap_or(0))
            });
        let slice: Vec<Instruction> = instructions
            .iter()
            .filter(|ins| ins.offset >= start && ins.offset < end)
            .cloned()
            .collect();

        let params = collect_csharp_parameters(&method.parameters);
        let param_signature = format_csharp_parameters(&params);
        let method_name = sanitize_identifier(&method.name);
        let return_type = format_manifest_type_csharp(&method.return_type);
        let signature = format_method_signature(&method_name, &param_signature, &return_type);

        write_method_attributes(output, &method_name, &method.name, method.safe);
        writeln!(output, "        {signature}").unwrap();
        writeln!(output, "        {{").unwrap();

        if slice.is_empty() {
            writeln!(output, "            throw new NotImplementedException();").unwrap();
        } else {
            let labels: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
            body::write_lifted_body(output, &slice, Some(&labels));
        }

        writeln!(output, "        }}").unwrap();
        writeln!(output).unwrap();
    }

    for method in without_offsets {
        let params = collect_csharp_parameters(&method.parameters);
        let param_signature = format_csharp_parameters(&params);
        let method_name = sanitize_identifier(&method.name);
        let return_type = format_manifest_type_csharp(&method.return_type);
        let signature = format_method_signature(&method_name, &param_signature, &return_type);

        write_method_attributes(output, &method_name, &method.name, method.safe);
        writeln!(output, "        {signature}").unwrap();
        writeln!(output, "        {{").unwrap();
        writeln!(output, "            throw new NotImplementedException();").unwrap();
        writeln!(output, "        }}").unwrap();
        writeln!(output).unwrap();
    }
}

pub(super) fn write_fallback_entry(output: &mut String, instructions: &[Instruction]) {
    let entry_method_name = "ScriptEntry".to_string();
    let entry_signature = format_method_signature(&entry_method_name, "", "void");
    writeln!(output, "        {entry_signature}").unwrap();
    writeln!(output, "        {{").unwrap();
    body::write_lifted_body(output, instructions, None);
    writeln!(output, "        }}").unwrap();
}

fn write_method_attributes(output: &mut String, method_name: &str, raw_name: &str, is_safe: bool) {
    if method_name != raw_name {
        writeln!(
            output,
            "        [DisplayName(\"{}\")]",
            escape_csharp_string(raw_name)
        )
        .unwrap();
    }
    if is_safe {
        writeln!(output, "        [Safe]").unwrap();
    }
}
