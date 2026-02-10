use std::io::Write as _;
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
        let result = decompiler.disassemble_file(path)?;
        match format {
            DisasmFormat::Text => {
                self.write_stdout(|out| {
                    for instruction in &result.instructions {
                        match instruction.operand {
                            Some(ref operand) => {
                                writeln!(
                                    out,
                                    "{:04X}: {:<10} {}",
                                    instruction.offset, instruction.opcode, operand
                                )?;
                            }
                            None => {
                                writeln!(
                                    out,
                                    "{:04X}: {}",
                                    instruction.offset, instruction.opcode
                                )?;
                            }
                        }
                    }
                    if !result.warnings.is_empty() {
                        writeln!(out)?;
                        writeln!(out, "Warnings:")?;
                        for warning in &result.warnings {
                            writeln!(out, "- {warning}")?;
                        }
                    }
                    Ok(())
                })?;
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
                    warnings: result.warnings.iter().map(ToString::to_string).collect(),
                };
                self.print_json(&report)?;
            }
        }
        Ok(())
    }
}
