//! Control Flow Graph (CFG) construction and analysis.
//!
//! This module provides explicit basic block representation and graph
//! construction from Neo VM bytecode, enabling advanced analysis passes.

mod basic_block;
mod builder;
mod graph;
pub(crate) mod method_body;
pub mod method_view;
mod phi_lowering;
pub mod ssa;
mod structure;

pub use basic_block::{BasicBlock, BlockId, Terminator};
pub use builder::CfgBuilder;
pub use graph::{Cfg, Edge, EdgeKind};
pub use structure::structure as structure_cfg;
pub(crate) use structure::structure_with_source_names as structure_cfg_with_source_names;

#[cfg(test)]
mod tests;
