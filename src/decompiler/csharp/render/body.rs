use std::collections::BTreeMap;
use std::fmt::Write;

use crate::instruction::Instruction;

use super::super::super::high_level::HighLevelEmitter;
use super::super::helpers::csharpize_statement;

pub(super) struct LiftedBodyContext<'a> {
    pub(super) method_arg_counts_by_offset: &'a BTreeMap<usize, usize>,
    pub(super) call_targets_by_offset: &'a BTreeMap<usize, usize>,
    pub(super) calla_targets_by_offset: &'a BTreeMap<usize, usize>,
    pub(super) callt_labels: &'a [String],
    pub(super) callt_param_counts: &'a [usize],
    pub(super) callt_returns_value: &'a [bool],
}

pub(super) fn write_lifted_body(
    output: &mut String,
    instructions: &[Instruction],
    argument_labels: Option<&[String]>,
    warnings: &mut Vec<String>,
    context: &LiftedBodyContext<'_>,
) {
    let mut emitter = HighLevelEmitter::with_program(instructions);
    if let Some(labels) = argument_labels {
        emitter.set_argument_labels(labels);
    }
    emitter.set_callt_labels(context.callt_labels.to_vec());
    emitter.set_callt_param_counts(context.callt_param_counts.to_vec());
    emitter.set_callt_returns_value(context.callt_returns_value.to_vec());
    emitter.set_method_arg_counts_by_offset(context.method_arg_counts_by_offset);
    emitter.set_call_targets_by_offset(context.call_targets_by_offset);
    emitter.set_calla_targets_by_offset(context.calla_targets_by_offset);
    for instruction in instructions {
        emitter.advance_to(instruction.offset);
        emitter.emit_instruction(instruction);
    }
    let result = emitter.finish();
    warnings.extend(result.warnings);
    let statements = result.statements;
    if statements.is_empty() {
        writeln!(output, "            // no instructions decoded").unwrap();
        return;
    }

    let mut indent_level = 0usize;
    for line in statements {
        let converted = csharpize_statement(&line);
        let trimmed = converted.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with('}') {
            indent_level = indent_level.saturating_sub(1);
        }

        let indent = 12 + indent_level * 4;
        writeln!(output, "{:indent$}{}", "", trimmed, indent = indent).unwrap();

        if trimmed.ends_with('{') {
            indent_level += 1;
        }
    }
}
