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
        /// Path to the NEF file. The companion `<PATH>.manifest.json`
        /// is auto-discovered when not passed via `--manifest`.
        path: PathBuf,

        /// Choose the output format
        #[arg(long, value_enum, default_value_t = InfoFormat::Text)]
        format: InfoFormat,
    },

    /// Decode bytecode into instructions
    Disasm {
        /// Path to the NEF file.
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
        /// Path to the NEF file. The companion `<PATH>.manifest.json`
        /// is auto-discovered when not passed via `--manifest`; when
        /// found, the contract name is included in the graph label.
        path: PathBuf,

        /// Fail fast if an unknown opcode is encountered (default: tolerate and emit UNKNOWN_0x..)
        #[arg(long)]
        fail_on_unknown_opcodes: bool,
    },

    /// Parse and pretty-print the bytecode
    Decompile {
        /// Path to the NEF file. The companion `<PATH>.manifest.json`
        /// is auto-discovered when not passed via `--manifest`.
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

        /// Emit per-instruction `// XXXX: OPCODE` trace comments above
        /// each lifted statement. Off by default (the lifted source is
        /// rendered without trace noise); pass this flag when
        /// cross-referencing the high-level view against raw bytecode.
        #[arg(long)]
        trace_comments: bool,

        /// Suppress single-use temp inlining. By default, temporaries
        /// referenced exactly once are inlined into their consumer for
        /// readability; pass this flag to keep every `let tN = ...`
        /// statement visible (useful when correlating against
        /// `--trace-comments` or against raw bytecode offsets).
        #[arg(long)]
        no_inline_temps: bool,

        // The flags below pre-date the default flip described above
        // and are kept as hidden no-op aliases so existing scripts
        // and CI configurations continue to work. They were the
        // opt-in path to the new default behaviour.
        #[arg(long, hide = true)]
        inline_single_use_temps: bool,
        #[arg(long, hide = true)]
        no_trace_comments: bool,
        #[arg(long, hide = true)]
        clean: bool,
    },

    /// List method tokens embedded in the NEF file
    Tokens {
        /// Path to the NEF file.
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
