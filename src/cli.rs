use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::decompiler::Decompiler;
use crate::error::Result;
use crate::nef::NefParser;

/// Command line interface for the minimal Neo N3 decompiler.
#[derive(Debug, Parser)]
#[command(author, version, about = "Inspect Neo N3 NEF bytecode", long_about = None)]
pub struct Cli {
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
    Decompile { path: PathBuf },

    /// List method tokens embedded in the NEF file
    Tokens { path: PathBuf },
}

impl Cli {
    pub fn run(&self) -> Result<()> {
        match &self.command {
            Command::Info { path } => self.run_info(path),
            Command::Disasm { path } => self.run_disasm(path),
            Command::Decompile { path } => self.run_decompile(path),
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

    fn run_decompile(&self, path: &PathBuf) -> Result<()> {
        let decompiler = Decompiler::new();
        let result = decompiler.decompile_file(path)?;
        print!("{}", result.pseudocode);
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
            println!(
                "#{index}: hash={} method={} params={} return=0x{:02X} flags=0x{:02X}",
                format_hash(&token.hash),
                token.method,
                token.params,
                token.return_type,
                token.call_flags
            );
        }

        Ok(())
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
