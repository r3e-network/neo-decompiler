use std::fmt::Write;

use crate::instruction::Instruction;
use crate::manifest::ContractManifest;

use super::super::super::helpers::{
    find_manifest_entry_method, format_manifest_parameters, format_manifest_type,
    next_method_offset, sanitize_identifier,
};
use super::body;

pub(super) fn write_entry_method(
    output: &mut String,
    instructions: &[Instruction],
    manifest: Option<&ContractManifest>,
    inline_single_use_temps: bool,
) -> Option<(String, Option<u32>)> {
    let entry_offset = instructions.first().map(|ins| ins.offset).unwrap_or(0);
    let entry_method = manifest.and_then(|m| find_manifest_entry_method(m, entry_offset));
    let entry_start = entry_method
        .as_ref()
        .and_then(|(method, _)| method.offset.map(|v| v as usize))
        .unwrap_or(entry_offset);
    let entry_end = entry_method
        .as_ref()
        .and_then(|(method, _)| manifest.and_then(|m| next_method_offset(m, method.offset)))
        .or_else(|| instructions.last().map(|ins| ins.offset + 1));
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
    let entry_param_labels = entry_method.as_ref().map(|(method, _)| {
        method
            .parameters
            .iter()
            .map(|param| sanitize_identifier(&param.name))
            .collect::<Vec<_>>()
    });
    let entry_name = entry_method
        .as_ref()
        .map(|(method, _)| sanitize_identifier(&method.name))
        .unwrap_or_else(|| "script_entry".to_string());
    let entry_params = entry_method
        .as_ref()
        .map(|(method, _)| format_manifest_parameters(&method.parameters))
        .unwrap_or_default();
    let entry_return = entry_method
        .as_ref()
        .map(|(method, _)| format_manifest_type(&method.return_type))
        .filter(|ty| ty != "void");
    let signature = match entry_return {
        Some(ret) => format!("fn {entry_name}({entry_params}) -> {ret}"),
        None => format!("fn {entry_name}({entry_params})"),
    };
    if let Some((method, matched)) = entry_method.as_ref() {
        if !matched {
            writeln!(
                output,
                "    // warning: manifest entry offset {} did not match script entry at 0x{:04X}; using first ABI method",
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
    );
    writeln!(output, "    }}").unwrap();

    entry_method
        .as_ref()
        .map(|(method, _)| (method.name.clone(), method.offset))
}
