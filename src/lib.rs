//! # Neo N3 Decompiler Library
//! 
//! A comprehensive Neo N3 smart contract decompiler that transforms compiled NEF 
//! (Neo Executable Format) bytecode into human-readable pseudocode.
//! 
//! ## Architecture Overview
//! 
//! The decompiler follows a modular pipeline architecture:
//! 
//! ```text
//! NEF File → Frontend → Core Engine → Analysis → Backend → Output
//!    ↓         ↓           ↓          ↓         ↓        ↓
//!  Parser   Disasm     Lifter     CFG/Types  Codegen  Pseudocode
//! ```
//! 
//! ## Quick Start
//! 
//! ```rust,no_run
//! use neo_decompiler::{Decompiler, DecompilerConfig};
//! 
//! let config = DecompilerConfig::default();
//! let decompiler = Decompiler::new(config);
//! 
//! let nef_data = std::fs::read("contract.nef")?;
//! let manifest = std::fs::read_to_string("contract.manifest.json")?;
//! 
//! let result = decompiler.decompile(&nef_data, Some(&manifest))?;
//! println!("{}", result.pseudocode);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

pub mod common;
pub mod frontend;
pub mod core;
pub mod analysis;
pub mod backend;
pub mod plugins;

#[cfg(test)]
mod tests;

#[cfg(feature = "cli")]
pub mod cli;

// Re-export main types for convenience
pub use common::{
    types::{Instruction, OpCode, Operand},
    errors::{DecompilerError, DecompilerResult},
    config::{DecompilerConfig, ConfigLoader},
};

pub use frontend::{
    nef_parser::{NEFParser, NEFFile},
    manifest_parser::{ManifestParser, ContractManifest},
};

pub use core::{
    disassembler::Disassembler,
    lifter::IRLifter,
    decompiler::DecompilerEngine,
};

pub use backend::{
    pseudocode::PseudocodeGenerator,
    reports::ReportGenerator,
};

/// Main decompiler facade providing high-level API
pub struct Decompiler {
    config: DecompilerConfig,
    nef_parser: NEFParser,
    manifest_parser: ManifestParser,
    disassembler: Disassembler,
    lifter: IRLifter,
    engine: DecompilerEngine,
    pseudocode_generator: PseudocodeGenerator,
}

impl Decompiler {
    /// Create new decompiler with configuration
    pub fn new(config: DecompilerConfig) -> Self {
        Self {
            nef_parser: NEFParser::new(),
            manifest_parser: ManifestParser::new(),
            disassembler: Disassembler::new(&config),
            lifter: IRLifter::new(&config),
            engine: DecompilerEngine::new(&config),
            pseudocode_generator: PseudocodeGenerator::new(&config),
            config,
        }
    }

    /// Decompile NEF bytecode to pseudocode
    pub fn decompile(
        &mut self,
        nef_data: &[u8],
        manifest_json: Option<&str>,
    ) -> DecompilerResult<DecompilationResult> {
        // Parse NEF file
        let nef_file = self.nef_parser.parse(nef_data)?;
        
        // Parse manifest if provided
        let manifest = match manifest_json {
            Some(json) => Some(self.manifest_parser.parse(json)?),
            None => None,
        };

        // Disassemble bytecode
        let instructions = self.disassembler.disassemble(&nef_file.bytecode)?;

        // Lift to IR
        let mut ir_function = self.lifter.lift_to_ir(&instructions)?;

        // Apply analysis passes
        self.engine.analyze(&mut ir_function, manifest.as_ref())?;

        // Generate pseudocode
        let pseudocode = self.pseudocode_generator.generate(&ir_function)?;

        Ok(DecompilationResult {
            pseudocode,
            ir_function,
            instructions,
            nef_file,
            manifest,
        })
    }
}

/// Complete decompilation result
#[derive(Debug)]
pub struct DecompilationResult {
    /// Generated pseudocode
    pub pseudocode: String,
    /// Internal IR representation
    pub ir_function: core::ir::IRFunction,
    /// Disassembled instructions
    pub instructions: Vec<Instruction>,
    /// Parsed NEF file
    pub nef_file: NEFFile,
    /// Contract manifest (if provided)
    pub manifest: Option<ContractManifest>,
}

