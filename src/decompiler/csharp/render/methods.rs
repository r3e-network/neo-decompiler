use std::collections::HashSet;
use std::fmt::Write;

use crate::instruction::Instruction;
use crate::manifest::{ContractManifest, ManifestMethod};

use super::super::super::helpers::{has_manifest_method_at_offset, next_method_offset, offset_as_usize};
use super::super::helpers::{
    collect_csharp_parameters, escape_csharp_string, format_csharp_parameters,
    format_manifest_type_csharp, format_method_signature, sanitize_csharp_identifier,
};
use super::body;

pub(super) fn write_manifest_methods(
    output: &mut String,
    manifest: &ContractManifest,
    instructions: &[Instruction],
    warnings: &mut Vec<String>,
) {
    write_script_entry_if_needed(output, manifest, instructions, warnings);

    let mut used_signatures: HashSet<(String, String)> = HashSet::new();
    let mut sorted_methods: Vec<&ManifestMethod> = manifest.abi.methods.iter().collect();
    sorted_methods.sort_by_key(|m| m.offset.unwrap_or(i32::MAX));

    let (with_offsets, without_offsets): (Vec<_>, Vec<_>) =
        sorted_methods.into_iter().partition(|m| m.offset.is_some());

    for (idx, method) in with_offsets.iter().enumerate() {
        let start = offset_as_usize(method.offset).unwrap_or(0);
        let end = with_offsets
            .get(idx + 1)
            .and_then(|m| offset_as_usize(m.offset))
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
        let signature_key = params
            .iter()
            .map(|param| param.ty.as_str())
            .collect::<Vec<_>>()
            .join(",");
        let param_signature = format_csharp_parameters(&params);
        let base_name = sanitize_csharp_identifier(&method.name);
        let method_name = make_unique_method_name(base_name, &signature_key, &mut used_signatures);
        let return_type = format_manifest_type_csharp(&method.return_type);
        let signature = format_method_signature(&method_name, &param_signature, &return_type);

        write_method_attributes(output, &method_name, &method.name, method.safe);
        writeln!(output, "        {signature}").unwrap();
        writeln!(output, "        {{").unwrap();

        if slice.is_empty() {
            writeln!(output, "            throw new NotImplementedException();").unwrap();
        } else {
            let labels: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
            body::write_lifted_body(output, &slice, Some(&labels), warnings);
        }

        writeln!(output, "        }}").unwrap();
        writeln!(output).unwrap();
    }

    for method in without_offsets {
        let params = collect_csharp_parameters(&method.parameters);
        let signature_key = params
            .iter()
            .map(|param| param.ty.as_str())
            .collect::<Vec<_>>()
            .join(",");
        let param_signature = format_csharp_parameters(&params);
        let base_name = sanitize_csharp_identifier(&method.name);
        let method_name = make_unique_method_name(base_name, &signature_key, &mut used_signatures);
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

fn write_script_entry_if_needed(
    output: &mut String,
    manifest: &ContractManifest,
    instructions: &[Instruction],
    warnings: &mut Vec<String>,
) {
    let Some(entry_offset) = instructions.first().map(|ins| ins.offset) else {
        return;
    };

    if has_manifest_method_at_offset(manifest, entry_offset) {
        return;
    }

    let end = manifest
        .abi
        .methods
        .iter()
        .filter_map(|method| offset_as_usize(method.offset))
        .filter(|offset| *offset > entry_offset)
        .min();
    let slice: Vec<Instruction> = match end {
        Some(end) => instructions
            .iter()
            .filter(|ins| ins.offset >= entry_offset && ins.offset < end)
            .cloned()
            .collect(),
        None => instructions.to_vec(),
    };

    let slice = if slice.is_empty() {
        instructions.to_vec()
    } else {
        slice
    };

    writeln!(
        output,
        "        // warning: manifest entry offset did not match script entry at 0x{entry_offset:04X}; using synthetic ScriptEntry"
    )
    .unwrap();
    let entry_signature = format_method_signature("ScriptEntry", "", "void");
    writeln!(output, "        {entry_signature}").unwrap();
    writeln!(output, "        {{").unwrap();
    body::write_lifted_body(output, &slice, None, warnings);
    writeln!(output, "        }}").unwrap();
    writeln!(output).unwrap();
}

pub(super) fn write_fallback_entry(
    output: &mut String,
    instructions: &[Instruction],
    warnings: &mut Vec<String>,
) {
    let entry_method_name = "ScriptEntry".to_string();
    let entry_signature = format_method_signature(&entry_method_name, "", "void");
    writeln!(output, "        {entry_signature}").unwrap();
    writeln!(output, "        {{").unwrap();
    body::write_lifted_body(output, instructions, None, warnings);
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

fn make_unique_method_name(
    base: String,
    signature: &str,
    used: &mut HashSet<(String, String)>,
) -> String {
    let mut candidate = base.clone();
    let mut index = 1usize;
    while !used.insert((candidate.clone(), signature.to_string())) {
        candidate = format!("{base}_{index}");
        index += 1;
    }
    candidate
}
