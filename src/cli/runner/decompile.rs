use std::fs;
use std::io::Write as _;
use std::path::PathBuf;

use crate::decompiler::{Decompiler, OutputFormat, MAX_NEF_FILE_SIZE};
use crate::disassembler::UnknownHandling;
use crate::error::{NefError, Result};
use crate::manifest::ContractManifest;
use crate::util;

use super::super::args::{Cli, DecompileFormat};
use super::super::reports::{
    self, AnalysisReport, DecompileReport, InstructionReport, MethodTokenReport,
};

impl Cli {
    pub(super) fn run_decompile(
        &self,
        path: &PathBuf,
        format: DecompileFormat,
        output_format: OutputFormat,
        fail_on_unknown_opcodes: bool,
        inline_single_use_temps: bool,
    ) -> Result<()> {
        let handling = if fail_on_unknown_opcodes {
            UnknownHandling::Error
        } else {
            UnknownHandling::Permit
        };
        let decompiler = Decompiler::with_unknown_handling(handling)
            .with_inline_single_use_temps(inline_single_use_temps);
        let manifest_path = self.resolve_manifest_path(path);
        // Use explicit output_format, but ensure All is used for JSON format
        let effective_output_format = if matches!(format, DecompileFormat::Json) {
            OutputFormat::All
        } else {
            output_format
        };

        let size = fs::metadata(path)?.len();
        if size > MAX_NEF_FILE_SIZE {
            return Err(NefError::FileTooLarge {
                size,
                max: MAX_NEF_FILE_SIZE,
            }
            .into());
        }
        let data = fs::read(path)?;
        let manifest = match manifest_path.as_ref() {
            Some(path) => Some(if self.strict_manifest {
                ContractManifest::from_file_strict(path)?
            } else {
                ContractManifest::from_file(path)?
            }),
            None => None,
        };
        let result =
            decompiler.decompile_bytes_with_manifest(&data, manifest, effective_output_format)?;

        match format {
            DecompileFormat::Pseudocode => {
                self.write_stdout(|out| {
                    write!(out, "{}", result.pseudocode.as_deref().unwrap_or_default())?;
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
            DecompileFormat::HighLevel => {
                self.write_stdout(|out| {
                    write!(out, "{}", result.high_level.as_deref().unwrap_or_default())?;
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
            DecompileFormat::Both => {
                self.write_stdout(|out| {
                    writeln!(out, "// High-level view")?;
                    writeln!(out, "{}", result.high_level.as_deref().unwrap_or_default())?;
                    writeln!(out, "// Pseudocode view")?;
                    write!(out, "{}", result.pseudocode.as_deref().unwrap_or_default())?;
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
            DecompileFormat::Csharp => {
                self.write_stdout(|out| {
                    write!(out, "{}", result.csharp.as_deref().unwrap_or_default())?;
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
            DecompileFormat::Json => {
                let script_hash = result.nef.script_hash();
                let method_tokens: Vec<MethodTokenReport> = result
                    .nef
                    .method_tokens
                    .iter()
                    .map(reports::build_method_token_report)
                    .collect();
                let mut warnings = reports::collect_warnings(&method_tokens);
                for warning in &result.warnings {
                    if !warnings.contains(warning) {
                        warnings.push(warning.clone());
                    }
                }
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
                    analysis: AnalysisReport {
                        call_graph: result.call_graph.clone(),
                        xrefs: result.xrefs.clone(),
                        types: result.types.clone(),
                    },
                    warnings,
                };
                self.print_json(&report)?;
            }
        }
        Ok(())
    }
}
