use crate::instruction::Instruction;

/// Render a simple offset + mnemonic listing mirroring the disassembly stream.
pub(crate) fn render(instructions: &[Instruction]) -> String {
    use std::fmt::Write;

    let mut output = String::new();
    for instruction in instructions {
        let _ = write!(output, "{:04X}: {}", instruction.offset, instruction.opcode);
        if let Some(operand) = &instruction.operand {
            let _ = write!(output, " {}", operand);
        }
        output.push('\n');
    }
    output
}
