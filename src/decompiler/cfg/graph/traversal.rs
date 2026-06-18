use std::collections::BTreeSet;

use super::super::basic_block::BlockId;
use super::Cfg;

impl Cfg {
    /// Iterate blocks in reverse post-order (useful for dataflow analysis).
    #[must_use]
    pub fn reverse_postorder(&self) -> Vec<BlockId> {
        let mut visited = BTreeSet::new();
        let mut postorder = Vec::new();
        // Iterative DFS post-order with an explicit stack to avoid native stack
        // overflow on deeply nested CFGs produced by large or malformed
        // bytecode (this is a public API reachable with attacker-controlled
        // input). Each frame tracks a block and how many successors it has
        // visited so far.
        if visited.insert(self.entry) {
            let mut stack: Vec<(BlockId, usize)> = vec![(self.entry, 0)];
            while let Some((block, next_idx)) = stack.last_mut() {
                let successors = self.successors(*block);
                if *next_idx < successors.len() {
                    let succ = successors[*next_idx];
                    *next_idx += 1;
                    if visited.insert(succ) {
                        stack.push((succ, 0));
                    }
                } else {
                    postorder.push(*block);
                    stack.pop();
                }
            }
        }
        postorder.reverse();
        postorder
    }
}
