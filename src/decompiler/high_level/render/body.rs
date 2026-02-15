use std::fmt::Write;

use crate::instruction::Instruction;

use super::super::emitter::HighLevelEmitter;

pub(super) fn write_method_body(
    output: &mut String,
    instructions: &[Instruction],
    argument_labels: Option<&[String]>,
    inline_single_use_temps: bool,
    callt_labels: &[String],
    warnings: &mut Vec<String>,
) {
    let mut emitter = HighLevelEmitter::with_program(instructions);
    if let Some(labels) = argument_labels {
        emitter.set_argument_labels(labels);
    }
    emitter.set_inline_single_use_temps(inline_single_use_temps);
    emitter.set_callt_labels(callt_labels.to_vec());
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
