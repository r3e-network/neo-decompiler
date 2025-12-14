use std::collections::BTreeSet;

use super::super::basic_block::BlockId;
use super::Cfg;

impl Cfg {
    /// Iterate blocks in reverse post-order (useful for dataflow analysis).
    pub fn reverse_postorder(&self) -> Vec<BlockId> {
        let mut visited = BTreeSet::new();
        let mut postorder = Vec::new();
        self.dfs_postorder(self.entry, &mut visited, &mut postorder);
        postorder.reverse();
        postorder
    }

    fn dfs_postorder(
        &self,
        block: BlockId,
        visited: &mut BTreeSet<BlockId>,
        postorder: &mut Vec<BlockId>,
    ) {
        if !visited.insert(block) {
            return;
        }
        for succ in self.successors(block) {
            self.dfs_postorder(succ, visited, postorder);
        }
        postorder.push(block);
    }
}
