//! CFG graph queries used by structural control-flow recovery.

use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};

use crate::decompiler::cfg::{BasicBlock, BlockId, Cfg, EdgeKind, Terminator};

use super::StructCtx;

impl<'a> StructCtx<'a> {
    /// Find the merge of two branch arms: the closest real join that
    /// post-dominates both entries.
    pub(super) fn find_merge(&self, then_target: BlockId, else_target: BlockId) -> Option<BlockId> {
        let from_then = self.shortest_distances(then_target);
        let from_else = self.shortest_distances(else_target);
        let explicit_leave_merge = [
            (then_target, from_else.get(&then_target).copied()),
            (else_target, from_then.get(&else_target).copied()),
        ]
        .into_iter()
        .filter_map(|(target, distance)| {
            (self.leave_targets.contains(&target)).then_some((distance?, target))
        })
        .min();
        if let Some((_, target)) = explicit_leave_merge {
            return Some(target);
        }
        from_then
            .iter()
            .filter_map(|(block, then_distance)| {
                let else_distance = from_else.get(block)?;
                (self.cfg.predecessors(*block).len() >= 2
                    && self.postdominates(*block, then_target)
                    && self.postdominates(*block, else_target))
                .then_some((
                    (*then_distance).max(*else_distance),
                    *then_distance + *else_distance,
                    *block,
                ))
            })
            .min()
            .map(|(_, _, block)| block)
    }

    fn postdominates(&self, candidate: BlockId, block: BlockId) -> bool {
        self.postdominators
            .get(&block)
            .is_some_and(|postdominators| postdominators.contains(&candidate))
    }

    pub(super) fn shortest_distances(&self, start: BlockId) -> BTreeMap<BlockId, usize> {
        let mut distances = BTreeMap::from([(start, 0)]);
        let mut queue = VecDeque::from([start]);
        while let Some(block) = queue.pop_front() {
            let distance = distances[&block];
            for successor in self.cfg.successors(block) {
                if distances.contains_key(successor) {
                    continue;
                }
                distances.insert(*successor, distance + 1);
                queue.push_back(*successor);
            }
        }
        distances
    }

    /// All blocks reachable from `start` (inclusive) via successor edges.
    pub(super) fn reachable(&self, start: BlockId) -> BTreeSet<BlockId> {
        let mut seen = BTreeSet::new();
        let mut stack = vec![start];
        while let Some(b) = stack.pop() {
            if !seen.insert(b) {
                continue;
            }
            if let Some(block) = self.cfg.block(b) {
                for successor in block.terminator.successors() {
                    stack.push(successor);
                }
            }
        }
        seen
    }

    /// Compute the standard natural-loop node set for all back-edges entering
    /// `header` by walking predecessor edges from each dominated latch.
    pub(super) fn natural_loop_blocks(&self, header: BlockId) -> HashSet<BlockId> {
        let mut members = HashSet::from([header]);
        let mut stack = Vec::new();
        for predecessor in self.cfg.predecessors(header) {
            if *predecessor != header
                && self.ssa.dominance.strictly_dominates(header, *predecessor)
                && members.insert(*predecessor)
            {
                stack.push(*predecessor);
            }
        }
        while let Some(block) = stack.pop() {
            for predecessor in self.cfg.predecessors(block) {
                if members.insert(*predecessor) && *predecessor != header {
                    stack.push(*predecessor);
                }
            }
        }
        members
    }

    pub(super) fn closest_loop_merge(
        &self,
        then_target: BlockId,
        else_target: BlockId,
        header: BlockId,
        members: &HashSet<BlockId>,
    ) -> Option<BlockId> {
        let then_distances = self.loop_distances(then_target, header, members);
        let else_distances = self.loop_distances(else_target, header, members);
        let common_postdominators =
            self.loop_common_postdominators(then_target, else_target, header, members);
        then_distances
            .iter()
            .filter_map(|(block, then_distance)| {
                if *block == header || !common_postdominators.contains(block) {
                    return None;
                }
                else_distances.get(block).map(|else_distance| {
                    (
                        (*then_distance).max(*else_distance),
                        *then_distance + *else_distance,
                        block.0,
                        *block,
                    )
                })
            })
            .min()
            .map(|(_, _, _, block)| block)
    }

    fn loop_common_postdominators(
        &self,
        then_target: BlockId,
        else_target: BlockId,
        header: BlockId,
        members: &HashSet<BlockId>,
    ) -> HashSet<BlockId> {
        let mut ordered: Vec<_> = members.iter().copied().collect();
        ordered.sort_unstable();
        let reverse_ids: BTreeMap<_, _> = ordered
            .iter()
            .enumerate()
            .map(|(index, block)| (*block, BlockId(index + 1)))
            .collect();
        let mut reverse = Cfg::new();
        reverse.add_block(BasicBlock::new(
            BlockId::ENTRY,
            0,
            0,
            0..0,
            Terminator::Unknown,
        ));
        for reverse_id in reverse_ids.values() {
            reverse.add_block(BasicBlock::new(
                *reverse_id,
                reverse_id.0,
                reverse_id.0,
                0..0,
                Terminator::Unknown,
            ));
        }
        let Some(reverse_header) = reverse_ids.get(&header).copied() else {
            return HashSet::new();
        };
        reverse.add_edge(BlockId::ENTRY, reverse_header, EdgeKind::Unconditional);
        for source in &ordered {
            for target in self.cfg.successors(*source) {
                let (Some(reverse_source), Some(reverse_target)) =
                    (reverse_ids.get(source), reverse_ids.get(target))
                else {
                    continue;
                };
                reverse.add_edge(*reverse_target, *reverse_source, EdgeKind::Unconditional);
            }
        }
        let dominance = crate::decompiler::cfg::ssa::compute(&reverse);
        let dominators_of = |target: BlockId| {
            let mut result = HashSet::new();
            let Some(mut current) = reverse_ids.get(&target).copied() else {
                return result;
            };
            loop {
                if current != BlockId::ENTRY {
                    result.insert(ordered[current.0 - 1]);
                }
                let Some(parent) = dominance.idom(current) else {
                    break;
                };
                current = parent;
            }
            result
        };
        let then_dominators = dominators_of(then_target);
        let else_dominators = dominators_of(else_target);
        then_dominators
            .intersection(&else_dominators)
            .copied()
            .collect()
    }

    fn loop_distances(
        &self,
        start: BlockId,
        header: BlockId,
        members: &HashSet<BlockId>,
    ) -> BTreeMap<BlockId, usize> {
        let mut distances = BTreeMap::new();
        let mut queue = VecDeque::from([(start, 0usize)]);
        while let Some((block, distance)) = queue.pop_front() {
            if !members.contains(&block) || distances.contains_key(&block) {
                continue;
            }
            distances.insert(block, distance);
            if block == header {
                continue;
            }
            for successor in self.cfg.successors(block) {
                queue.push_back((*successor, distance + 1));
            }
        }
        distances
    }

    pub(super) fn terminator(&self, bid: BlockId) -> Terminator {
        self.cfg
            .block(bid)
            .map(|block| block.terminator.clone())
            .unwrap_or(Terminator::Unknown)
    }
}
