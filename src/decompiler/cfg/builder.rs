//! CFG construction from instruction stream.

use std::collections::{BTreeMap, BTreeSet};

use crate::instruction::Instruction;

use super::graph::Cfg;

mod blocks;
mod edges;
mod leaders;
mod offsets;
mod targets;
mod terminator;

/// Builder for constructing a CFG from instructions.
pub struct CfgBuilder<'a> {
    instructions: &'a [Instruction],
    /// Map from bytecode offset to instruction index.
    offset_to_index: BTreeMap<usize, usize>,
    /// Offsets that are jump targets (start new blocks).
    leaders: BTreeSet<usize>,
}

impl<'a> CfgBuilder<'a> {
    /// Create a new CFG builder.
    #[must_use]
    pub fn new(instructions: &'a [Instruction]) -> Self {
        let mut offset_to_index = BTreeMap::new();
        for (i, instr) in instructions.iter().enumerate() {
            offset_to_index.insert(instr.offset, i);
        }

        Self {
            instructions,
            offset_to_index,
            leaders: BTreeSet::new(),
        }
    }

    /// Build the CFG.
    #[must_use]
    pub fn build(mut self) -> Cfg {
        if self.instructions.is_empty() {
            return Cfg::new();
        }

        // Phase 1: Identify leaders (block start points)
        self.find_leaders();

        // Phase 2: Create basic blocks
        let blocks = self.create_blocks();

        // Phase 3: Build CFG with edges
        self.build_cfg(blocks)
    }
}
