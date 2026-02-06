//! Typed intermediate representation for decompiled code.
//!
//! This module provides a structured IR that separates semantic analysis
//! from text rendering, enabling cleaner transformations and multiple output formats.

#[allow(missing_docs)]
mod control_flow;
#[allow(missing_docs)]
mod expression;
#[allow(missing_docs)]
mod render;
#[allow(missing_docs)]
mod simplify;
#[allow(missing_docs)]
mod statement;

pub use control_flow::ControlFlow;
pub use expression::{BinOp, Expr, Literal, UnaryOp};
pub use render::{render_block, render_expr, render_stmt};
pub use simplify::simplify;
pub use statement::{Block, Stmt};

#[cfg(test)]
mod tests;
