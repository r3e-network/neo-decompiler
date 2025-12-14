use std::ops::Range;

use super::block_id::BlockId;
use super::terminator::Terminator;

/// A basic block: a sequence of instructions with single entry and exit.
#[derive(Debug, Clone)]
pub struct BasicBlock {
    /// Unique identifier for this block.
    pub id: BlockId,
    /// Starting bytecode offset of this block.
    pub start_offset: usize,
    /// Ending bytecode offset (exclusive) of this block.
    pub end_offset: usize,
    /// Instructions in this block (references by offset range).
    pub instruction_range: Range<usize>,
    /// How this block terminates.
    pub terminator: Terminator,
}

impl BasicBlock {
    /// Create a new basic block.
    pub fn new(
        id: BlockId,
        start_offset: usize,
        end_offset: usize,
        instruction_range: Range<usize>,
        terminator: Terminator,
    ) -> Self {
        Self {
            id,
            start_offset,
            end_offset,
            instruction_range,
            terminator,
        }
    }

    /// Check if this block contains the given offset.
    pub fn contains_offset(&self, offset: usize) -> bool {
        offset >= self.start_offset && offset < self.end_offset
    }

    /// Get the number of instructions in this block.
    pub fn instruction_count(&self) -> usize {
        self.instruction_range.len()
    }

    /// Check if this is an empty block (no instructions).
    pub fn is_empty(&self) -> bool {
        self.instruction_range.is_empty()
    }
}
