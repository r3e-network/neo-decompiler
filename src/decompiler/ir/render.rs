//! IR to text rendering utilities.

mod expr;
mod stmt;

pub use expr::render_expr;
pub use stmt::{render_block, render_stmt};
