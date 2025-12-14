use std::collections::BTreeSet;

use super::super::basic_block::BlockId;
use super::Cfg;

impl Cfg {
    /// Determine which basic blocks are reachable from the entry block.
    ///
    /// This is the foundation for dead-code detection: blocks that are not
    /// reachable via CFG edges are considered unreachable/dead.
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
            for succ in self.successors(id) {
                if self.blocks.contains_key(&succ) {
                    stack.push(succ);
                }
            }
        }
        visited
    }

    /// Determine which basic blocks are unreachable (dead code).
    pub fn unreachable_blocks(&self) -> BTreeSet<BlockId> {
        let reachable = self.reachable_blocks();
        self.blocks
            .keys()
            .copied()
            .filter(|id| !reachable.contains(id))
            .collect()
    }

    /// Check whether a block is reachable from the entry block.
    pub fn is_reachable(&self, id: BlockId) -> bool {
        self.reachable_blocks().contains(&id)
    }
}
