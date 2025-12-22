//! High-level decompilation pipeline shared by the library and CLI.
//! Parses NEF, disassembles bytecode, lifts control flow, and renders text/C#.

/// Maximum file size allowed for NEF files (10 MiB).
pub const MAX_NEF_FILE_SIZE: u64 = crate::nef::MAX_NEF_FILE_SIZE;

pub mod analysis;
pub mod cfg;
mod csharp;
mod decompilation;
mod helpers;
mod high_level;
pub mod ir;
mod output_format;
mod pipeline;
mod pseudocode;

pub use decompilation::Decompilation;
pub use output_format::OutputFormat;
pub use pipeline::Decompiler;

#[cfg(test)]
mod tests;
