use std::path::Path;

use crate::disassembler::Disassembler;
use crate::error::Result;
use crate::instruction::Instruction;
use crate::nef::{NefFile, NefParser};

/// Main entry point used by the CLI and tests.
#[derive(Debug, Default)]
pub struct Decompiler {
    parser: NefParser,
    disassembler: Disassembler,
}

impl Decompiler {
    pub fn new() -> Self {
        Self {
            parser: NefParser::new(),
            disassembler: Disassembler::new(),
        }
    }

    /// Decompile a NEF blob already loaded in memory.
    pub fn decompile_bytes(&self, bytes: &[u8]) -> Result<Decompilation> {
        let nef = self.parser.parse(bytes)?;
        let instructions = self.disassembler.disassemble(&nef.script)?;
        let pseudocode = render_pseudocode(&instructions);
        Ok(Decompilation {
            nef,
            instructions,
            pseudocode,
        })
    }

    /// Decompile a NEF file from disk.
    pub fn decompile_file<P: AsRef<Path>>(&self, path: P) -> Result<Decompilation> {
        let data = std::fs::read(path)?;
        self.decompile_bytes(&data)
    }
}

/// Result of a successful decompilation run.
#[derive(Debug, Clone)]
pub struct Decompilation {
    pub nef: NefFile,
    pub instructions: Vec<Instruction>,
    pub pseudocode: String,
}

fn render_pseudocode(instructions: &[Instruction]) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_nef() -> Vec<u8> {
        // Build a minimal NEF with script: PUSH0, PUSH1, ADD, RET
        let script = [0x10, 0x11, 0x9E, 0x40];
        let mut data = Vec::new();
        data.extend_from_slice(b"NEF3");
        let mut compiler = [0u8; 32];
        compiler[..4].copy_from_slice(b"test");
        data.extend_from_slice(&compiler);
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&(script.len() as u32).to_le_bytes());
        data.push(0); // method token count
        data.extend_from_slice(&script);
        let checksum = NefParser::calculate_checksum(&data);
        data.extend_from_slice(&checksum.to_le_bytes());
        data
    }

    #[test]
    fn decompile_end_to_end() {
        let nef_bytes = sample_nef();
        let decompilation = Decompiler::new()
            .decompile_bytes(&nef_bytes)
            .expect("decompile succeeds");

        assert_eq!(decompilation.instructions.len(), 4);
        assert!(decompilation.pseudocode.contains("ADD"));
    }
}
