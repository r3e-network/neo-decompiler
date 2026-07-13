//! CFG construction from instruction stream.

use std::collections::{BTreeMap, BTreeSet};

use crate::instruction::Instruction;

use super::graph::Cfg;

mod blocks;
mod edges;
mod finally;
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
    /// Resolved call sites proven not to return normally.
    non_returning_calls: BTreeSet<usize>,
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
            non_returning_calls: BTreeSet::new(),
        }
    }

    /// Mark resolved call sites that terminate their current control-flow path.
    #[must_use]
    pub fn with_non_returning_calls(mut self, offsets: impl IntoIterator<Item = usize>) -> Self {
        self.non_returning_calls.extend(offsets);
        self
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
        let mut blocks = self.create_blocks();
        self.apply_finally_routing(&mut blocks);

        // Phase 3: Build CFG with edges
        self.build_cfg(blocks)
    }
}
