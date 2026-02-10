use std::collections::BTreeSet;

use super::super::basic_block::BlockId;
use super::Cfg;

impl Cfg {
    /// Determine which basic blocks are reachable from the entry block.
    ///
    /// This is the foundation for dead-code detection: blocks that are not
    /// reachable via CFG edges are considered unreachable/dead.
    #[must_use]
    pub fn reachable_blocks(&self) -> BTreeSet<BlockId> {
        let mut visited = BTreeSet::new();
        if !self.blocks.contains_key(&self.entry) {
            return visited;
        }

        let mut stack = vec![self.entry];
        while let Some(id) = stack.pop() {
            if !visited.insert(id) {
                continue;
            }
            for &succ in self.successors(id) {
                if self.blocks.contains_key(&succ) {
                    stack.push(succ);
                }
            }
        }
        visited
    }

    /// Determine which basic blocks are unreachable (dead code).
    #[must_use]
    pub fn unreachable_blocks(&self) -> BTreeSet<BlockId> {
        let reachable = self.reachable_blocks();
        self.blocks
            .keys()
            .copied()
            .filter(|id| !reachable.contains(id))
            .collect()
    }

    /// Check whether a block is reachable from the entry block.
    ///
    /// **Note:** This recomputes the full reachable set on every call (O(V+E)).
    /// When checking multiple blocks, call [`reachable_blocks`](Self::reachable_blocks)
    /// once and query the returned set instead.
    #[must_use]
    pub fn is_reachable(&self, id: BlockId) -> bool {
        self.reachable_blocks().contains(&id)
    }
}
