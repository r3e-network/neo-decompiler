//! CLI command execution.

mod catalog;
mod cfg;
mod common;
mod decompile;
mod disasm;
mod info;
mod schema;
mod tokens;

use crate::error::Result;

use super::args::{Cli, Command};

impl Cli {
    /// Execute the selected CLI subcommand.
    ///
    /// # Errors
    ///
    /// Returns an error if the subcommand encounters an I/O failure, a
    /// malformed NEF container, an invalid manifest, or a disassembly problem.
    pub fn run(&self) -> Result<()> {
        match &self.command {
            Command::Info { path, format } => self.run_info(path, *format),
            Command::Disasm {
                path,
                format,
                fail_on_unknown_opcodes,
            } => self.run_disasm(path, *format, *fail_on_unknown_opcodes),
            Command::Cfg {
                path,
                fail_on_unknown_opcodes,
            } => self.run_cfg(path, *fail_on_unknown_opcodes),
            Command::Decompile {
                path,
                format,
                output_format,
                fail_on_unknown_opcodes,
                trace_comments,
                no_inline_temps,
                inline_single_use_temps: _legacy_inline,
                no_trace_comments: _legacy_no_trace,
                clean: _legacy_clean,
            } => self.run_decompile(
                path,
                *format,
                *output_format,
                *fail_on_unknown_opcodes,
                !*no_inline_temps,
                !*trace_comments,
            ),
            Command::Tokens { path, format } => self.run_tokens(path, *format),
            Command::Catalog(args) => self.run_catalog(args),
            Command::Schema(args) => self.run_schema(args),
        }
    }
}
