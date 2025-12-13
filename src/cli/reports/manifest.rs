//! Manifest report structures for CLI JSON output.

mod build;
mod convert;
mod model;

pub(in crate::cli) use build::summarize_manifest;
pub(in crate::cli) use model::*;
