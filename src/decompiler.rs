//! High-level decompilation pipeline shared by the library and CLI.
//! Parses NEF, disassembles bytecode, lifts control flow, and renders text/C#.

/// Maximum file size allowed for NEF files (1 MiB).
pub const MAX_NEF_FILE_SIZE: u64 = crate::nef::MAX_NEF_FILE_SIZE;

pub mod analysis;
pub mod cfg;
mod csharp;
mod decompilation;
mod helpers;
mod high_level;
pub mod ir;
pub(crate) mod native_method_types;
mod output_format;
mod pipeline;
mod pseudocode;
pub(crate) mod syscall_types;

pub use decompilation::Decompilation;
pub(crate) use high_level::write_contract_header;
pub use output_format::OutputFormat;
pub use pipeline::Decompiler;

/// Return whether a known syscall has a complete, framework-backed C# spelling
/// that does not depend on recovering a VM-specific argument overload.
///
/// The structured C# lowerer uses this to avoid reporting an unnecessary
/// conservative diagnostic for calls that will render directly as a Neo
/// framework API. Keeping the policy behind the decompiler module avoids
/// coupling the renderer-neutral SSA builder to a private renderer submodule.
pub(crate) fn is_exact_csharp_syscall(hash: u32) -> bool {
    csharp::is_exact_syscall(hash)
}

#[cfg(test)]
mod tests;
