//! JSON report structures and helpers for CLI output.

mod instructions;
mod manifest;
mod method_tokens;
mod types;

pub(super) use instructions::InstructionReport;
pub(super) use manifest::summarize_manifest;
pub(super) use method_tokens::{
    build_method_token_report, collect_warnings, format_method_token_line, MethodTokenReport,
};
pub(super) use types::{DecompileReport, DisasmReport, InfoReport, TokensReport};
