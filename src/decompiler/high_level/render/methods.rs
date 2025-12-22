use std::collections::HashSet;
use std::fmt::Write;

use crate::instruction::Instruction;
use crate::manifest::{ContractManifest, ManifestMethod};

use super::super::super::helpers::{
    format_manifest_parameters, format_manifest_type, make_unique_identifier, next_method_offset,
    sanitize_identifier, sanitize_parameter_names,
};
use super::body;

pub(super) fn write_manifest_methods(
    output: &mut String,
    instructions: &[Instruction],
    manifest: &ContractManifest,
    entry_method: Option<&(String, Option<u32>)>,
    inline_single_use_temps: bool,
    warnings: &mut Vec<String>,
    used_method_names: &mut HashSet<String>,
) {
    let mut methods: Vec<&ManifestMethod> = manifest.abi.methods.iter().collect();
    methods.sort_by_key(|m| m.offset.unwrap_or(u32::MAX));

    for (idx, method) in methods.iter().enumerate() {
        if entry_method
            .map(|(name, offset)| name == &method.name && offset == &method.offset)
            .unwrap_or(false)
        {
            continue;
        }

        let params = format_manifest_parameters(&method.parameters);
        let return_ty = format_manifest_type(&method.return_type);
        let method_name =
            make_unique_identifier(sanitize_identifier(&method.name), used_method_names);
        let signature = if return_ty == "void" {
            format!("fn {}({})", method_name, params)
        } else {
            format!(
                "fn {}({}) -> {}",
                method_name,
                params,
                return_ty
            )
        };

        writeln!(output).unwrap();
        writeln!(output, "    {signature} {{").unwrap();

        if let Some(offset) = method.offset {
            let start = offset as usize;
            let end = methods
                .get(idx + 1)
                .and_then(|m| m.offset)
                .map(|v| v as usize)
                .or_else(|| next_method_offset(manifest, method.offset));
            let end = end
                .or_else(|| instructions.last().map(|i| i.offset + 1))
                .unwrap_or(start);

            let slice: Vec<Instruction> = instructions
                .iter()
                .filter(|ins| ins.offset >= start && ins.offset < end)
                .cloned()
                .collect();

            if slice.is_empty() {
                writeln!(
                    output,
                    "        // no instructions decoded for manifest method at offset 0x{start:04X}"
                )
                .unwrap();
            } else {
                let labels = sanitize_parameter_names(&method.parameters);
                body::write_method_body(
                    output,
                    &slice,
                    Some(&labels),
                    inline_single_use_temps,
                    warnings,
                );
            }
        } else {
            writeln!(
                output,
                "        // manifest did not provide an offset; body not decompiled"
            )
            .unwrap();
        }

        writeln!(output, "    }}").unwrap();
    }
}
