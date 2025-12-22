use std::fmt::Write;

use crate::instruction::Instruction;

use super::super::super::high_level::HighLevelEmitter;
use super::super::helpers::csharpize_statement;

pub(super) fn write_lifted_body(
    output: &mut String,
    instructions: &[Instruction],
    argument_labels: Option<&[String]>,
    warnings: &mut Vec<String>,
) {
    let mut emitter = HighLevelEmitter::with_program(instructions);
    if let Some(labels) = argument_labels {
        emitter.set_argument_labels(labels);
    }
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
