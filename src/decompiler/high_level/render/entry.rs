use std::collections::HashSet;
use std::fmt::Write;

use crate::instruction::Instruction;

use crate::manifest::ContractManifest;

use super::super::super::helpers::{
    find_manifest_entry_method, format_manifest_parameters, format_manifest_type,
    initslot_argument_count_at, make_unique_identifier, next_inferred_method_offset,
    sanitize_identifier, sanitize_parameter_names,
};
use super::body;

pub(super) fn write_entry_method(
    output: &mut String,
    instructions: &[Instruction],
    inferred_method_starts: &[usize],
    manifest: Option<&ContractManifest>,
    body_context: &body::MethodBodyContext<'_>,
    warnings: &mut Vec<String>,
    used_method_names: &mut HashSet<String>,
) -> Option<(String, Option<i32>)> {
    let entry_offset = instructions.first().map(|ins| ins.offset).unwrap_or(0);
    let entry_method = manifest.and_then(|m| find_manifest_entry_method(m, entry_offset));
    let use_manifest_entry = entry_method.is_some();

    let entry_start = entry_offset;
    let entry_end = next_inferred_method_offset(inferred_method_starts, entry_start);

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

    // For manifest-known entries we use the declared parameter names; for
    // synthesised script_entry we fall back to `arg0..argN` based on the
    // INITSLOT-inferred argument count. Only `INITSLOT`-declared args are
    // surfaced — purely stack-depth-inferred args (where the body consumes
    // values that no opcode pushed) stay anonymous so the
    // missing-argument warning remains visible to the reader. The JS port
    // applies the same rule.
    let entry_param_labels = if use_manifest_entry {
        entry_method
            .as_ref()
            .map(|(method, _)| sanitize_parameter_names(&method.parameters))
    } else {
        match initslot_argument_count_at(instructions, entry_offset) {
            Some(arg_count) if arg_count > 0 => {
                Some((0..arg_count).map(|index| format!("arg{index}")).collect())
            }
            _ => None,
        }
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
        // Reuse the same arg-label list as the body so the signature and
        // first lines (`let loc0 = arg0;`) reference matching identifiers.
        entry_param_labels
            .as_ref()
            .map(|labels| labels.join(", "))
            .unwrap_or_default()
    };

    let entry_return = if use_manifest_entry {
        entry_method
            .as_ref()
            .map(|(method, _)| format_manifest_type(&method.return_type))
            .filter(|ty| ty != "void")
    } else {
        None
    };

    let entry_is_void = use_manifest_entry && entry_return.is_none();

    let signature = match entry_return {
        Some(ret) => format!("fn {entry_name}({entry_params}) -> {ret}"),
        None => format!("fn {entry_name}({entry_params})"),
    };

    if !use_manifest_entry {
        if let Some(method) =
            manifest.and_then(|m| m.abi.methods.iter().find(|method| method.offset.is_some()))
        {
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
        warnings,
        body_context,
        entry_is_void,
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
