use std::path::PathBuf;

use clap::Parser;

mod catalog;
mod commands;
mod formats;
mod schema;

pub(super) use catalog::{CatalogArgs, CatalogFormat, CatalogKind};
pub(super) use commands::Command;
pub(super) use formats::{DecompileFormat, DisasmFormat, InfoFormat, TokensFormat};
pub(super) use schema::SchemaArgs;

/// Command line interface for the minimal Neo N3 decompiler.
#[derive(Debug, Parser)]
#[command(
    author,
    version,
    about = "Inspect, disassemble, and decompile Neo N3 NEF bytecode",
    long_about = "Inspect, disassemble, and decompile Neo N3 NEF bytecode.\n\
        \n\
        - `info` / `tokens`: header metadata, ABI summary, method tokens.\n\
        - `disasm`: instruction stream (text or JSON).\n\
        - `cfg`: control-flow graph as Graphviz DOT.\n\
        - `decompile`: lift bytecode to high-level pseudocode or a C# skeleton.\n\
        - `catalog` / `schema`: bundled opcode/syscall metadata and JSON schemas.\n\
        \n\
        Companion `<NEF>.manifest.json` is auto-discovered alongside the NEF\n\
        unless `--manifest` is passed explicitly."
)]
pub struct Cli {
    /// Optional path to the companion manifest JSON file. When omitted,
    /// `<NEF>.manifest.json` next to the NEF is auto-discovered if it
    /// exists.
    #[arg(long, global = true)]
    pub(super) manifest: Option<PathBuf>,

    /// Emit compact JSON (no extra whitespace) whenever `--format json` is requested.
    #[arg(long, global = true)]
    pub(super) json_compact: bool,

    /// Enforce strict manifest validation (reject non-canonical wildcard-like values).
    #[arg(long, global = true)]
    pub(super) strict_manifest: bool,

    #[command(subcommand)]
    pub(super) command: Command,
}
