use std::io::Write as _;
use std::path::PathBuf;

use crate::decompiler::Decompiler;
use crate::error::Result;
use crate::nef::NefParser;
use crate::util;

use super::super::args::{Cli, DisasmFormat};
use super::super::reports::{DisasmReport, InstructionReport};

impl Cli {
    pub(super) fn run_disasm(
        &self,
        path: &PathBuf,
        format: DisasmFormat,
        fail_on_unknown_opcodes: bool,
    ) -> Result<()> {
        let handling = Self::unknown_handling(fail_on_unknown_opcodes);
        let decompiler = Decompiler::with_unknown_handling(handling);
        let result = decompiler.disassemble_file(path)?;
        // Parse the NEF a second time only when emitting JSON, so the
        // text path stays cheap. Both parses have already succeeded
        // by this point (disassemble_file ran the same parser
        // internally), so this is just to recover the script hash for
        // the report.
        let script_hash = if matches!(format, DisasmFormat::Json) {
            let bytes = Self::read_nef_bytes(path)?;
            Some(NefParser::new().parse(&bytes)?.script_hash())
        } else {
            None
        };
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
                    Self::write_warnings(out, &result.warnings)
                })?;
            }
            DisasmFormat::Json => {
                let instructions: Vec<InstructionReport> = result
                    .instructions
                    .iter()
                    .map(InstructionReport::from)
                    .collect();
                let script_hash = script_hash.expect("script_hash computed for JSON path");
                let report = DisasmReport {
                    file: path.display().to_string(),
                    script_hash_le: util::format_hash(&script_hash),
                    script_hash_be: util::format_hash_be(&script_hash),
                    instructions,
                    warnings: result.warnings.iter().map(ToString::to_string).collect(),
                };
                self.print_json(&report)?;
            }
        }
        Ok(())
    }
}
