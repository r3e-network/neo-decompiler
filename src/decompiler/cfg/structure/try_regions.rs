//! Exception-region recovery for the CFG structurer.

use std::collections::{BTreeSet, HashSet};

use crate::decompiler::cfg::{BlockId, EdgeKind, Terminator};
use crate::decompiler::ir::{Block as IrBlock, ControlFlow, Stmt};

use super::analysis::resolve_leave_target_cfg;
use super::{ssa_var_name, SsaVariable, StructCtx};

impl<'a> StructCtx<'a> {
    /// Recover a `try` / `catch` / `finally` from a `TryEntry` terminator.
    pub(super) fn handle_try(
        &self,
        out: &mut IrBlock,
        bid: BlockId,
        body_target: BlockId,
        catch_target: Option<BlockId>,
        finally_target: Option<BlockId>,
        visited: &mut HashSet<BlockId>,
    ) -> Option<BlockId> {
        let end_try = self.find_endtry_for_arms(bid, body_target, catch_target);
        let end_try_nonlocal = end_try.is_some_and(|(end_try, _)| {
            matches!(
                self.terminator(end_try),
                Terminator::EndTry { nonlocal: true, .. }
                    | Terminator::EndTryFinally { nonlocal: true, .. }
            )
        });
        let end_try_is_pure = end_try.is_some_and(|(end_try, _)| {
            !end_try_nonlocal
                && self
                    .cfg
                    .block(end_try)
                    .is_some_and(|block| block.instruction_range.len() == 1)
        });
        let continuation = end_try.map(|(_, continuation)| {
            if end_try_nonlocal {
                resolve_leave_target_cfg(self.cfg, continuation)
            } else {
                continuation
            }
        });

        let mut body_stop = HashSet::new();
        if let Some(c) = catch_target {
            body_stop.insert(c);
        }
        if let Some(f) = finally_target {
            body_stop.insert(f);
        }
        if end_try_is_pure {
            if let Some((end_try, _)) = end_try {
                body_stop.insert(end_try);
            }
        }
        let try_body = self.structure_set_edge_region(bid, body_target, &body_stop, visited);

        let catch_body = catch_target.map(|c| {
            let mut stop = HashSet::new();
            if let Some(f) = finally_target {
                stop.insert(f);
            }
            if end_try_is_pure {
                if let Some((end_try, _)) = end_try {
                    stop.insert(end_try);
                }
            }
            self.structure_set_edge_region(bid, c, &stop, visited)
        });
        let finally_body = finally_target.map(|f| {
            let mut stop = HashSet::new();
            if end_try_is_pure {
                if let Some((end_try, _)) = end_try {
                    stop.insert(end_try);
                }
            }
            self.structure_set_edge_region(bid, f, &stop, visited)
        });

        let catch_var = catch_target.and_then(|target| self.catch_variable(target));
        out.push(Stmt::ControlFlow(Box::new(ControlFlow::try_catch(
            try_body,
            catch_var,
            catch_body,
            finally_body,
        ))));
        if end_try_is_pure {
            if let Some((end_try, _)) = end_try {
                if visited.insert(end_try) {
                    self.emit_body(out, end_try);
                }
            }
        }
        continuation
    }

    fn catch_variable(&self, target: BlockId) -> Option<String> {
        let predecessors = self.cfg.predecessors(target);
        predecessors
            .iter()
            .any(|predecessor| {
                self.cfg.edge_kind(*predecessor, target) == Some(EdgeKind::Exception)
            })
            .then(|| ssa_var_name(&SsaVariable::exception_payload(target), self.source_names))
    }

    fn endtries_for_region(
        &self,
        owner_entry: BlockId,
        start: BlockId,
    ) -> Vec<(BlockId, BlockId, bool)> {
        let mut found = Vec::new();
        let mut seen = BTreeSet::new();
        let mut stack = vec![(start, 0usize)];
        while let Some((block_id, depth)) = stack.pop() {
            if depth > self.cfg.block_count() || !seen.insert((block_id, depth)) {
                continue;
            }
            let Some(block) = self.cfg.block(block_id) else {
                continue;
            };
            match block.terminator.clone() {
                Terminator::TryEntry {
                    body_target,
                    catch_target,
                    finally_target,
                } => {
                    if block_id == owner_entry {
                        continue;
                    }
                    stack.push((body_target, depth + 1));
                    if let Some(target) = catch_target {
                        stack.push((target, depth + 1));
                    }
                    if let Some(target) = finally_target {
                        stack.push((target, depth + 1));
                    }
                }
                Terminator::EndTry {
                    continuation,
                    nonlocal,
                }
                | Terminator::EndTryFinally {
                    continuation,
                    nonlocal,
                    ..
                } => {
                    if depth == 0 {
                        found.push((block_id, continuation, nonlocal));
                    } else {
                        stack.push((continuation, depth - 1));
                    }
                }
                _ => {
                    for successor in block.terminator.successors() {
                        stack.push((successor, depth));
                    }
                }
            }
        }
        found.sort_by_key(|(block, _, _)| *block);
        found.dedup();
        found
    }

    fn find_endtry_for_arms(
        &self,
        owner_entry: BlockId,
        body_target: BlockId,
        catch_target: Option<BlockId>,
    ) -> Option<(BlockId, BlockId)> {
        let mut endtries = self.endtries_for_region(owner_entry, body_target);
        if let Some(target) = catch_target {
            endtries.extend(self.endtries_for_region(owner_entry, target));
        }
        endtries
            .iter()
            .find(|(_, _, nonlocal)| !*nonlocal)
            .or_else(|| endtries.first())
            .map(|(block, continuation, _)| (*block, *continuation))
    }

    pub(super) fn try_has_nonlocal_leave(
        &self,
        owner_entry: BlockId,
        body_target: BlockId,
        catch_target: Option<BlockId>,
    ) -> bool {
        self.endtries_for_region(owner_entry, body_target)
            .into_iter()
            .chain(
                catch_target
                    .into_iter()
                    .flat_map(|target| self.endtries_for_region(owner_entry, target)),
            )
            .any(|(_, _, nonlocal)| nonlocal)
    }
}
