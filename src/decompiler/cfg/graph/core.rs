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
    /// Pre-computed successor map for O(1) lookup.
    pub(super) successors: BTreeMap<BlockId, Vec<BlockId>>,
    /// Pre-computed predecessor map for O(1) lookup.
    pub(super) predecessors: BTreeMap<BlockId, Vec<BlockId>>,
    /// Index from start_offset → BlockId for O(log n) offset lookup.
    pub(super) offset_to_block: BTreeMap<usize, BlockId>,
}

impl Cfg {
    /// Create a new empty CFG.
    #[must_use]
    pub fn new() -> Self {
        Self {
            blocks: BTreeMap::new(),
            edges: Vec::new(),
            entry: BlockId::ENTRY,
            exits: BTreeSet::new(),
            successors: BTreeMap::new(),
            predecessors: BTreeMap::new(),
            offset_to_block: BTreeMap::new(),
        }
    }

    /// Add a basic block to the CFG.
    pub fn add_block(&mut self, block: BasicBlock) {
        let id = block.id;
        let start_offset = block.start_offset;
        if matches!(
            block.terminator,
            Terminator::Return | Terminator::Throw | Terminator::Abort
        ) {
            self.exits.insert(id);
        }
        self.offset_to_block.insert(start_offset, id);
        self.blocks.insert(id, block);
    }

    /// Add an edge between two blocks.
    pub fn add_edge(&mut self, from: BlockId, to: BlockId, kind: EdgeKind) {
        self.edges.push(Edge { from, to, kind });
        // Maintain adjacency lists for O(1) lookup
        self.successors.entry(from).or_default().push(to);
        self.predecessors.entry(to).or_default().push(from);
    }

    /// Get a block by ID.
    #[must_use]
    pub fn block(&self, id: BlockId) -> Option<&BasicBlock> {
        self.blocks.get(&id)
    }

    /// Get a mutable block by ID.
    pub fn block_mut(&mut self, id: BlockId) -> Option<&mut BasicBlock> {
        self.blocks.get_mut(&id)
    }

    /// Get the entry block.
    #[must_use]
    pub fn entry_block(&self) -> Option<&BasicBlock> {
        self.blocks.get(&self.entry)
    }

    /// Get all blocks.
    pub fn blocks(&self) -> impl Iterator<Item = &BasicBlock> {
        self.blocks.values()
    }

    /// Get the number of blocks.
    #[must_use]
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    /// Get all edges.
    #[must_use]
    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    /// Get successors of a block.
    ///
    /// Returns an empty slice if the block has no successors.
    /// This operation is O(1) due to pre-computed adjacency lists.
    #[must_use]
    pub fn successors(&self, id: BlockId) -> &[BlockId] {
        self.successors
            .get(&id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Get predecessors of a block.
    ///
    /// Returns an empty slice if the block has no predecessors.
    /// This operation is O(1) due to pre-computed adjacency lists.
    #[must_use]
    pub fn predecessors(&self, id: BlockId) -> &[BlockId] {
        self.predecessors
            .get(&id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Get exit blocks.
    #[must_use]
    pub fn exit_blocks(&self) -> &BTreeSet<BlockId> {
        &self.exits
    }

    /// Find block containing the given offset.
    ///
    /// Uses a BTreeMap range query for O(log n) lookup instead of linear scan.
    #[must_use]
    pub fn block_at_offset(&self, offset: usize) -> Option<&BasicBlock> {
        // Find the block whose start_offset is the largest value ≤ offset
        let (_, &block_id) = self.offset_to_block.range(..=offset).next_back()?;
        let block = self.blocks.get(&block_id)?;
        if block.contains_offset(offset) {
            Some(block)
        } else {
            None
        }
    }
}

impl Default for Cfg {
    fn default() -> Self {
        Self::new()
    }
}
