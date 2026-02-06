//! Dominance analysis for SSA construction.
//!
//! Computes immediate dominators, dominator tree, and dominance frontiers
//! using the Cooper-Harvey-Kennedy iterative algorithm.

use std::collections::{BTreeMap, BTreeSet};

use crate::decompiler::cfg::{BlockId, Cfg};

/// Dominance information computed from a CFG.
///
/// This includes immediate dominators, the dominator tree, and dominance frontiers
/// needed for SSA construction.
#[derive(Debug, Clone)]
pub struct DominanceInfo {
    /// Immediate dominator for each block.
    ///
    /// `None` for the entry block (which has no dominator).
    pub idom: BTreeMap<BlockId, Option<BlockId>>,

    /// Dominator tree: parent -> children.
    pub dominator_tree: BTreeMap<BlockId, Vec<BlockId>>,

    /// Dominance frontier for each block.
    ///
    /// Used to determine where to insert φ nodes.
    pub dominance_frontier: BTreeMap<BlockId, BTreeSet<BlockId>>,
}

impl DominanceInfo {
    /// Create a new empty dominance info.
    #[must_use]
    pub fn new() -> Self {
        Self {
            idom: BTreeMap::new(),
            dominator_tree: BTreeMap::new(),
            dominance_frontier: BTreeMap::new(),
        }
    }

    /// Get the immediate dominator of a block.
    ///
    /// Returns `None` for the entry block (which has no dominator).
    #[must_use]
    pub fn idom(&self, block: BlockId) -> Option<BlockId> {
        let idom = self.idom.get(&block).copied().flatten();
        // Entry block has no dominator, even if stored as dominating itself
        if idom == Some(block) {
            None
        } else {
            idom
        }
    }

    /// Get all blocks that this block dominates (children in dominator tree).
    #[must_use]
    pub fn children(&self, block: BlockId) -> &[BlockId] {
        self.dominator_tree
            .get(&block)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Get the dominance frontier of a block as a vector.
    #[must_use]
    pub fn dominance_frontier_vec(&self, block: BlockId) -> Vec<BlockId> {
        self.dominance_frontier
            .get(&block)
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Check if `a` strictly dominates `b`.
    #[must_use]
    pub fn strictly_dominates(&self, a: BlockId, b: BlockId) -> bool {
        if a == b {
            return false;
        }
        let mut current = self.idom(b);
        while let Some(idom) = current {
            if idom == a {
                return true;
            }
            current = self.idom(idom);
        }
        false
    }
}

impl Default for DominanceInfo {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute dominance information for a CFG.
///
/// Uses the Cooper-Harvey-Kennedy iterative algorithm:
/// 1. Initialize: entry dominates itself, others unknown
/// 2. Iterate: Intersect dominators of predecessors until convergence
/// 3. Build dominator tree from immediate dominator relationships
/// 4. Compute dominance frontiers for φ node insertion
///
/// Complexity: O(n²) worst case, but typically much faster for structured code.
pub fn compute(cfg: &Cfg) -> DominanceInfo {
    if cfg.blocks().count() == 0 {
        return DominanceInfo::new();
    }

    let idom = compute_immediate_dominators(cfg);
    let dominator_tree = build_dominator_tree(&idom);
    let dominance_frontier = compute_df(cfg, &idom);

    DominanceInfo {
        idom,
        dominator_tree,
        dominance_frontier,
    }
}

/// Compute immediate dominators using the Cooper-Harvey-Kennedy algorithm.
///
/// For each block n, IDOM(n) is the unique block that:
/// - Strictly dominates n
/// - Does not strictly dominate any other block that dominates n
fn compute_immediate_dominators(cfg: &Cfg) -> BTreeMap<BlockId, Option<BlockId>> {
    let mut idom: BTreeMap<BlockId, Option<BlockId>> = BTreeMap::new();

    // Get entry block ID
    let entry_id = cfg.entry_block().map(|b| b.id);

    // Initialize: entry dominates itself, others are unknown (None)
    for block in cfg.blocks() {
        let block_id = block.id;
        idom.insert(
            block_id,
            if Some(block_id) == entry_id {
                Some(block_id)
            } else {
                None
            },
        );
    }

    // Iterate until convergence
    // Pre-compute RPO once — the CFG is immutable during the fixpoint loop.
    let rpo = reverse_post_order(cfg);
    let mut changed = true;
    let mut iteration_count = 0u32;
    while changed {
        iteration_count += 1;
        if iteration_count > 1000 {
            // Gracefully return partial results instead of panicking
            // This can happen with pathological CFGs from malformed bytecode
            break;
        }
        changed = false;

        // Process blocks in reverse post-order (predecessors processed first)
        for &block_id in &rpo {
            if Some(block_id) == entry_id {
                continue;
            }

            // Find the new dominator by intersecting predecessors' dominators
            let new_idom = intersect_dominators(cfg, block_id, &idom);

            let current_value = idom.get(&block_id).and_then(|o| *o);
            if current_value != new_idom {
                idom.insert(block_id, new_idom);
                changed = true;
            }
        }
    }

    idom
}

/// Find the intersection of dominators for all predecessors of a block.
///
/// This implements the "intersect" operation from the CHK algorithm:
/// - Start with the first predecessor's dominator
/// - For each subsequent predecessor, find the common dominator
/// - Uses the "finger" method to walk up the dominator chains
fn intersect_dominators(
    cfg: &Cfg,
    block: BlockId,
    idom: &BTreeMap<BlockId, Option<BlockId>>,
) -> Option<BlockId> {
    let predecessors = cfg.predecessors(block);

    if predecessors.is_empty() {
        return None;
    }

    // Start with the first processed predecessor (the predecessor itself, per CHK algorithm)
    let mut result = None;

    for pred in predecessors.iter() {
        let pred_idom = idom.get(pred).copied().flatten();

        result = match result {
            None => {
                // First processed predecessor: use the predecessor itself (not its idom).
                // Skip unprocessed predecessors (pred_idom == None).
                if pred_idom.is_some() {
                    Some(*pred)
                } else {
                    None
                }
            }
            Some(current) => {
                // Skip predecessors that haven't been processed yet (idom = None)
                match pred_idom {
                    None => Some(current),
                    Some(_) => Some(find_common_dominator(cfg, current, *pred, idom)),
                }
            }
        };
    }

    result
}

/// Find the least common ancestor (dominator) of two blocks.
///
/// Uses the "finger" method: move fingers up the dominator chains
/// until they meet at the common ancestor.
///
/// Returns the common dominator, or falls back to finger1 if the algorithm
/// fails to converge (e.g., due to malformed CFG from invalid bytecode).
fn find_common_dominator(
    _cfg: &Cfg,
    mut finger1: BlockId,
    mut finger2: BlockId,
    idom: &BTreeMap<BlockId, Option<BlockId>>,
) -> BlockId {
    // Move fingers to the same depth in the dominator tree
    let mut depth1 = depth_in_dominator_tree(finger1, idom);
    let mut depth2 = depth_in_dominator_tree(finger2, idom);

    let mut iterations = 0;
    const MAX_ITERATIONS: usize = 1000;

    while depth1 > depth2 {
        let Some(next) = idom.get(&finger1).copied().flatten() else {
            return finger1; // Graceful fallback
        };
        finger1 = next;
        depth1 -= 1;
        iterations += 1;
        if iterations > MAX_ITERATIONS {
            return finger1; // Graceful fallback on pathological CFG
        }
    }
    while depth2 > depth1 {
        let Some(next) = idom.get(&finger2).copied().flatten() else {
            return finger1; // Graceful fallback
        };
        finger2 = next;
        depth2 -= 1;
        iterations += 1;
        if iterations > MAX_ITERATIONS {
            return finger1; // Graceful fallback on pathological CFG
        }
    }

    // Move both fingers up until they meet
    while finger1 != finger2 {
        let (Some(next1), Some(next2)) = (
            idom.get(&finger1).copied().flatten(),
            idom.get(&finger2).copied().flatten(),
        ) else {
            return finger1; // Graceful fallback
        };
        finger1 = next1;
        finger2 = next2;
        iterations += 1;
        if iterations > MAX_ITERATIONS {
            return finger1; // Graceful fallback on pathological CFG
        }
    }

    finger1
}

/// Get the depth of a block in the dominator tree.
///
/// Uses an iteration counter instead of a `BTreeSet` for cycle detection.
/// A dominator tree over N blocks has at most N nodes, so exceeding that
/// depth means we hit a cycle (e.g. entry dominating itself).
fn depth_in_dominator_tree(block: BlockId, idom: &BTreeMap<BlockId, Option<BlockId>>) -> usize {
    let max_depth = idom.len();
    let mut depth = 1; // Count the block itself
    let mut current = idom.get(&block).copied().flatten();

    while let Some(idom_block) = current {
        if idom_block == block || depth >= max_depth {
            break;
        }
        depth += 1;
        current = idom.get(&idom_block).copied().flatten();
    }
    depth
}

/// Get blocks in reverse post-order.
///
/// Reverse post-order guarantees that when processing a block,
/// all its successors have already been processed.
fn reverse_post_order(cfg: &Cfg) -> Vec<BlockId> {
    let mut visited = BTreeSet::new();
    let mut order = Vec::new();

    // Start from entry block
    let entry_id = cfg.entry_block().map(|b| b.id);
    if let Some(entry) = entry_id {
        dfs_post_order(cfg, entry, &mut visited, &mut order);
    }

    order.reverse();
    order
}

/// DFS post-order traversal helper.
fn dfs_post_order(
    cfg: &Cfg,
    block: BlockId,
    visited: &mut BTreeSet<BlockId>,
    order: &mut Vec<BlockId>,
) {
    if visited.contains(&block) {
        return;
    }
    visited.insert(block);

    // Visit successors first
    for &succ in cfg.successors(block) {
        dfs_post_order(cfg, succ, visited, order);
    }

    // Add block after visiting successors (post-order)
    order.push(block);
}

/// Build the dominator tree from immediate dominator relationships.
///
/// The dominator tree has edges from each block to its immediate dominator.
/// This creates a tree rooted at the entry block.
fn build_dominator_tree(
    idom: &BTreeMap<BlockId, Option<BlockId>>,
) -> BTreeMap<BlockId, Vec<BlockId>> {
    let mut tree: BTreeMap<BlockId, Vec<BlockId>> = BTreeMap::new();

    // Initialize empty children lists
    for &block in idom.keys() {
        tree.entry(block).or_default();
    }

    // Build parent -> children mapping
    for (&block, &opt_idom) in idom {
        if let Some(idom_block) = opt_idom {
            if idom_block != block {
                // Don't add entry as its own child
                tree.entry(idom_block).or_default().push(block);
            }
        }
    }

    tree
}

/// Compute dominance frontiers for φ node insertion.
///
/// A block n is in the dominance frontier of block d if:
/// - d dominates a predecessor of n
/// - d does NOT strictly dominate n
///
/// Intuitively: this is where control flow from d "merges" with other paths.
fn compute_df(
    cfg: &Cfg,
    idom: &BTreeMap<BlockId, Option<BlockId>>,
) -> BTreeMap<BlockId, BTreeSet<BlockId>> {
    let mut df: BTreeMap<BlockId, BTreeSet<BlockId>> = BTreeMap::new();

    // Initialize empty sets
    for block in cfg.blocks() {
        df.insert(block.id, BTreeSet::new());
    }

    // For each block with multiple predecessors
    for block in cfg.blocks() {
        let predecessors = cfg.predecessors(block.id);

        if predecessors.len() < 2 {
            continue; // Skip single-predecessor blocks
        }

        // For each predecessor
        for &runner in predecessors {
            // Walk up the dominator tree from runner
            let mut current = runner;
            while let Some(&Some(idom_block)) = idom.get(&current) {
                if Some(idom_block) == idom.get(&block.id).copied().flatten() {
                    // Reached the block's immediate dominator - stop
                    break;
                }

                // Add block to current's dominance frontier
                df.entry(current).or_default().insert(block.id);

                // Continue walking up
                current = idom_block;
            }
        }
    }

    df
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decompiler::cfg::{BasicBlock, BlockId, Terminator};

    #[test]
    fn test_dominance_empty_cfg() {
        let cfg = Cfg::new();
        let dominance = compute(&cfg);

        assert!(dominance.idom.is_empty());
        assert!(dominance.dominator_tree.is_empty());
        assert!(dominance.dominance_frontier.is_empty());
    }

    #[test]
    fn test_dominance_single_block() {
        let mut cfg = Cfg::new();
        let block = BasicBlock::new(BlockId(0), 0, 0, 0..0, Terminator::Return);
        cfg.add_block(block);

        let dominance = compute(&cfg);

        // Entry dominates itself only
        assert_eq!(dominance.idom(BlockId::ENTRY), None);
    }

    #[test]
    fn test_dominance_linear_chain() {
        // Build: 0 -> 1 -> 2
        let cfg = create_linear_cfg(3);
        let dominance = compute(&cfg);

        // In a linear chain, idom(1) = 0, idom(2) = 1
        assert_eq!(dominance.idom(BlockId(1)), Some(BlockId(0)));
        assert_eq!(dominance.idom(BlockId(2)), Some(BlockId(1)));

        // Block 0 strictly dominates 1 and 2
        assert!(dominance.strictly_dominates(BlockId(0), BlockId(1)));
        assert!(dominance.strictly_dominates(BlockId(0), BlockId(2)));

        // Block 1 strictly dominates 2
        assert!(dominance.strictly_dominates(BlockId(1), BlockId(2)));
    }

    #[test]
    fn test_dominance_diamond() {
        // Build diamond: entry -> (left, right) -> exit
        let cfg = create_diamond_cfg();
        let dominance = compute(&cfg);

        // Entry dominates all blocks
        assert!(dominance.strictly_dominates(BlockId::ENTRY, BlockId(1)));
        assert!(dominance.strictly_dominates(BlockId::ENTRY, BlockId(2)));
        assert!(dominance.strictly_dominates(BlockId::ENTRY, BlockId(3)));

        // idom of exit (3) is entry (0) since both paths merge there
        assert_eq!(dominance.idom(BlockId(3)), Some(BlockId(0)));
    }

    #[test]
    fn test_dominator_tree_structure() {
        let cfg = create_diamond_cfg();
        let dominance = compute(&cfg);

        // Entry should have children (it dominates all other blocks)
        let entry_children = dominance.children(BlockId::ENTRY);
        assert!(!entry_children.is_empty());
    }

    fn create_linear_cfg(count: usize) -> Cfg {
        let mut cfg = Cfg::new();
        for i in 0..count {
            let block = BasicBlock::new(
                BlockId(i),
                i,
                i + 1,
                i..(i + 1),
                if i < count - 1 {
                    Terminator::Jump {
                        target: BlockId(i + 1),
                    }
                } else {
                    Terminator::Return
                },
            );
            cfg.add_block(block);

            if i > 0 {
                cfg.add_edge(
                    BlockId(i - 1),
                    BlockId(i),
                    crate::decompiler::cfg::EdgeKind::Unconditional,
                );
            }
        }
        cfg
    }

    fn create_diamond_cfg() -> Cfg {
        let mut cfg = Cfg::new();

        // Entry - branches to left or right
        let entry = BasicBlock::new(
            BlockId::ENTRY,
            0,
            1,
            0..1,
            Terminator::Branch {
                then_target: BlockId(1),
                else_target: BlockId(2),
            },
        );
        cfg.add_block(entry);

        // Left branch
        let left = BasicBlock::new(
            BlockId(1),
            1,
            2,
            1..2,
            Terminator::Jump { target: BlockId(3) },
        );
        cfg.add_block(left);
        cfg.add_edge(
            BlockId::ENTRY,
            BlockId(1),
            crate::decompiler::cfg::EdgeKind::Unconditional,
        );

        // Right branch
        let right = BasicBlock::new(
            BlockId(2),
            2,
            3,
            2..3,
            Terminator::Jump { target: BlockId(3) },
        );
        cfg.add_block(right);
        cfg.add_edge(
            BlockId::ENTRY,
            BlockId(2),
            crate::decompiler::cfg::EdgeKind::Unconditional,
        );

        // Exit
        let exit = BasicBlock::new(BlockId(3), 3, 4, 3..4, Terminator::Return);
        cfg.add_block(exit);
        cfg.add_edge(
            BlockId(1),
            BlockId(3),
            crate::decompiler::cfg::EdgeKind::Unconditional,
        );
        cfg.add_edge(
            BlockId(2),
            BlockId(3),
            crate::decompiler::cfg::EdgeKind::Unconditional,
        );

        cfg
    }
}
