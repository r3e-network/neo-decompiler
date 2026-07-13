use std::collections::HashSet;
use std::fmt::Write;

use crate::instruction::Instruction;
use crate::manifest::{ContractManifest, ManifestMethod};

use super::super::super::helpers::{
    find_manifest_entry_method, next_inferred_method_offset, offset_as_usize,
};
use super::super::helpers::{
    escape_csharp_string, format_csharp_parameters, format_method_signature,
};
use super::structured::plan::CSharpMethodPlans;
use super::{body, CSharpCoverage};

pub(super) struct MethodsContext<'a> {
    pub(super) instructions: &'a [Instruction],
    pub(super) inferred_method_starts: &'a [usize],
    pub(super) method_plans: &'a CSharpMethodPlans,
    pub(super) body_context: body::LiftedBodyContext<'a>,
}

pub(super) fn write_manifest_methods(
    output: &mut String,
    manifest: &ContractManifest,
    context: &MethodsContext<'_>,
    warnings: &mut Vec<String>,
    coverage: &mut CSharpCoverage,
) {
    let entry_method = write_script_entry_if_needed(output, manifest, context, warnings, coverage);
    let entry_offset = context
        .instructions
        .first()
        .map(|ins| ins.offset)
        .unwrap_or(0);

    let mut sorted_methods: Vec<&ManifestMethod> = manifest.abi.methods.iter().collect();
    sorted_methods.sort_by_key(|m| m.offset.unwrap_or(i32::MAX));

    let (with_offsets, without_offsets): (Vec<_>, Vec<_>) =
        sorted_methods.into_iter().partition(|m| m.offset.is_some());
    let mut manifest_plan_index = 0usize;

    for method in with_offsets.iter() {
        let method_plan = context.method_plans.manifest_method(manifest_plan_index);
        manifest_plan_index += 1;
        let start = offset_as_usize(method.offset).unwrap_or(0);
        let end = next_inferred_method_offset(context.inferred_method_starts, start)
            .or_else(|| context.instructions.last().map(|i| i.offset + 1))
            .unwrap_or(start);
        let slice: Vec<Instruction> = context
            .instructions
            .iter()
            .filter(|ins| ins.offset >= start && ins.offset < end)
            .cloned()
            .collect();

        let param_signature = format_csharp_parameters(&method_plan.parameters);
        let signature = format_method_signature(
            &method_plan.emitted_name,
            &param_signature,
            &method_plan.return_type,
        );

        write_method_attributes(output, &method_plan.emitted_name, &method.name, method.safe);
        writeln!(output, "        {signature}").unwrap();
        writeln!(output, "        {{").unwrap();

        write_body(
            output,
            &slice,
            method_plan,
            warnings,
            coverage,
            &context.body_context,
        );

        writeln!(output, "        }}").unwrap();
        writeln!(output).unwrap();
    }

    for method in without_offsets {
        let method_plan = context.method_plans.manifest_method(manifest_plan_index);
        manifest_plan_index += 1;
        let param_signature = format_csharp_parameters(&method_plan.parameters);
        let signature = format_method_signature(
            &method_plan.emitted_name,
            &param_signature,
            &method_plan.return_type,
        );

        write_method_attributes(output, &method_plan.emitted_name, &method.name, method.safe);
        writeln!(output, "        {signature}").unwrap();
        writeln!(output, "        {{").unwrap();

        if entry_method
            .as_ref()
            .map(|(entry, _)| std::ptr::eq(*entry, method))
            .unwrap_or(false)
        {
            let end = next_inferred_method_offset(context.inferred_method_starts, entry_offset);
            let slice: Vec<Instruction> = match end {
                Some(end) => context
                    .instructions
                    .iter()
                    .filter(|ins| ins.offset >= entry_offset && ins.offset < end)
                    .cloned()
                    .collect(),
                None => context.instructions.to_vec(),
            };
            let slice = if slice.is_empty() {
                context.instructions.to_vec()
            } else {
                slice
            };
            write_body(
                output,
                &slice,
                method_plan,
                warnings,
                coverage,
                &context.body_context,
            );
        } else {
            write_body(
                output,
                &[],
                method_plan,
                warnings,
                coverage,
                &context.body_context,
            );
        }

        writeln!(output, "        }}").unwrap();
        writeln!(output).unwrap();
    }
}

pub(super) fn write_inferred_methods(
    output: &mut String,
    context: &MethodsContext<'_>,
    manifest: Option<&ContractManifest>,
    warnings: &mut Vec<String>,
    coverage: &mut CSharpCoverage,
) {
    let entry_offset = context.instructions.first().map(|ins| ins.offset);
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
        if Some(*start) == entry_offset || manifest_offsets.contains(start) {
            continue;
        }

        let end = next_inferred_method_offset(context.inferred_method_starts, *start)
            .or_else(|| context.instructions.last().map(|ins| ins.offset + 1))
            .unwrap_or(*start);
        let slice: Vec<Instruction> = context
            .instructions
            .iter()
            .filter(|ins| ins.offset >= *start && ins.offset < end)
            .cloned()
            .collect();
        if slice.is_empty()
            || slice
                .iter()
                .all(|ins| ins.opcode == crate::instruction::OpCode::Nop)
        {
            continue;
        }

        let method_plan = context
            .method_plans
            .inferred_method(*start)
            .expect("every emitted inferred method must have a precomputed plan");
        let params = format_csharp_parameters(&method_plan.parameters);

        // Each inferred helper is preceded by the trailing blank line
        // emitted by the previous method (the synthetic ScriptEntry from
        // `write_fallback_entry`, the last manifest method from
        // `write_manifest_methods`, or the previous iteration of this
        // loop). Emitting another blank line here would double-space the
        // separator.
        writeln!(
            output,
            "        private static {} {}({params})",
            method_plan.return_type, method_plan.emitted_name
        )
        .unwrap();
        writeln!(output, "        {{").unwrap();
        write_body(
            output,
            &slice,
            method_plan,
            warnings,
            coverage,
            &context.body_context,
        );
        writeln!(output, "        }}").unwrap();
        writeln!(output).unwrap();
    }
}

fn write_script_entry_if_needed<'a>(
    output: &mut String,
    manifest: &'a ContractManifest,
    context: &MethodsContext<'_>,
    warnings: &mut Vec<String>,
    coverage: &mut CSharpCoverage,
) -> Option<(&'a ManifestMethod, bool)> {
    let entry_offset = context.instructions.first().map(|ins| ins.offset)?;

    let entry_method = find_manifest_entry_method(manifest, entry_offset);
    if entry_method.is_some() {
        return entry_method;
    }
    let method_plan = context
        .method_plans
        .synthetic_entry()
        .expect("a manifest without a script-entry declaration needs a synthetic plan");

    let end = next_inferred_method_offset(context.inferred_method_starts, entry_offset);
    let slice: Vec<Instruction> = match end {
        Some(end) => context
            .instructions
            .iter()
            .filter(|ins| ins.offset >= entry_offset && ins.offset < end)
            .cloned()
            .collect(),
        None => context.instructions.to_vec(),
    };

    let slice = if slice.is_empty() {
        context.instructions.to_vec()
    } else {
        slice
    };

    writeln!(
        output,
        "        // warning: manifest entry offset did not match script entry at 0x{entry_offset:04X}; using synthetic ScriptEntry"
    )
    .unwrap();
    let parameters = format_csharp_parameters(&method_plan.parameters);
    let entry_signature = format_method_signature(
        &method_plan.emitted_name,
        &parameters,
        &method_plan.return_type,
    );
    writeln!(output, "        {entry_signature}").unwrap();
    writeln!(output, "        {{").unwrap();
    write_body(
        output,
        &slice,
        method_plan,
        warnings,
        coverage,
        &context.body_context,
    );
    writeln!(output, "        }}").unwrap();
    writeln!(output).unwrap();
    None
}

pub(super) fn write_fallback_entry(
    output: &mut String,
    context: &MethodsContext<'_>,
    warnings: &mut Vec<String>,
    coverage: &mut CSharpCoverage,
) {
    let entry_offset = context
        .instructions
        .first()
        .map(|ins| ins.offset)
        .unwrap_or(0);
    let end = next_inferred_method_offset(context.inferred_method_starts, entry_offset)
        .or_else(|| context.instructions.last().map(|i| i.offset + 1))
        .unwrap_or(entry_offset);
    let slice: Vec<Instruction> = context
        .instructions
        .iter()
        .filter(|ins| ins.offset >= entry_offset && ins.offset < end)
        .cloned()
        .collect();
    let slice = if slice.is_empty() {
        context.instructions.to_vec()
    } else {
        slice
    };

    let method_plan = context
        .method_plans
        .fallback_entry()
        .expect("manifest-free C# rendering needs a fallback entry plan");
    let parameters = format_csharp_parameters(&method_plan.parameters);
    let entry_signature = format_method_signature(
        &method_plan.emitted_name,
        &parameters,
        &method_plan.return_type,
    );
    writeln!(output, "        {entry_signature}").unwrap();
    writeln!(output, "        {{").unwrap();
    write_body(
        output,
        &slice,
        method_plan,
        warnings,
        coverage,
        &context.body_context,
    );
    writeln!(output, "        }}").unwrap();
    // Trailing blank line for consistency with manifest-driven
    // method emission (each manifest method ends with one); without
    // this the close-brace of a synthetic ScriptEntry sits flush
    // against the class close-brace.
    writeln!(output).unwrap();
}

fn write_body(
    output: &mut String,
    instructions: &[Instruction],
    method_plan: &super::structured::plan::CSharpMethodPlan,
    warnings: &mut Vec<String>,
    coverage: &mut CSharpCoverage,
    context: &body::LiftedBodyContext<'_>,
) {
    let result = body::render_method_body(instructions, method_plan, context);
    coverage.record(method_plan, &result);
    warnings.extend(result.warnings);
    output.push_str(&result.source);
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
