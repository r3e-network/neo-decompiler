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
            .map(Vec::as_slice)
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
#[must_use]
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
    let rpo_positions = rpo
        .iter()
        .enumerate()
        .map(|(position, &block)| (block, position))
        .collect::<BTreeMap<_, _>>();
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
            let new_idom = intersect_dominators(cfg, block_id, &idom, &rpo_positions);

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
    rpo_positions: &BTreeMap<BlockId, usize>,
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
                    Some(_) => find_common_dominator(current, *pred, idom, rpo_positions),
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
/// Immediate dominators precede their children in reverse post-order, so move
/// only the later finger. This avoids rescanning both full ancestor chains to
/// compute their depths, and handles valid chains of any length.
fn find_common_dominator(
    mut finger1: BlockId,
    mut finger2: BlockId,
    idom: &BTreeMap<BlockId, Option<BlockId>>,
    rpo_positions: &BTreeMap<BlockId, usize>,
) -> Option<BlockId> {
    while finger1 != finger2 {
        let position1 = *rpo_positions.get(&finger1)?;
        let position2 = *rpo_positions.get(&finger2)?;
        let (finger, position) = if position1 > position2 {
            (&mut finger1, position1)
        } else {
            (&mut finger2, position2)
        };
        let parent = idom_parent(idom, *finger)?;
        // Reject inconsistent parent links instead of inventing a dominator
        // or looping. The rank strictly decreases on every successful step.
        if *rpo_positions.get(&parent)? >= position {
            return None;
        }
        *finger = parent;
    }

    Some(finger1)
}

/// The parent of `b` in the idom map — i.e. `idom.get(b).flatten()` — except
/// for the entry block, whose idom is initialised to `Some(b)` as a sentinel
/// and is treated here as "no parent" so walks up the tree terminate cleanly
/// instead of spinning on the entry's self-idom.
fn idom_parent(idom: &BTreeMap<BlockId, Option<BlockId>>, b: BlockId) -> Option<BlockId> {
    let parent = idom.get(&b).copied().flatten();
    match parent {
        Some(p) if p == b => None,
        other => other,
    }
}

/// Get blocks in reverse post-order.
///
/// Reverse post-order visits predecessors before successors except for back
/// edges, making forward propagation converge quickly.
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

/// Iterative DFS post-order traversal.
///
/// Uses an explicit stack instead of recursion to avoid stack overflow
/// on deeply nested CFGs produced by malformed bytecode.
fn dfs_post_order(
    cfg: &Cfg,
    entry: BlockId,
    visited: &mut BTreeSet<BlockId>,
    order: &mut Vec<BlockId>,
) {
    // Each frame tracks the block and how many successors have been visited.
    let mut stack: Vec<(BlockId, usize)> = Vec::new();

    if !visited.insert(entry) {
        return;
    }
    stack.push((entry, 0));

    while let Some((block, next_idx)) = stack.last_mut() {
        let successors = cfg.successors(*block);
        if *next_idx < successors.len() {
            let succ = successors[*next_idx];
            *next_idx += 1;
            if visited.insert(succ) {
                stack.push((succ, 0));
            }
        } else {
            let (block, _) = stack.pop().expect("stack is non-empty");
            order.push(block);
        }
    }
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

    // Cooper-Harvey-Kennedy dominance frontier algorithm:
    // For each join point (block with ≥2 predecessors), walk up the
    // dominator tree from each predecessor until reaching the join
    // point's immediate dominator, adding the join point to each
    // visited node's DF along the way.
    for block in cfg.blocks() {
        if idom.get(&block.id).copied().flatten().is_none() {
            continue;
        }
        let predecessors = cfg.predecessors(block.id);

        let block_idom = idom_parent(idom, block.id);
        if predecessors.len() < 2 && block_idom.is_some() {
            continue; // Only join points contribute to DFs
        }

        for &pred in predecessors {
            if idom.get(&pred).copied().flatten().is_none() {
                continue;
            }
            let mut runner = pred;
            // Walk up until runner IS the immediate dominator of the join block
            while Some(runner) != block_idom {
                df.entry(runner).or_default().insert(block.id);
                match idom_parent(idom, runner) {
                    Some(next) => runner = next,
                    None => break, // entry block — no further idom
                }
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
    fn deep_branch_merge_is_dominated_only_by_the_entry() {
        // A long left arm and a one-block right arm merge at the exit. The
        // long arm exceeds the former hard-coded 1000-step ancestor limit.
        let left_depth = 1_500;
        let right = BlockId(left_depth + 1);
        let exit = BlockId(left_depth + 2);
        let mut cfg = Cfg::new();
        for index in 0..=left_depth {
            let terminator = if index == 0 {
                Terminator::Branch {
                    then_target: BlockId(1),
                    else_target: right,
                }
            } else {
                Terminator::Jump {
                    target: if index == left_depth {
                        exit
                    } else {
                        BlockId(index + 1)
                    },
                }
            };
            cfg.add_block(BasicBlock::new(
                BlockId(index),
                index,
                index + 1,
                index..index + 1,
                terminator,
            ));
            if index > 0 {
                cfg.add_edge(
                    BlockId(index - 1),
                    BlockId(index),
                    crate::decompiler::cfg::EdgeKind::Unconditional,
                );
            }
        }
        cfg.add_block(BasicBlock::new(
            right,
            right.0,
            right.0 + 1,
            right.0..right.0 + 1,
            Terminator::Jump { target: exit },
        ));
        cfg.add_block(BasicBlock::new(
            exit,
            exit.0,
            exit.0 + 1,
            exit.0..exit.0 + 1,
            Terminator::Return,
        ));
        cfg.add_edge(
            BlockId::ENTRY,
            right,
            crate::decompiler::cfg::EdgeKind::ConditionalFalse,
        );
        cfg.add_edge(
            BlockId(left_depth),
            exit,
            crate::decompiler::cfg::EdgeKind::Unconditional,
        );
        cfg.add_edge(right, exit, crate::decompiler::cfg::EdgeKind::Unconditional);

        let dominance = compute(&cfg);
        assert_eq!(dominance.idom(exit), Some(BlockId::ENTRY));
        assert!(!dominance.strictly_dominates(BlockId(1), exit));
        assert_eq!(dominance.dominance_frontier_vec(BlockId(1)), vec![exit]);
        assert!(dominance.dominance_frontier_vec(BlockId::ENTRY).is_empty());
    }

    #[test]
    fn entry_self_loop_has_itself_in_its_dominance_frontier() {
        let mut cfg = Cfg::new();
        cfg.add_block(BasicBlock::new(
            BlockId::ENTRY,
            0,
            1,
            0..1,
            Terminator::Jump {
                target: BlockId::ENTRY,
            },
        ));
        cfg.add_edge(
            BlockId::ENTRY,
            BlockId::ENTRY,
            crate::decompiler::cfg::EdgeKind::Unconditional,
        );

        let dominance = compute(&cfg);
        assert_eq!(
            dominance.dominance_frontier_vec(BlockId::ENTRY),
            vec![BlockId::ENTRY]
        );
    }

    #[test]
    fn unreachable_predecessor_does_not_contribute_to_dominance_frontiers() {
        let mut cfg = create_linear_cfg(2);
        cfg.add_block(BasicBlock::new(
            BlockId(2),
            2,
            3,
            2..3,
            Terminator::Jump { target: BlockId(1) },
        ));
        cfg.add_edge(
            BlockId(2),
            BlockId(1),
            crate::decompiler::cfg::EdgeKind::Unconditional,
        );

        let dominance = compute(&cfg);
        assert_eq!(dominance.idom(BlockId(1)), Some(BlockId::ENTRY));
        assert_eq!(dominance.idom(BlockId(2)), None);
        assert!(dominance.dominance_frontier_vec(BlockId(2)).is_empty());
    }

    #[test]
    fn cyclic_graphs_match_a_dominator_set_reference() {
        const BLOCKS: usize = 12;
        let label = |index| BlockId(if index == 0 { 0 } else { index * 7 % 11 + 1 });
        for seed in 1_u64..=64 {
            let mut random = seed;
            let mut cfg = Cfg::new();
            for index in 0..BLOCKS {
                random = random
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1);
                let next = label((index + 1).min(BLOCKS - 1));
                let other = label((random >> 32) as usize % BLOCKS);
                cfg.add_block(BasicBlock::new(
                    label(index),
                    index,
                    index + 1,
                    index..index + 1,
                    if index + 1 == BLOCKS {
                        Terminator::Return
                    } else {
                        Terminator::Branch {
                            then_target: next,
                            else_target: other,
                        }
                    },
                ));
                if index + 1 != BLOCKS {
                    cfg.add_edge(
                        label(index),
                        next,
                        crate::decompiler::cfg::EdgeKind::ConditionalTrue,
                    );
                    cfg.add_edge(
                        label(index),
                        other,
                        crate::decompiler::cfg::EdgeKind::ConditionalFalse,
                    );
                }
            }

            // Independent reference: intersect full dominator sets until
            // stable. Every block is reachable through the mandatory chain.
            let all = cfg.blocks().map(|block| block.id).collect::<BTreeSet<_>>();
            let mut sets = all
                .iter()
                .map(|&block| (block, all.clone()))
                .collect::<BTreeMap<_, _>>();
            sets.insert(BlockId::ENTRY, BTreeSet::from([BlockId::ENTRY]));
            loop {
                let mut changed = false;
                for &block in all.iter().filter(|&&block| block != BlockId::ENTRY) {
                    let mut next = all.clone();
                    for pred in cfg.predecessors(block) {
                        next.retain(|candidate| sets[pred].contains(candidate));
                    }
                    next.insert(block);
                    if sets[&block] != next {
                        sets.insert(block, next);
                        changed = true;
                    }
                }
                if !changed {
                    break;
                }
            }

            let actual = compute(&cfg);
            for &block in &all {
                let expected_parent = sets[&block]
                    .iter()
                    .copied()
                    .filter(|&candidate| candidate != block)
                    .max_by_key(|candidate| sets[candidate].len());
                assert_eq!(
                    actual.idom(block),
                    expected_parent,
                    "seed {seed}, {block:?}"
                );
                let expected_frontier = all
                    .iter()
                    .copied()
                    .filter(|&target| {
                        (target == block || !sets[&target].contains(&block))
                            && cfg
                                .predecessors(target)
                                .iter()
                                .any(|pred| sets[pred].contains(&block))
                    })
                    .collect::<BTreeSet<_>>();
                assert_eq!(
                    actual.dominance_frontier[&block], expected_frontier,
                    "seed {seed}, {block:?}"
                );
            }
        }
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

    #[test]
    fn diamond_cfg_dominance_frontier() {
        // Diamond: BB0 -> (BB1, BB2) -> BB3
        // DF(BB0) = {}, DF(BB1) = {BB3}, DF(BB2) = {BB3}, DF(BB3) =
        let cfg = create_diamond_cfg();
        let dominance = compute(&cfg);

        assert!(
            dominance.dominance_frontier_vec(BlockId(0)).is_empty(),
            "DF(BB0) should be empty for diamond entry"
        );
        assert_eq!(
            dominance.dominance_frontier_vec(BlockId(1)),
            vec![BlockId(3)],
            "DF(BB1) should be {{BB3}}"
        );
        assert_eq!(
            dominance.dominance_frontier_vec(BlockId(2)),
            vec![BlockId(3)],
            "DF(BB2) should be {{BB3}}"
        );
        assert!(
            dominance.dominance_frontier_vec(BlockId(3)).is_empty(),
            "DF(BB3) should be empty for diamond exit"
        );
    }

    #[test]
    fn loop_cfg_dominance_frontier() {
        // Loop with pre-header:
        //   BB0 (pre-header) -> BB1
        //   BB1 (loop header) -> branch BB2 (body) / BB3 (exit)
        //   BB2 (body) -> BB1  (back edge)
        //   BB3 (exit) -> return
        //
        // BB1 has 2 predecessors (BB0, BB2) → join point
        // DF(BB0) = {}, DF(BB1) = {BB1}, DF(BB2) = {BB1}, DF(BB3) = {}
        let cfg = create_loop_cfg();
        let dominance = compute(&cfg);

        let df0 = dominance.dominance_frontier_vec(BlockId(0));
        assert!(df0.is_empty(), "DF(BB0) should be empty (pre-header)");

        let df1 = dominance.dominance_frontier_vec(BlockId(1));
        assert_eq!(
            df1,
            vec![BlockId(1)],
            "DF(BB1) should be {{BB1}} (loop header in own DF)"
        );

        let df2 = dominance.dominance_frontier_vec(BlockId(2));
        assert_eq!(df2, vec![BlockId(1)], "DF(BB2) should be {{BB1}}");

        let df3 = dominance.dominance_frontier_vec(BlockId(3));
        assert!(df3.is_empty(), "DF(BB3) should be empty (exit block)");
    }

    /// Regression: a back-edge latch must be dominated by the loop header, so
    /// `compute_loop_headers` recognises the header and the structurer
    /// recovers a `while`. A previous bug caused `depth_in_dominator_tree` /
    /// `find_common_dominator` to spin on the entry's self-idom and return
    /// the entry as the LCA for the latch, hiding every natural loop
    /// (empty `loop_headers`).
    #[test]
    fn loop_latch_is_dominated_by_header_and_header_is_a_loop_header() {
        let cfg = create_loop_cfg();
        let dominance = compute(&cfg);

        // Latch BB2 is reachable from BB1 (header) on every path, so BB1
        // dominates BB2 and idom(BB2) = BB1.
        assert_eq!(
            dominance.idom(BlockId(2)),
            Some(BlockId(1)),
            "loop latch (BB2) must be immediately dominated by the header (BB1)"
        );

        // Strict-domination via the back-edge (BB2 -> BB1) makes BB1 a loop
        // header. Compute it the same way `cfg::structure` does.
        let mut headers = std::collections::HashSet::new();
        for block in cfg.blocks() {
            for pred in cfg.predecessors(block.id) {
                if *pred == block.id || dominance.strictly_dominates(block.id, *pred) {
                    headers.insert(block.id);
                    break;
                }
            }
        }
        assert!(
            headers.contains(&BlockId(1)),
            "BB1 should be detected as a loop header; got {headers:?}"
        );
    }

    fn create_loop_cfg() -> Cfg {
        let mut cfg = Cfg::new();

        // BB0: pre-header, jumps to loop header
        let pre_header = BasicBlock::new(
            BlockId(0),
            0,
            1,
            0..1,
            Terminator::Jump { target: BlockId(1) },
        );
        cfg.add_block(pre_header);

        // BB1: loop header, branch to body (BB2) or exit (BB3)
        let header = BasicBlock::new(
            BlockId(1),
            1,
            2,
            1..2,
            Terminator::Branch {
                then_target: BlockId(2),
                else_target: BlockId(3),
            },
        );
        cfg.add_block(header);
        cfg.add_edge(
            BlockId(0),
            BlockId(1),
            crate::decompiler::cfg::EdgeKind::Unconditional,
        );

        // BB2: loop body, back-edge to header
        let body = BasicBlock::new(
            BlockId(2),
            2,
            3,
            2..3,
            Terminator::Jump { target: BlockId(1) },
        );
        cfg.add_block(body);
        cfg.add_edge(
            BlockId(1),
            BlockId(2),
            crate::decompiler::cfg::EdgeKind::ConditionalTrue,
        );

        // BB3: exit
        let exit = BasicBlock::new(BlockId(3), 3, 4, 3..4, Terminator::Return);
        cfg.add_block(exit);
        cfg.add_edge(
            BlockId(1),
            BlockId(3),
            crate::decompiler::cfg::EdgeKind::ConditionalFalse,
        );

        // Back edge: BB2 -> BB1
        cfg.add_edge(
            BlockId(2),
            BlockId(1),
            crate::decompiler::cfg::EdgeKind::Unconditional,
        );

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
