use std::collections::{BTreeSet, HashSet};

use crate::decompiler::cfg::{BlockId, Terminator};
use crate::decompiler::ir::{Block as IrBlock, ControlFlow, Expr, Stmt};

use super::StructCtx;

impl<'a> StructCtx<'a> {
    /// Emit a loop for a loop-header branch, promoting only an unambiguous
    /// source-level induction shape and retaining `while` otherwise.
    pub(super) fn build_loop(
        &self,
        out: &mut IrBlock,
        bid: BlockId,
        cond: Expr,
        body: &mut IrBlock,
    ) {
        // Backedge phi copies are already at the body tail. Replay the header
        // after them so its effects run before the next condition check.
        self.emit_body_except_condition(body, bid);
        if self.try_promote_for(out, cond.clone(), body) {
            return;
        }
        out.push(Stmt::ControlFlow(Box::new(ControlFlow::while_loop(
            cond,
            std::mem::take(body),
        ))));
    }

    /// For a do-while loop header `header`, find its latch: the back-edge
    /// predecessor that is a `Branch` re-entering `header`. Returns `(latch,
    /// exit)` where `exit` is the latch's other (non-back-edge) target.
    pub(super) fn find_dowhile_latch(&self, header: BlockId) -> Option<(BlockId, BlockId)> {
        for pred in self.cfg.predecessors(header) {
            // The latch is inside the loop (header dominates it).
            if !self.ssa.dominance.strictly_dominates(header, *pred) {
                continue;
            }
            if let Terminator::Branch {
                then_target,
                else_target,
            } = self.terminator(*pred)
            {
                // One target must be the back-edge to the header; the other is
                // the loop exit.
                if then_target == header {
                    return Some((*pred, else_target));
                }
                if else_target == header {
                    return Some((*pred, then_target));
                }
            }
        }
        None
    }

    pub(super) fn find_unconditional_latch(&self, header: BlockId) -> Option<BlockId> {
        self.cfg
            .predecessors(header)
            .iter()
            .copied()
            .find(|predecessor| {
                self.ssa.dominance.strictly_dominates(header, *predecessor)
                    && matches!(
                        self.terminator(*predecessor),
                        Terminator::Jump { target } | Terminator::Fallthrough { target }
                            if target == header
                    )
            })
    }

    /// Render a branch-headed natural loop as unconditional when both header
    /// successors stay inside it. Terminal return/throw edges may leave the
    /// loop without making the header comparison its loop condition.
    pub(super) fn try_emit_infinite_branch_loop(
        &self,
        out: &mut IrBlock,
        header: BlockId,
        then_target: BlockId,
        else_target: BlockId,
        visited: &mut HashSet<BlockId>,
    ) -> bool {
        if !self.loop_headers.contains(&header) {
            return false;
        }

        let members = self.natural_loop_blocks(header);
        let outside_targets: BTreeSet<_> = members
            .iter()
            .flat_map(|member| self.cfg.successors(*member).iter().copied())
            .filter(|successor| !members.contains(successor))
            .collect();
        if !members.contains(&then_target)
            || !members.contains(&else_target)
            || outside_targets.iter().any(|target| {
                !matches!(
                    self.terminator(*target),
                    Terminator::Return
                        | Terminator::Throw
                        | Terminator::Abort
                        | Terminator::NoReturnCall
                )
            })
        {
            return false;
        }

        let mut body = IrBlock::new();
        self.emit_body_except_condition(&mut body, header);

        if then_target == else_target {
            let shared = self.structure_edge_region(header, then_target, Some(header), visited);
            body.stmts.extend(shared.stmts);
        } else {
            let merge = self
                .closest_loop_merge(then_target, else_target, header, &members)
                .unwrap_or(header);
            let mut then_visited = visited.clone();
            let mut else_visited = visited.clone();
            let then_block =
                self.structure_edge_region(header, then_target, Some(merge), &mut then_visited);
            let else_block =
                self.structure_edge_region(header, else_target, Some(merge), &mut else_visited);
            visited.extend(then_visited);
            visited.extend(else_visited);
            let condition = self
                .comparison_condition_for_block(header)
                .unwrap_or_else(|| self.condition_for_block(header));

            if then_block.is_empty() {
                if !else_block.is_empty() {
                    body.push(Stmt::ControlFlow(Box::new(ControlFlow::if_else(
                        condition, then_block, else_block,
                    ))));
                }
            } else if else_block.is_empty() {
                body.push(Stmt::ControlFlow(Box::new(ControlFlow::if_then(
                    condition, then_block,
                ))));
            } else {
                body.push(Stmt::ControlFlow(Box::new(ControlFlow::if_else(
                    condition, then_block, else_block,
                ))));
            }

            if merge != header {
                let tail = self.structure_region(merge, Some(header), visited, true);
                body.stmts.extend(tail.stmts);
            }
        }

        out.push(Stmt::ControlFlow(Box::new(ControlFlow::while_loop(
            Expr::Literal(crate::decompiler::ir::Literal::Bool(true)),
            body,
        ))));
        true
    }
}
