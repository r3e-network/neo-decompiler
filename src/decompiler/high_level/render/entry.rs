use std::fmt::Write;

use crate::instruction::Instruction;
use std::collections::HashSet;

use crate::manifest::ContractManifest;

use super::super::super::helpers::{
    find_manifest_entry_method, format_manifest_parameters, format_manifest_type,
    has_manifest_method_at_offset, make_unique_identifier, sanitize_identifier,
    sanitize_parameter_names,
};
use super::body;

pub(super) fn write_entry_method(
    output: &mut String,
    instructions: &[Instruction],
    manifest: Option<&ContractManifest>,
    inline_single_use_temps: bool,
    warnings: &mut Vec<String>,
    used_method_names: &mut HashSet<String>,
) -> Option<(String, Option<u32>)> {
    let entry_offset = instructions.first().map(|ins| ins.offset).unwrap_or(0);
    let entry_method = manifest.and_then(|m| find_manifest_entry_method(m, entry_offset));
    let use_manifest_entry = entry_method
        .as_ref()
        .map(|(_, matched)| *matched)
        .unwrap_or(false);

    let entry_start = entry_offset;
    let entry_end = manifest.and_then(|m| {
        m.abi
            .methods
            .iter()
            .filter_map(|method| method.offset.map(|value| value as usize))
            .filter(|offset| *offset > entry_start)
            .min()
    });

    let entry_instructions: Vec<Instruction> = match entry_end {
        Some(end) => instructions
            .iter()
            .filter(|ins| ins.offset >= entry_start && ins.offset < end)
            .cloned()
            .collect(),
        None => instructions.to_vec(),
    };
    let entry_instructions = if entry_instructions.is_empty() {
        instructions.to_vec()
    } else {
        entry_instructions
    };

    let entry_param_labels = if use_manifest_entry {
        entry_method
            .as_ref()
            .map(|(method, _)| sanitize_parameter_names(&method.parameters))
    } else {
        None
    };

    let entry_name = if use_manifest_entry {
        entry_method
            .as_ref()
            .map(|(method, _)| sanitize_identifier(&method.name))
            .unwrap_or_else(|| "script_entry".to_string())
    } else {
        "script_entry".to_string()
    };

    let entry_name = make_unique_identifier(entry_name, used_method_names);

    let entry_params = if use_manifest_entry {
        entry_method
            .as_ref()
            .map(|(method, _)| format_manifest_parameters(&method.parameters))
            .unwrap_or_default()
    } else {
        String::new()
    };

    let entry_return = if use_manifest_entry {
        entry_method
            .as_ref()
            .map(|(method, _)| format_manifest_type(&method.return_type))
            .filter(|ty| ty != "void")
    } else {
        None
    };

    let signature = match entry_return {
        Some(ret) => format!("fn {entry_name}({entry_params}) -> {ret}"),
        None => format!("fn {entry_name}({entry_params})"),
    };

    if !use_manifest_entry {
        if manifest
            .map(|m| has_manifest_method_at_offset(m, entry_offset))
            .unwrap_or(false)
        {
            writeln!(
                output,
                "    // warning: manifest method at script entry 0x{entry_offset:04X} was not selected; using synthetic script_entry"
            )
            .unwrap();
        } else if let Some((method, _)) = entry_method.as_ref() {
            writeln!(
                output,
                "    // warning: manifest entry offset {} did not match script entry at 0x{:04X}; using synthetic script_entry",
                method.offset.unwrap_or_default(),
                entry_offset
            )
            .unwrap();
        }
    }

    writeln!(output, "    {signature} {{").unwrap();

    body::write_method_body(
        output,
        &entry_instructions,
        entry_param_labels.as_deref(),
        inline_single_use_temps,
        warnings,
    );
    writeln!(output, "    }}").unwrap();

    if use_manifest_entry {
        entry_method
            .as_ref()
            .map(|(method, _)| (method.name.clone(), method.offset))
    } else {
        None
    }
}
