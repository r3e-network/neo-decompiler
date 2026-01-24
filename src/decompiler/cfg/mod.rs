//! Control Flow Graph (CFG) construction and analysis.
//!
//! This module provides explicit basic block representation and graph
//! construction from Neo VM bytecode, enabling advanced analysis passes.

mod basic_block;
mod builder;
mod graph;
pub mod ssa;

pub use basic_block::{BasicBlock, BlockId, Terminator};
pub use builder::CfgBuilder;
pub use graph::{Cfg, Edge, EdgeKind};

#[cfg(test)]
mod tests;
