use std::path::PathBuf;

use clap::Subcommand;

use crate::decompiler::OutputFormat;

use super::catalog::CatalogArgs;
use super::formats::{DecompileFormat, DisasmFormat, InfoFormat, TokensFormat};
use super::schema::SchemaArgs;

#[derive(Debug, Subcommand)]
pub(in crate::cli) enum Command {
    /// Show NEF header information
    Info {
        path: PathBuf,

        /// Choose the output format
        #[arg(long, value_enum, default_value_t = InfoFormat::Text)]
        format: InfoFormat,
    },

    /// Decode bytecode into instructions
    Disasm {
        path: PathBuf,

        /// Choose the output format
        #[arg(long, value_enum, default_value_t = DisasmFormat::Text)]
        format: DisasmFormat,

        /// Fail fast if an unknown opcode is encountered (default: tolerate and emit UNKNOWN_0x..)
        #[arg(long)]
        fail_on_unknown_opcodes: bool,
    },

    /// Render the contract's control flow graph (DOT format)
    Cfg {
        path: PathBuf,

        /// Fail fast if an unknown opcode is encountered (default: tolerate and emit UNKNOWN_0x..)
        #[arg(long)]
        fail_on_unknown_opcodes: bool,
    },

    /// Parse and pretty-print the bytecode
    Decompile {
        path: PathBuf,

        /// Choose the output view
        #[arg(long, value_enum, default_value_t = DecompileFormat::HighLevel)]
        format: DecompileFormat,

        /// Choose which outputs to generate (default: all)
        #[arg(long, value_enum, default_value_t = OutputFormat::All)]
        output_format: OutputFormat,

        /// Fail fast if an unknown opcode is encountered (default: tolerate and emit UNKNOWN_0x..)
        #[arg(long)]
        fail_on_unknown_opcodes: bool,

        /// Inline single-use temporary variables in the high-level view (experimental)
        #[arg(long)]
        inline_single_use_temps: bool,
    },

    /// List method tokens embedded in the NEF file
    Tokens {
        path: PathBuf,

        /// Choose the output view
        #[arg(long, value_enum, default_value_t = TokensFormat::Text)]
        format: TokensFormat,
    },

    /// List the bundled opcode/syscall/native metadata
    Catalog(CatalogArgs),

    /// Print one of the bundled JSON schema documents
    Schema(SchemaArgs),
}
