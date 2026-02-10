use std::collections::HashSet;

use crate::disassembler::{Disassembler, DisassemblyOutput, UnknownHandling};
use crate::error::Result;
use crate::manifest::ContractManifest;
use crate::nef::NefParser;

use super::cfg::CfgBuilder;
use super::decompilation::Decompilation;
use super::output_format::OutputFormat;
use super::{analysis, csharp, high_level, pseudocode};

mod io;

/// Main entry point used by the CLI and tests.
#[derive(Debug, Default)]
pub struct Decompiler {
    parser: NefParser,
    disassembler: Disassembler,
    inline_single_use_temps: bool,
}

impl Decompiler {
    /// Create a new decompiler that permits unknown opcodes during disassembly.
    ///
    /// This is equivalent to `Decompiler::with_unknown_handling(UnknownHandling::Permit)`.
    #[must_use]
    pub fn new() -> Self {
        Self::with_unknown_handling(UnknownHandling::Permit)
    }

    /// Create a new decompiler configured with the desired unknown-opcode policy.
    ///
    /// Unknown opcodes can appear when disassembling corrupted inputs or when
    /// targeting a newer VM revision. Use [`crate::UnknownHandling::Error`] to
    /// fail fast, or [`crate::UnknownHandling::Permit`] to emit `Unknown`
    /// instructions and continue.
    ///
    /// # Examples
    /// ```
    /// use neo_decompiler::{Decompiler, UnknownHandling};
    ///
    /// let decompiler = Decompiler::with_unknown_handling(UnknownHandling::Error);
    /// let _ = decompiler;
    /// ```
    #[must_use]
    pub fn with_unknown_handling(handling: UnknownHandling) -> Self {
        Self {
            parser: NefParser::new(),
            disassembler: Disassembler::with_unknown_handling(handling),
            inline_single_use_temps: false,
        }
    }

    /// Enable experimental inlining of single-use temporary variables in the high-level view.
    ///
    /// This can reduce noise in lifted code by replacing temps like `t0` with their RHS
    /// at the first use site, but it may reduce readability for larger expressions.
    #[must_use]
    pub fn with_inline_single_use_temps(mut self, enabled: bool) -> Self {
        self.inline_single_use_temps = enabled;
        self
    }

    /// Decompile a NEF blob already loaded in memory.
    ///
    /// # Errors
    ///
    /// Returns an error if the NEF container is malformed or disassembly fails.
    pub fn decompile_bytes(&self, bytes: &[u8]) -> Result<Decompilation> {
        self.decompile_bytes_with_manifest(bytes, None, OutputFormat::All)
    }

    /// Disassemble a NEF blob already loaded in memory.
    ///
    /// This fast path parses the NEF container and decodes instructions only;
    /// it skips CFG construction, analysis passes, and renderers.
    ///
    /// # Errors
    ///
    /// Returns an error if the NEF container is malformed or disassembly fails.
    pub fn disassemble_bytes(&self, bytes: &[u8]) -> Result<DisassemblyOutput> {
        let nef = self.parser.parse(bytes)?;
        self.disassembler.disassemble_with_warnings(&nef.script)
    }

    /// Decompile a NEF blob using an optional manifest.
    ///
    /// # Errors
    ///
    /// Returns an error if the NEF container is malformed or disassembly fails.
    pub fn decompile_bytes_with_manifest(
        &self,
        bytes: &[u8],
        manifest: Option<ContractManifest>,
        output_format: OutputFormat,
    ) -> Result<Decompilation> {
        let nef = self.parser.parse(bytes)?;
        let disassembly = self.disassembler.disassemble_with_warnings(&nef.script)?;
        let instructions = disassembly.instructions;

        let mut warnings = Vec::new();
        let mut seen_warnings = HashSet::new();
        let mut push_warning = |warning: String| {
            if seen_warnings.insert(warning.clone()) {
                warnings.push(warning);
            }
        };
        for warning in disassembly.warnings {
            push_warning(warning.to_string());
        }

        let cfg = CfgBuilder::new(&instructions).build();
        let call_graph =
            analysis::call_graph::build_call_graph(&nef, &instructions, manifest.as_ref());
        let xrefs = analysis::xrefs::build_xrefs(&instructions, manifest.as_ref());
        let types = analysis::types::infer_types(&instructions, manifest.as_ref());

        let pseudocode = output_format
            .wants_pseudocode()
            .then(|| pseudocode::render(&instructions));
        let high_level = output_format.wants_high_level().then(|| {
            let render = high_level::render_high_level(
                &nef,
                &instructions,
                manifest.as_ref(),
                self.inline_single_use_temps,
            );
            for warning in render.warnings {
                push_warning(warning);
            }
            render.text
        });
        let csharp = output_format.wants_csharp().then(|| {
            let render = csharp::render_csharp(&nef, &instructions, manifest.as_ref());
            for warning in render.warnings {
                push_warning(warning);
            }
            render.source
        });

        Ok(Decompilation {
            nef,
            manifest,
            warnings,
            instructions,
            cfg,
            call_graph,
            xrefs,
            types,
            pseudocode,
            high_level,
            csharp,
            ssa: None,
        })
    }

    /// Decompile a NEF file from disk.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read, the NEF container is
    /// malformed, or disassembly fails.
    pub fn decompile_file<P: AsRef<std::path::Path>>(&self, path: P) -> Result<Decompilation> {
        self.io_decompile_file(path)
    }

    /// Disassemble a NEF file from disk.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read, the NEF container is
    /// malformed, or disassembly fails.
    pub fn disassemble_file<P: AsRef<std::path::Path>>(
        &self,
        path: P,
    ) -> Result<DisassemblyOutput> {
        self.io_disassemble_file(path)
    }

    /// Decompile a NEF file alongside an optional manifest file.
    ///
    /// # Errors
    ///
    /// Returns an error if either file cannot be read, the NEF container is
    /// malformed, the manifest JSON is invalid, or disassembly fails.
    pub fn decompile_file_with_manifest<P, Q>(
        &self,
        nef_path: P,
        manifest_path: Option<Q>,
        output_format: OutputFormat,
    ) -> Result<Decompilation>
    where
        P: AsRef<std::path::Path>,
        Q: AsRef<std::path::Path>,
    {
        self.io_decompile_file_with_manifest(nef_path, manifest_path, output_format)
    }
}
