use std::collections::HashSet;
use std::fmt::Write;

use crate::instruction::Instruction;
use crate::manifest::{ContractManifest, ManifestMethod};

use super::super::super::helpers::{
    format_manifest_parameters, format_manifest_type, make_unique_identifier,
    next_inferred_method_offset, offset_as_usize, sanitize_identifier, sanitize_parameter_names,
};
use super::body;

pub(super) struct MethodsContext<'a> {
    pub(super) instructions: &'a [Instruction],
    pub(super) inferred_method_starts: &'a [usize],
    pub(super) body_context: &'a body::MethodBodyContext<'a>,
}

pub(super) fn write_manifest_methods(
    output: &mut String,
    context: &MethodsContext<'_>,
    manifest: &ContractManifest,
    entry_method: Option<&(String, Option<i32>)>,
    warnings: &mut Vec<String>,
    used_method_names: &mut HashSet<String>,
) {
    let mut methods: Vec<&ManifestMethod> = manifest.abi.methods.iter().collect();
    methods.sort_by_key(|m| m.offset.unwrap_or(i32::MAX));

    for method in methods.iter() {
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
            format!("fn {}({}) -> {}", method_name, params, return_ty)
        };

        writeln!(output).unwrap();
        writeln!(output, "    {signature} {{").unwrap();

        if let Some(start) = offset_as_usize(method.offset) {
            let end = next_inferred_method_offset(context.inferred_method_starts, start)
                .or_else(|| context.instructions.last().map(|i| i.offset + 1))
                .unwrap_or(start);

            // Instructions are sorted by offset — use partition_point
            // to locate the sub-slice without cloning.
            let lo = context
                .instructions
                .partition_point(|ins| ins.offset < start);
            let hi = context.instructions.partition_point(|ins| ins.offset < end);
            let slice = &context.instructions[lo..hi];

            if slice.is_empty() {
                writeln!(
                    output,
                    "        // no instructions decoded for manifest method at offset 0x{start:04X}"
                )
                .unwrap();
            } else {
                let labels = sanitize_parameter_names(&method.parameters);
                let is_void = method.return_type == "Void";
                body::write_method_body(
                    output,
                    slice,
                    Some(&labels),
                    warnings,
                    context.body_context,
                    is_void,
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

pub(super) fn write_inferred_methods(
    output: &mut String,
    context: &MethodsContext<'_>,
    manifest: Option<&ContractManifest>,
    entry_method: Option<&(String, Option<i32>)>,
    warnings: &mut Vec<String>,
    used_method_names: &mut HashSet<String>,
) {
    let entry_offset = context.instructions.first().map(|ins| ins.offset);
    let entry_manifest_offset =
        entry_method.and_then(|(_, offset)| offset.and_then(|value| usize::try_from(value).ok()));
    let manifest_offsets: HashSet<usize> = manifest
        .map(|manifest| {
            manifest
                .abi
                .methods
                .iter()
                .filter_map(|method| offset_as_usize(method.offset))
                .collect()
        })
        .unwrap_or_default();

    for start in context.inferred_method_starts {
        if Some(*start) == entry_offset
            || Some(*start) == entry_manifest_offset
            || manifest_offsets.contains(start)
        {
            continue;
        }

        let end = next_inferred_method_offset(context.inferred_method_starts, *start)
            .or_else(|| context.instructions.last().map(|ins| ins.offset + 1))
            .unwrap_or(*start);
        let lo = context
            .instructions
            .partition_point(|ins| ins.offset < *start);
        let hi = context.instructions.partition_point(|ins| ins.offset < end);
        let slice = &context.instructions[lo..hi];
        if slice.is_empty() {
            continue;
        }

        let base_name = format!("sub_0x{start:04X}");
        let method_name = make_unique_identifier(base_name, used_method_names);
        let arg_count = context
            .body_context
            .method_arg_counts_by_offset
            .get(start)
            .copied()
            .unwrap_or(0);
        let argument_labels = (0..arg_count)
            .map(|index| format!("arg{index}"))
            .collect::<Vec<_>>();
        let argument_signature = argument_labels.join(", ");

        writeln!(output).unwrap();
        writeln!(output, "    fn {method_name}({argument_signature}) {{").unwrap();
        body::write_method_body(
            output,
            slice,
            (!argument_labels.is_empty()).then_some(argument_labels.as_slice()),
            warnings,
            context.body_context,
            false, // inferred methods: return type unknown
        );
        writeln!(output, "    }}").unwrap();
    }
}
