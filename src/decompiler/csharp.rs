//! C# output renderer for the decompiler.

mod helpers;
mod render;

#[cfg(test)]
pub(super) use helpers::legacy_statement_to_csharp;
pub(crate) use render::render_csharp;
#[cfg(test)]
pub(crate) use render::{BodyBackend, CSharpRender};

pub(crate) fn is_exact_syscall(hash: u32) -> bool {
    render::is_exact_syscall(hash)
}
