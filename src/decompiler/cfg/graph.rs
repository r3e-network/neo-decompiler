//! Control Flow Graph structure and operations.

mod core;
mod edge;

mod dot;
mod reachability;
mod traversal;

pub use self::core::Cfg;
pub use self::edge::{Edge, EdgeKind};
