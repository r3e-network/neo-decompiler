use std::io::Write as _;
use std::path::Path;

use crate::decompiler::{Decompilation, Decompiler, OutputFormat};
use crate::error::Result;
use crate::util;

use super::super::args::{Cli, DecompileFormat};
use super::super::reports::{
    self, AnalysisReport, DecompileReport, InstructionReport, MethodTokenReport,
};

impl Cli {
    pub(super) fn run_decompile(
        &self,
        path: &Path,
        format: DecompileFormat,
        output_format: OutputFormat,
        decompiler: Decompiler,
    ) -> Result<()> {
        let manifest_path = self.resolve_manifest_path(path);
        // `--format` selects which view is printed; `--output-format` selects
        // which views are computed. When the requested view is not among the
        // computed ones the printed output is silently empty (exit 0), so when
        // `--output-format` does not cover `--format` upgrade to `All`. Both and
        // Json need several views, so they always require `All`. `--output-format`
        // is still honored whenever it already covers the requested view.
        let output_format_covers_view = match format {
            DecompileFormat::Pseudocode => {
                matches!(output_format, OutputFormat::Pseudocode | OutputFormat::All)
            }
            DecompileFormat::HighLevel => {
                matches!(output_format, OutputFormat::HighLevel | OutputFormat::All)
            }
            DecompileFormat::Csharp => {
                matches!(output_format, OutputFormat::CSharp | OutputFormat::All)
            }
            DecompileFormat::Both | DecompileFormat::Json => output_format == OutputFormat::All,
            // The IR / SSA views are derived from the CFG + SSA, which are
            // always computed, so they don't require a particular output format.
            DecompileFormat::Ir | DecompileFormat::Ssa => true,
        };
        let effective_output_format = if output_format_covers_view {
            output_format
        } else {
            OutputFormat::All
        };

        let data = Self::read_nef_bytes(path)?;
        let manifest = self.load_manifest(path)?;
        let mut result =
            decompiler.decompile_bytes_with_manifest(&data, manifest, effective_output_format)?;

        match format {
            DecompileFormat::Pseudocode => {
                self.write_stdout(|out| {
                    write!(out, "{}", result.pseudocode.as_deref().unwrap_or_default())?;
                    Self::write_warnings(out, &result.warnings)
                })?;
            }
            DecompileFormat::HighLevel => {
                self.write_stdout(|out| {
                    write!(out, "{}", result.high_level.as_deref().unwrap_or_default())?;
                    Self::write_warnings(out, &result.warnings)
                })?;
            }
            DecompileFormat::Both => {
                self.write_stdout(|out| {
                    writeln!(out, "// High-level view")?;
                    writeln!(out, "{}", result.high_level.as_deref().unwrap_or_default())?;
                    writeln!(out, "// Pseudocode view")?;
                    write!(out, "{}", result.pseudocode.as_deref().unwrap_or_default())?;
                    Self::write_warnings(out, &result.warnings)
                })?;
            }
            DecompileFormat::Csharp => {
                self.write_stdout(|out| {
                    write!(out, "{}", result.csharp.as_deref().unwrap_or_default())?;
                    Self::write_warnings(out, &result.warnings)
                })?;
            }
            DecompileFormat::Ir => {
                let text = result.render_structured_ir();
                self.write_stdout(|out| {
                    write!(out, "{text}")?;
                    Self::write_warnings(out, &result.warnings)
                })?;
            }
            DecompileFormat::Ssa => {
                let text = result.render_optimized_ssa();
                self.write_stdout(|out| {
                    write!(out, "{text}")?;
                    Self::write_warnings(out, &result.warnings)
                })?;
            }
            DecompileFormat::Json => {
                let Decompilation {
                    nef,
                    manifest,
                    warnings: decompile_warnings,
                    instructions,
                    call_graph,
                    method_contracts,
                    patterns,
                    xrefs,
                    types,
                    pseudocode,
                    high_level,
                    csharp,
                    ..
                } = result;
                let script_hash = nef.script_hash();
                let method_tokens: Vec<MethodTokenReport> = nef
                    .method_tokens
                    .iter()
                    .map(reports::build_method_token_report)
                    .collect();
                let mut warnings = reports::collect_warnings(&method_tokens);
                for warning in &decompile_warnings {
                    if !warnings.contains(warning) {
                        warnings.push(warning.clone());
                    }
                }
                let report = DecompileReport {
                    file: path.display().to_string(),
                    manifest_path: manifest_path
                        .or(self.manifest.clone())
                        .map(|p| p.display().to_string()),
                    compiler: nef.header.compiler.trim_end_matches('\0').to_string(),
                    source: (!nef.header.source.is_empty()).then(|| nef.header.source.clone()),
                    script_hash_le: util::format_hash(&script_hash),
                    script_hash_be: util::format_hash_be(&script_hash),
                    csharp: csharp.unwrap_or_default(),
                    high_level: high_level.unwrap_or_default(),
                    pseudocode: pseudocode.unwrap_or_default(),
                    instructions: instructions.iter().map(InstructionReport::from).collect(),
                    method_tokens,
                    manifest: manifest.as_ref().map(reports::summarize_manifest),
                    analysis: AnalysisReport {
                        call_graph,
                        method_contracts,
                        patterns,
                        xrefs,
                        types,
                    },
                    warnings,
                };
                self.print_json(&report)?;
            }
        }
        Ok(())
    }
}
