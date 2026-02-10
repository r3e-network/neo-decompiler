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
#[command(author, version, about = "Inspect Neo N3 NEF bytecode", long_about = None)]
pub struct Cli {
    /// Optional path to the companion manifest JSON file.
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
