//! Basic block representation for CFG.

mod block;
mod block_id;
mod terminator;

pub use block::BasicBlock;
pub use block_id::BlockId;
pub use terminator::Terminator;
