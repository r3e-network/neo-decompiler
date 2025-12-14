use std::collections::{BTreeMap, BTreeSet};

use super::super::basic_block::{BasicBlock, BlockId, Terminator};
use super::edge::{Edge, EdgeKind};

/// A Control Flow Graph representing the structure of a function/contract.
#[derive(Debug, Clone)]
pub struct Cfg {
    /// All basic blocks in the CFG, indexed by BlockId.
    pub(super) blocks: BTreeMap<BlockId, BasicBlock>,
    /// Edges between blocks.
    pub(super) edges: Vec<Edge>,
    /// Entry block ID.
    pub(super) entry: BlockId,
    /// Exit block IDs (blocks that return/throw/abort).
    pub(super) exits: BTreeSet<BlockId>,
}

impl Cfg {
    /// Create a new empty CFG.
    pub fn new() -> Self {
        Self {
            blocks: BTreeMap::new(),
            edges: Vec::new(),
            entry: BlockId::ENTRY,
            exits: BTreeSet::new(),
        }
    }

    /// Add a basic block to the CFG.
    pub fn add_block(&mut self, block: BasicBlock) {
        let id = block.id;
        if matches!(
            block.terminator,
            Terminator::Return | Terminator::Throw | Terminator::Abort
        ) {
            self.exits.insert(id);
        }
        self.blocks.insert(id, block);
    }

    /// Add an edge between two blocks.
    pub fn add_edge(&mut self, from: BlockId, to: BlockId, kind: EdgeKind) {
        self.edges.push(Edge { from, to, kind });
    }

    /// Get a block by ID.
    pub fn block(&self, id: BlockId) -> Option<&BasicBlock> {
        self.blocks.get(&id)
    }

    /// Get a mutable block by ID.
    pub fn block_mut(&mut self, id: BlockId) -> Option<&mut BasicBlock> {
        self.blocks.get_mut(&id)
    }

    /// Get the entry block.
    pub fn entry_block(&self) -> Option<&BasicBlock> {
        self.blocks.get(&self.entry)
    }

    /// Get all blocks.
    pub fn blocks(&self) -> impl Iterator<Item = &BasicBlock> {
        self.blocks.values()
    }

    /// Get the number of blocks.
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    /// Get all edges.
    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    /// Get successors of a block.
    pub fn successors(&self, id: BlockId) -> Vec<BlockId> {
        self.edges
            .iter()
            .filter(|e| e.from == id)
            .map(|e| e.to)
            .collect()
    }

    /// Get predecessors of a block.
    pub fn predecessors(&self, id: BlockId) -> Vec<BlockId> {
        self.edges
            .iter()
            .filter(|e| e.to == id)
            .map(|e| e.from)
            .collect()
    }

    /// Get exit blocks.
    pub fn exit_blocks(&self) -> &BTreeSet<BlockId> {
        &self.exits
    }

    /// Find block containing the given offset.
    pub fn block_at_offset(&self, offset: usize) -> Option<&BasicBlock> {
        self.blocks.values().find(|b| b.contains_offset(offset))
    }
}

impl Default for Cfg {
    fn default() -> Self {
        Self::new()
    }
}
