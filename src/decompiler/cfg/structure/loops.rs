use std::collections::{BTreeSet, HashSet};

use crate::decompiler::cfg::ssa::ssa_var_name;
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

    /// Detect a loop header that branches back to itself: the loop body and
    /// the bottom test share a single block (`BB -> BB`). Returns the exit
    /// target of that block.
    ///
    /// A self-edge never satisfies `strictly_dominates(header, header)`, so
    /// the two-block latch search cannot see it; the shape needs its own
    /// recognition.
    pub(super) fn self_loop_exit(&self, header: BlockId) -> Option<BlockId> {
        let Terminator::Branch {
            then_target,
            else_target,
        } = self.terminator(header)
        else {
            return None;
        };
        match (then_target == header, else_target == header) {
            (true, false) => Some(else_target),
            (false, true) => Some(then_target),
            _ => None,
        }
    }

    /// Structure a single-block self-loop (`BB -> BB`) as a do-while.
    ///
    /// The header block holds both the loop body and the bottom test, so the
    /// generic `while` path would hoist the header's own statements out of
    /// the loop: the condition would keep its first value and the loop would
    /// spin forever without ever updating its state. Emitting the header's
    /// statements as the loop body keeps the update inside the loop, and the
    /// back-edge φ copy materialises the loop-carried value for the next
    /// iteration at the tail of the body.
    pub(super) fn structure_self_loop_dowhile(
        &self,
        out: &mut IrBlock,
        header: BlockId,
    ) -> Option<BlockId> {
        let exit = self.self_loop_exit(header)?;
        let backedge_copies = self.phi_lowering.edge_statements(header, header);
        let backedge_targets: BTreeSet<String> = backedge_copies
            .iter()
            .filter_map(|statement| match statement {
                Stmt::Assign { target, .. } => Some(target.clone()),
                _ => None,
            })
            .collect();

        // The bottom test is computed by the header's own statements, so it
        // has to run inside the body. Inlining the comparison into the
        // `while` clause is only safe while the back-edge copy cannot change
        // a value the test reads; otherwise the test keeps its own variable,
        // which is evaluated before the copy runs.
        let inlined = self.comparison_condition_for_block(header);
        let inline_is_safe = inlined
            .as_ref()
            .is_some_and(|condition| !expr_reads_any(condition, &backedge_targets));
        let (mut body, condition) = if inline_is_safe {
            let mut body = IrBlock::new();
            self.emit_body_except_condition(&mut body, header);
            (
                body,
                inlined.expect("inline_is_safe implies an inlined condition"),
            )
        } else {
            let mut body = IrBlock::new();
            self.emit_body(&mut body, header);
            let condition = self
                .condition_variable_for_block(header)
                .map(|variable| Expr::Variable(ssa_var_name(variable, self.source_names)))
                .unwrap_or_else(|| Expr::Variable(format!("cond_{}", header.0)));
            (body, condition)
        };

        // The back-edge φ copy runs at the tail of the body, right after the
        // update that produced its source value.
        body.stmts.extend(backedge_copies);

        out.push(Stmt::ControlFlow(Box::new(ControlFlow::do_while(
            body, condition,
        ))));
        out.stmts
            .extend(self.phi_lowering.edge_statements(header, exit));
        Some(exit)
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

/// Whether the expression reads any of the named variables.
fn expr_reads_any(expr: &Expr, names: &BTreeSet<String>) -> bool {
    match expr {
        Expr::Variable(name) => names.contains(name),
        Expr::Binary { left, right, .. } => {
            expr_reads_any(left, names) || expr_reads_any(right, names)
        }
        Expr::Unary { operand, .. }
        | Expr::Cast { expr: operand, .. }
        | Expr::Convert { value: operand, .. }
        | Expr::IsType { value: operand, .. } => expr_reads_any(operand, names),
        Expr::Call { args, .. } | Expr::Array(args) | Expr::Struct(args) => {
            args.iter().any(|argument| expr_reads_any(argument, names))
        }
        Expr::Index { base, index } => expr_reads_any(base, names) || expr_reads_any(index, names),
        Expr::Member { base, .. } => expr_reads_any(base, names),
        Expr::NewArray { length, .. } => expr_reads_any(length, names),
        Expr::Map(entries) => entries
            .iter()
            .any(|(key, value)| expr_reads_any(key, names) || expr_reads_any(value, names)),
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_reads_any(condition, names)
                || expr_reads_any(then_expr, names)
                || expr_reads_any(else_expr, names)
        }
        Expr::Unknown | Expr::Literal(_) | Expr::StackTemp(_) => false,
    }
}
