use std::path::PathBuf;

use crate::decompiler::{Decompiler, OutputFormat};
use crate::disassembler::UnknownHandling;
use crate::error::Result;
use crate::util;

use super::super::args::{Cli, DecompileFormat};
use super::super::reports::{self, DecompileReport, InstructionReport, MethodTokenReport};

impl Cli {
    pub(super) fn run_decompile(
        &self,
        path: &PathBuf,
        format: DecompileFormat,
        output_format: OutputFormat,
        fail_on_unknown_opcodes: bool,
    ) -> Result<()> {
        let handling = if fail_on_unknown_opcodes {
            UnknownHandling::Error
        } else {
            UnknownHandling::Permit
        };
        let decompiler = Decompiler::with_unknown_handling(handling);
        let manifest_path = self.resolve_manifest_path(path);
        // Use explicit output_format, but ensure All is used for JSON format
        let effective_output_format = if matches!(format, DecompileFormat::Json) {
            OutputFormat::All
        } else {
            output_format
        };
        let result = decompiler.decompile_file_with_manifest(
            path,
            manifest_path.as_ref(),
            effective_output_format,
        )?;

        match format {
            DecompileFormat::Pseudocode => {
                print!("{}", result.pseudocode.as_deref().unwrap_or_default());
            }
            DecompileFormat::HighLevel => {
                print!("{}", result.high_level.as_deref().unwrap_or_default());
            }
            DecompileFormat::Both => {
                println!("// High-level view");
                println!("{}", result.high_level.as_deref().unwrap_or_default());
                println!("// Pseudocode view");
                print!("{}", result.pseudocode.as_deref().unwrap_or_default());
            }
            DecompileFormat::Csharp => {
                print!("{}", result.csharp.as_deref().unwrap_or_default());
            }
            DecompileFormat::Json => {
                let script_hash = result.nef.script_hash();
                let method_tokens: Vec<MethodTokenReport> = result
                    .nef
                    .method_tokens
                    .iter()
                    .map(reports::build_method_token_report)
                    .collect();
                let warnings = reports::collect_warnings(&method_tokens);
                let report = DecompileReport {
                    file: path.display().to_string(),
                    manifest_path: manifest_path
                        .or(self.manifest.clone())
                        .map(|p| p.display().to_string()),
                    script_hash_le: util::format_hash(&script_hash),
                    script_hash_be: util::format_hash_be(&script_hash),
                    csharp: result.csharp.clone().unwrap_or_default(),
                    high_level: result.high_level.clone().unwrap_or_default(),
                    pseudocode: result.pseudocode.clone().unwrap_or_default(),
                    instructions: result
                        .instructions
                        .iter()
                        .map(InstructionReport::from)
                        .collect(),
                    method_tokens,
                    manifest: result.manifest.as_ref().map(reports::summarize_manifest),
                    warnings,
                };
                self.print_json(&report)?;
            }
        }
        Ok(())
    }
}
