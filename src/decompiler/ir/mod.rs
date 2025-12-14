//! Typed intermediate representation for decompiled code.
//!
//! This module provides a structured IR that separates semantic analysis
//! from text rendering, enabling cleaner transformations and multiple output formats.

#![allow(missing_docs)]

mod control_flow;
mod expression;
mod render;
mod simplify;
mod statement;

pub use control_flow::ControlFlow;
pub use expression::{BinOp, Expr, Literal, UnaryOp};
pub use render::{render_block, render_expr, render_stmt};
pub use simplify::simplify;
pub use statement::{Block, Stmt};

#[cfg(test)]
mod tests;
