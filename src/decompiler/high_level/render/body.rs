use std::fmt::Write;

use crate::instruction::Instruction;

use super::super::emitter::HighLevelEmitter;

pub(super) fn write_method_body(
    output: &mut String,
    instructions: &[Instruction],
    argument_labels: Option<&[String]>,
) {
    let mut emitter = HighLevelEmitter::with_program(instructions);
    if let Some(labels) = argument_labels {
        emitter.set_argument_labels(labels);
    }
    for instruction in instructions {
        emitter.advance_to(instruction.offset);
        emitter.emit_instruction(instruction);
    }
    let statements = emitter.finish();

    if statements.is_empty() {
        writeln!(output, "        // no instructions decoded").unwrap();
        return;
    }

    for line in statements {
        if line.is_empty() {
            writeln!(output).unwrap();
        } else {
            writeln!(output, "        {line}").unwrap();
        }
    }
}
