//! High-level decompilation pipeline shared by the library and CLI.
//! Parses NEF, disassembles bytecode, lifts control flow, and renders text/C#.

use std::fs;
use std::path::Path;

use crate::disassembler::Disassembler;
use crate::error::{NefError, Result};
use crate::instruction::Instruction;
use crate::manifest::ContractManifest;
use crate::nef::{NefFile, NefParser};

/// Maximum file size allowed for NEF files (10 MiB).
pub const MAX_NEF_FILE_SIZE: u64 = 10 * 1024 * 1024;

#[cfg(feature = "cli")]
use clap::ValueEnum;

mod csharp;
mod helpers;
mod high_level;
pub mod ir;
mod pseudocode;

#[cfg(test)]
mod tests;

/// Select which decompiler outputs should be generated.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "cli", derive(ValueEnum))]
pub enum OutputFormat {
    /// Emit pseudocode output.
    Pseudocode,
    /// Emit a higher-level structured output.
    HighLevel,
    /// Emit C# source code output.
    CSharp,
    /// Emit all supported outputs.
    #[default]
    All,
}

impl OutputFormat {
    fn wants_pseudocode(self) -> bool {
        matches!(self, OutputFormat::Pseudocode | OutputFormat::All)
    }

    fn wants_high_level(self) -> bool {
        matches!(self, OutputFormat::HighLevel | OutputFormat::All)
    }

    fn wants_csharp(self) -> bool {
        matches!(self, OutputFormat::CSharp | OutputFormat::All)
    }
}

/// Main entry point used by the CLI and tests.
#[derive(Debug, Default)]
pub struct Decompiler {
    parser: NefParser,
    disassembler: Disassembler,
}

impl Decompiler {
    /// Create a new decompiler that permits unknown opcodes during disassembly.
    ///
    /// This is equivalent to `Decompiler::with_unknown_handling(UnknownHandling::Permit)`.
    #[must_use]
    pub fn new() -> Self {
        Self::with_unknown_handling(crate::disassembler::UnknownHandling::Permit)
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
    pub fn with_unknown_handling(handling: crate::disassembler::UnknownHandling) -> Self {
        Self {
            parser: NefParser::new(),
            disassembler: Disassembler::with_unknown_handling(handling),
        }
    }

    /// Decompile a NEF blob already loaded in memory.
    pub fn decompile_bytes(&self, bytes: &[u8]) -> Result<Decompilation> {
        self.decompile_bytes_with_manifest(bytes, None, OutputFormat::All)
    }

    /// Decompile a NEF blob using an optional manifest.
    pub fn decompile_bytes_with_manifest(
        &self,
        bytes: &[u8],
        manifest: Option<ContractManifest>,
        output_format: OutputFormat,
    ) -> Result<Decompilation> {
        let nef = self.parser.parse(bytes)?;
        let instructions = self.disassembler.disassemble(&nef.script)?;
        let pseudocode = output_format
            .wants_pseudocode()
            .then(|| pseudocode::render(&instructions));
        let high_level = output_format
            .wants_high_level()
            .then(|| high_level::render_high_level(&nef, &instructions, manifest.as_ref()));
        let csharp = output_format
            .wants_csharp()
            .then(|| csharp::render_csharp(&nef, &instructions, manifest.as_ref()));

        Ok(Decompilation {
            nef,
            manifest,
            instructions,
            pseudocode,
            high_level,
            csharp,
        })
    }

    /// Decompile a NEF file from disk.
    pub fn decompile_file<P: AsRef<Path>>(&self, path: P) -> Result<Decompilation> {
        let path = path.as_ref();
        let size = fs::metadata(path)?.len();
        if size > MAX_NEF_FILE_SIZE {
            return Err(NefError::FileTooLarge {
                size,
                max: MAX_NEF_FILE_SIZE,
            }
            .into());
        }
        let data = fs::read(path)?;
        self.decompile_bytes(&data)
    }

    /// Decompile a NEF file alongside an optional manifest file.
    pub fn decompile_file_with_manifest<P, Q>(
        &self,
        nef_path: P,
        manifest_path: Option<Q>,
        output_format: OutputFormat,
    ) -> Result<Decompilation>
    where
        P: AsRef<Path>,
        Q: AsRef<Path>,
    {
        let nef_path = nef_path.as_ref();
        let size = fs::metadata(nef_path)?.len();
        if size > MAX_NEF_FILE_SIZE {
            return Err(NefError::FileTooLarge {
                size,
                max: MAX_NEF_FILE_SIZE,
            }
            .into());
        }
        let data = fs::read(nef_path)?;
        let manifest = match manifest_path {
            Some(path) => Some(ContractManifest::from_file(path)?),
            None => None,
        };
        self.decompile_bytes_with_manifest(&data, manifest, output_format)
    }
}

/// Result of a successful decompilation run.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Decompilation {
    /// Parsed NEF container.
    pub nef: NefFile,
    /// Optional parsed contract manifest.
    pub manifest: Option<ContractManifest>,
    /// Disassembled instruction stream from the NEF script.
    pub instructions: Vec<Instruction>,
    /// Optional rendered pseudocode output.
    pub pseudocode: Option<String>,
    /// Optional rendered high-level output.
    pub high_level: Option<String>,
    /// Optional rendered C# output.
    pub csharp: Option<String>,
}
