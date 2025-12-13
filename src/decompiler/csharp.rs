//! C# output renderer for the decompiler.

mod helpers;
mod render;

#[cfg(test)]
pub(super) use helpers::csharpize_statement;
pub(crate) use render::render_csharp;
