use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand, ValueEnum};

use crate::decompiler::Decompiler;
use crate::error::Result;
use crate::manifest::ContractManifest;
use crate::native_contracts;
use crate::nef::NefParser;

/// Command line interface for the minimal Neo N3 decompiler.
#[derive(Debug, Parser)]
#[command(author, version, about = "Inspect Neo N3 NEF bytecode", long_about = None)]
pub struct Cli {
    /// Optional path to the companion manifest JSON file.
    #[arg(long, global = true)]
    manifest: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Show NEF header information
    Info { path: PathBuf },

    /// Decode bytecode into instructions
    Disasm { path: PathBuf },

    /// Parse and pretty-print the bytecode
    Decompile {
        path: PathBuf,

        /// Choose the output view
        #[arg(long, value_enum, default_value_t = DecompileFormat::HighLevel)]
        format: DecompileFormat,
    },

    /// List method tokens embedded in the NEF file
    Tokens { path: PathBuf },
}

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
enum DecompileFormat {
    Pseudocode,
    #[default]
    HighLevel,
    Both,
}

impl Cli {
    pub fn run(&self) -> Result<()> {
        match &self.command {
            Command::Info { path } => self.run_info(path),
            Command::Disasm { path } => self.run_disasm(path),
            Command::Decompile { path, format } => self.run_decompile(path, *format),
            Command::Tokens { path } => self.run_tokens(path),
        }
    }

    fn run_info(&self, path: &PathBuf) -> Result<()> {
        let data = std::fs::read(path)?;
        let nef = NefParser::new().parse(&data)?;

        println!("File: {}", path.display());
        println!("Compiler: {}", nef.header.compiler);
        println!("Version: {}", nef.header.version);
        println!("Script length: {} bytes", nef.header.script_length);
        println!("Method tokens: {}", nef.method_tokens.len());
        println!("Checksum: 0x{:08X}", nef.checksum);

        if let Some(manifest) = self.load_manifest(path)? {
            println!("Manifest contract: {}", manifest.name);
            if !manifest.supported_standards.is_empty() {
                println!(
                    "Supported standards: {}",
                    manifest.supported_standards.join(", ")
                );
            }
            println!(
                "ABI methods: {} events: {}",
                manifest.abi.methods.len(),
                manifest.abi.events.len()
            );
            println!(
                "Features: storage={} payable={}",
                manifest.features.storage, manifest.features.payable
            );
        }
        Ok(())
    }

    fn run_disasm(&self, path: &PathBuf) -> Result<()> {
        let decompiler = Decompiler::new();
        let result = decompiler.decompile_file(path)?;
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
        Ok(())
    }

    fn run_decompile(&self, path: &PathBuf, format: DecompileFormat) -> Result<()> {
        let decompiler = Decompiler::new();
        let manifest_path = self.resolve_manifest_path(path);
        let result = decompiler.decompile_file_with_manifest(path, manifest_path.as_ref())?;

        match format {
            DecompileFormat::Pseudocode => {
                print!("{}", result.pseudocode);
            }
            DecompileFormat::HighLevel => {
                print!("{}", result.high_level);
            }
            DecompileFormat::Both => {
                println!("// High-level view");
                println!("{}", result.high_level);
                println!("// Pseudocode view");
                print!("{}", result.pseudocode);
            }
        }
        Ok(())
    }

    fn run_tokens(&self, path: &PathBuf) -> Result<()> {
        let data = std::fs::read(path)?;
        let nef = NefParser::new().parse(&data)?;

        if nef.method_tokens.is_empty() {
            println!("(no method tokens)");
            return Ok(());
        }

        for (index, token) in nef.method_tokens.iter().enumerate() {
            let contract_label = native_contracts::lookup(&token.hash)
                .map(|info| format!(" ({})", info.name))
                .unwrap_or_default();
            println!(
                "#{index}: hash={}{} method={} params={} return=0x{:02X} flags=0x{:02X}",
                format_hash(&token.hash),
                contract_label,
                token.method,
                token.params,
                token.return_type,
                token.call_flags
            );
        }

        Ok(())
    }

    fn load_manifest(&self, nef_path: &Path) -> Result<Option<ContractManifest>> {
        match self.resolve_manifest_path(nef_path) {
            Some(path) => Ok(Some(ContractManifest::from_file(path)?)),
            None => Ok(None),
        }
    }

    fn resolve_manifest_path(&self, nef_path: &Path) -> Option<PathBuf> {
        if let Some(path) = &self.manifest {
            return Some(path.clone());
        }

        let mut candidate = nef_path.to_path_buf();
        candidate.set_extension("manifest.json");
        if candidate.exists() {
            return Some(candidate);
        }

        None
    }
}

fn format_hash(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    use std::fmt::Write;
    for byte in bytes {
        let _ = write!(s, "{byte:02X}");
    }
    s
}
