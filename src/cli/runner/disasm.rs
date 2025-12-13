use std::path::PathBuf;

use crate::decompiler::Decompiler;
use crate::disassembler::UnknownHandling;
use crate::error::Result;

use super::super::args::{Cli, DisasmFormat};
use super::super::reports::{DisasmReport, InstructionReport};

impl Cli {
    pub(super) fn run_disasm(
        &self,
        path: &PathBuf,
        format: DisasmFormat,
        fail_on_unknown_opcodes: bool,
    ) -> Result<()> {
        let handling = if fail_on_unknown_opcodes {
            UnknownHandling::Error
        } else {
            UnknownHandling::Permit
        };
        let decompiler = Decompiler::with_unknown_handling(handling);
        let result = decompiler.decompile_file(path)?;
        match format {
            DisasmFormat::Text => {
                for instruction in result.instructions {
                    match instruction.operand {
                        Some(ref operand) => {
                            println!(
                                "{:04X}: {:<10} {}",
                                instruction.offset, instruction.opcode, operand
                            );
                        }
                        None => {
                            println!("{:04X}: {}", instruction.offset, instruction.opcode);
                        }
                    }
                }
            }
            DisasmFormat::Json => {
                let instructions: Vec<InstructionReport> = result
                    .instructions
                    .iter()
                    .map(InstructionReport::from)
                    .collect();
                let report = DisasmReport {
                    file: path.display().to_string(),
                    instructions,
                    warnings: Vec::new(),
                };
                self.print_json(&report)?;
            }
        }
        Ok(())
    }
}
