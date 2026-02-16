use std::collections::BTreeMap;
use std::fmt::Write;

use crate::instruction::Instruction;

use super::super::emitter::HighLevelEmitter;

pub(super) struct MethodBodyContext<'a> {
    pub(super) method_labels_by_offset: &'a BTreeMap<usize, String>,
    pub(super) method_arg_counts_by_offset: &'a BTreeMap<usize, usize>,
    pub(super) call_targets_by_offset: &'a BTreeMap<usize, usize>,
    pub(super) calla_targets_by_offset: &'a BTreeMap<usize, usize>,
    pub(super) inline_single_use_temps: bool,
    pub(super) callt_labels: &'a [String],
    pub(super) callt_param_counts: &'a [usize],
    pub(super) callt_returns_value: &'a [bool],
}

pub(super) fn write_method_body(
    output: &mut String,
    instructions: &[Instruction],
    argument_labels: Option<&[String]>,
    warnings: &mut Vec<String>,
    context: &MethodBodyContext<'_>,
    returns_void: bool,
) {
    let mut emitter = HighLevelEmitter::with_program(instructions);
    if let Some(labels) = argument_labels {
        emitter.set_argument_labels(labels);
    }
    emitter.set_inline_single_use_temps(context.inline_single_use_temps);
    emitter.set_callt_labels(context.callt_labels.to_vec());
    emitter.set_callt_param_counts(context.callt_param_counts.to_vec());
    emitter.set_callt_returns_value(context.callt_returns_value.to_vec());
    emitter.set_method_labels_by_offset(context.method_labels_by_offset);
    emitter.set_method_arg_counts_by_offset(context.method_arg_counts_by_offset);
    emitter.set_call_targets_by_offset(context.call_targets_by_offset);
    emitter.set_calla_targets_by_offset(context.calla_targets_by_offset);
    emitter.set_returns_void(returns_void);
    for instruction in instructions {
        emitter.advance_to(instruction.offset);
        emitter.emit_instruction(instruction);
    }
    let result = emitter.finish();
    warnings.extend(result.warnings);
    let statements = result.statements;

    if statements.is_empty() {
        writeln!(output, "        // no instructions decoded").unwrap();
        return;
    }

    let mut indent_level = 0usize;
    for line in statements {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with('}') {
            indent_level = indent_level.saturating_sub(1);
        }

        let indent = 8 + indent_level * 4;
        writeln!(output, "{:indent$}{}", "", trimmed, indent = indent).unwrap();

        if trimmed.ends_with('{') {
            indent_level += 1;
        }
    }
}
