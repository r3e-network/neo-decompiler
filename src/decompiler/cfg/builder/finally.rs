use std::collections::{BTreeMap, BTreeSet};

use crate::instruction::OpCode;

use super::super::basic_block::{BasicBlock, BlockId, Terminator};
use super::CfgBuilder;

#[derive(Debug)]
struct TryRegion {
    try_offset: usize,
    body_start: usize,
    body_layout_end: usize,
    catch_layout_range: Option<(usize, usize)>,
    finally_offset: Option<usize>,
    endfinally_offset: Option<usize>,
    parent: Option<usize>,
    endtry_blocks: Vec<(BlockId, BlockId)>,
    has_self_continuation: bool,
}

impl TryRegion {
    fn lexically_contains(&self, offset: usize) -> bool {
        (offset >= self.body_start && offset < self.body_layout_end)
            || self
                .catch_layout_range
                .is_some_and(|(start, end)| offset >= start && offset < end)
            || self
                .finally_offset
                .zip(self.endfinally_offset)
                .is_some_and(|(start, end)| offset >= start && offset <= end)
    }
}

impl<'a> CfgBuilder<'a> {
    pub(super) fn apply_finally_routing(&self, blocks: &mut [BasicBlock]) {
        let leaders: Vec<_> = self.leaders.iter().copied().collect();
        let mut regions = self.collect_try_regions();
        self.assign_endfinally_offsets(&mut regions);
        Self::assign_region_parents(&mut regions);
        let owners = self.assign_endtry_owners(&regions);
        self.refine_catch_continuations(&mut regions, &owners);

        for (index, instruction) in self.instructions.iter().enumerate() {
            if !matches!(instruction.opcode, OpCode::Endtry | OpCode::EndtryL) {
                continue;
            }
            let Some(&region_index) = owners.get(&instruction.offset) else {
                continue;
            };
            let Some(target_offset) = self.jump_target(index, instruction) else {
                continue;
            };
            let endtry_block = self.offset_to_block_id(instruction.offset, &leaders);
            let continuation = self.offset_to_block_id(target_offset, &leaders);
            let Some(finally_offset) = regions[region_index].finally_offset else {
                let natural_offset = regions[region_index].catch_layout_range.map(|(_, end)| end);
                if let Some(Terminator::EndTry { nonlocal, .. }) = blocks
                    .iter_mut()
                    .find(|block| block.id == endtry_block)
                    .map(|block| &mut block.terminator)
                {
                    *nonlocal = natural_offset.is_some_and(|natural| target_offset != natural);
                }
                continue;
            };
            let finally_target = self.offset_to_block_id(finally_offset, &leaders);
            if let Some(block) = blocks.iter_mut().find(|block| block.id == endtry_block) {
                block.terminator = Terminator::EndTryFinally {
                    continuation,
                    finally_target,
                    nonlocal: false,
                };
            }
            if target_offset == instruction.offset {
                regions[region_index].has_self_continuation = true;
                continue;
            }
            regions[region_index]
                .endtry_blocks
                .push((endtry_block, continuation));
        }

        for region in &regions {
            let Some(endfinally_offset) = region.endfinally_offset else {
                continue;
            };
            let normal_continuations: Vec<_> = region
                .endtry_blocks
                .iter()
                .map(|(_, continuation)| *continuation)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            if normal_continuations.is_empty() && region.has_self_continuation {
                continue;
            }
            let natural_offset = self
                .offset_to_index
                .get(&endfinally_offset)
                .and_then(|index| self.instructions.get(index + 1))
                .map(|instruction| instruction.offset);
            for (endtry_block, continuation) in &region.endtry_blocks {
                let continuation_offset = blocks
                    .iter()
                    .find(|block| block.id == *continuation)
                    .map(|block| block.start_offset);
                let nonlocal = continuation_offset != natural_offset;
                if let Some(Terminator::EndTryFinally {
                    nonlocal: endtry_nonlocal,
                    ..
                }) = blocks
                    .iter_mut()
                    .find(|block| block.id == *endtry_block)
                    .map(|block| &mut block.terminator)
                {
                    *endtry_nonlocal = nonlocal;
                }
            }
            let endfinally_block = self.offset_to_block_id(endfinally_offset, &leaders);
            if let Some(block) = blocks.iter_mut().find(|block| block.id == endfinally_block) {
                block.terminator = Terminator::EndFinally {
                    normal_continuations,
                };
            }
        }
    }

    fn collect_try_regions(&self) -> Vec<TryRegion> {
        let mut regions = Vec::new();
        for (index, instruction) in self.instructions.iter().enumerate() {
            if !matches!(instruction.opcode, OpCode::Try | OpCode::TryL) {
                continue;
            }
            let Some((catch_target, finally_target)) = self.try_targets(index, instruction) else {
                continue;
            };
            if catch_target
                .is_some_and(|catch| finally_target.is_some_and(|finally| catch >= finally))
            {
                continue;
            }
            let body_start = self
                .instruction_end_offset(index)
                .unwrap_or_else(|| self.end_offset());
            let handler_start = catch_target
                .into_iter()
                .chain(finally_target)
                .min()
                .unwrap_or_else(|| self.end_offset());
            let body_end = self.local_resume_bound(body_start, handler_start);
            let catch_layout_range = catch_target.map(|catch| {
                let handler_end = finally_target
                    .or_else(|| self.catch_continuation(catch, body_start, body_end))
                    .unwrap_or_else(|| self.end_offset());
                (catch, handler_end.max(catch))
            });
            regions.push(TryRegion {
                try_offset: instruction.offset,
                body_start,
                body_layout_end: handler_start,
                catch_layout_range,
                finally_offset: finally_target,
                endfinally_offset: None,
                parent: None,
                endtry_blocks: Vec::new(),
                has_self_continuation: false,
            });
        }
        regions
    }

    fn assign_endfinally_offsets(&self, regions: &mut [TryRegion]) {
        let mut starts: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (index, region) in regions.iter().enumerate() {
            if let Some(finally_offset) = region.finally_offset {
                starts.entry(finally_offset).or_default().push(index);
            }
        }

        let mut active = Vec::new();
        for instruction in self.instructions {
            if let Some(mut starting) = starts.remove(&instruction.offset) {
                starting.sort_by_key(|index| regions[*index].try_offset);
                active.extend(starting);
            }
            if instruction.opcode == OpCode::Endfinally {
                if let Some(region_index) = active.pop() {
                    regions[region_index].endfinally_offset = Some(instruction.offset);
                }
            }
        }
    }

    fn assign_region_parents(regions: &mut [TryRegion]) {
        let parents: Vec<_> = regions
            .iter()
            .enumerate()
            .map(|(child_index, child)| {
                regions
                    .iter()
                    .enumerate()
                    .filter(|(parent_index, parent)| {
                        *parent_index != child_index
                            && parent.try_offset < child.try_offset
                            && parent.lexically_contains(child.try_offset)
                    })
                    .max_by_key(|(_, parent)| parent.try_offset)
                    .map(|(parent_index, _)| parent_index)
            })
            .collect();
        for (region, parent) in regions.iter_mut().zip(parents) {
            region.parent = parent;
        }
    }

    fn assign_endtry_owners(&self, regions: &[TryRegion]) -> BTreeMap<usize, usize> {
        let endtry_targets: BTreeMap<_, _> = self
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, instruction)| {
                matches!(instruction.opcode, OpCode::Endtry | OpCode::EndtryL)
            })
            .filter_map(|(index, instruction)| {
                self.jump_target(index, instruction)
                    .map(|target| (instruction.offset, target))
            })
            .collect();
        let endtry_offsets: BTreeSet<_> = endtry_targets.keys().copied().collect();
        let targeted_endtries: BTreeSet<_> = endtry_targets
            .values()
            .filter(|target| endtry_offsets.contains(target))
            .copied()
            .collect();
        let mut owners = BTreeMap::new();
        let mut ambiguous = BTreeSet::new();

        for root in endtry_offsets.difference(&targeted_endtries).copied() {
            let Some((root_owner, _)) = regions
                .iter()
                .enumerate()
                .filter(|(_, region)| region.lexically_contains(root))
                .max_by_key(|(_, region)| region.try_offset)
            else {
                continue;
            };
            if !Self::record_endtry_owner(&mut owners, &mut ambiguous, root, root_owner) {
                continue;
            }

            let mut offset = root;
            let mut owner = root_owner;
            let mut seen = BTreeSet::new();
            while seen.insert(offset) {
                let Some(&target) = endtry_targets.get(&offset) else {
                    break;
                };
                if !endtry_offsets.contains(&target) {
                    break;
                }
                let Some(parent) = regions[owner].parent else {
                    break;
                };
                if !regions[parent].lexically_contains(target)
                    || !Self::record_endtry_owner(&mut owners, &mut ambiguous, target, parent)
                {
                    break;
                }
                offset = target;
                owner = parent;
            }
        }

        for offset in ambiguous {
            owners.remove(&offset);
        }
        owners
    }

    fn refine_catch_continuations(
        &self,
        regions: &mut [TryRegion],
        owners: &BTreeMap<usize, usize>,
    ) {
        for (region_index, region) in regions.iter_mut().enumerate() {
            if region.finally_offset.is_some() {
                continue;
            }
            let Some((catch_start, provisional_end)) = region.catch_layout_range else {
                continue;
            };
            let mut target_counts = BTreeMap::<usize, usize>::new();
            for (instruction_index, instruction) in self.instructions.iter().enumerate() {
                if instruction.offset < catch_start
                    || instruction.offset >= provisional_end
                    || !matches!(instruction.opcode, OpCode::Endtry | OpCode::EndtryL)
                    || owners.get(&instruction.offset) != Some(&region_index)
                {
                    continue;
                }
                if let Some(target) = self.jump_target(instruction_index, instruction) {
                    *target_counts.entry(target).or_default() += 1;
                }
            }
            if target_counts.is_empty() || target_counts.contains_key(&provisional_end) {
                continue;
            }
            let natural = target_counts
                .into_iter()
                .max_by_key(|(target, count)| (*count, std::cmp::Reverse(*target)))
                .map(|(target, _)| target)
                .expect("non-empty target counts");
            region.catch_layout_range = Some((catch_start, natural.max(catch_start)));
        }
    }

    fn record_endtry_owner(
        owners: &mut BTreeMap<usize, usize>,
        ambiguous: &mut BTreeSet<usize>,
        offset: usize,
        owner: usize,
    ) -> bool {
        if ambiguous.contains(&offset) {
            return false;
        }
        if owners
            .get(&offset)
            .is_some_and(|existing| *existing != owner)
        {
            owners.remove(&offset);
            ambiguous.insert(offset);
            return false;
        }
        owners.insert(offset, owner);
        true
    }

    fn local_resume_bound(&self, start: usize, end: usize) -> usize {
        self.instructions
            .iter()
            .enumerate()
            .filter(|(_, instruction)| {
                instruction.offset >= start
                    && instruction.offset < end
                    && matches!(instruction.opcode, OpCode::Endtry | OpCode::EndtryL)
            })
            .filter_map(|(index, instruction)| {
                self.jump_target(index, instruction)
                    .filter(|target| *target > instruction.offset && *target < end)
            })
            .min()
            .unwrap_or(end)
    }

    fn catch_continuation(
        &self,
        catch_target: usize,
        body_start: usize,
        body_end: usize,
    ) -> Option<usize> {
        self.instructions
            .iter()
            .enumerate()
            .filter(|(_, instruction)| {
                instruction.offset >= body_start
                    && instruction.offset < body_end
                    && matches!(instruction.opcode, OpCode::Endtry | OpCode::EndtryL)
            })
            .filter_map(|(index, instruction)| self.jump_target(index, instruction))
            .filter(|target| *target > catch_target)
            .min()
    }
}
