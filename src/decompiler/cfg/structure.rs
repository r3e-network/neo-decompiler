//! Structural control-flow recovery: CFG → typed `ir::Block`.
//!
//! This is the core of the Phase-4 IR spine. Instead of recovering `if`/`else`
//! structure by pattern-matching rendered text (the fragile legacy postprocess),
//! it walks the CFG directly and emits structured [`crate::decompiler::ir`]
//! nodes ([`ControlFlow::If`]), using the optimized SSA form for straight-line
//! bodies and branch conditions.
//!
//! Scope of this first slice: straight-line code, `if` / `if-else` (the most
//! common construct), and the terminating opcodes (`return` / `throw` /
//! `abort`). `while` / `for` / `switch` / `try` recovery and full parity with
//! the legacy emitter are follow-ups; unreachable shapes fall back to a block
//! list so output is always well-formed.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::decompiler::cfg::{BlockId, Cfg, Terminator};
use crate::decompiler::ir::{BinOp, Block as IrBlock, ControlFlow, Expr, Stmt};

use super::phi_lowering::PhiLowering;
use super::ssa::{
    ssa_expr_to_ir_with_source_names, ssa_var_name, SsaExpr, SsaForm, SsaStmt, SsaVariable, UseSite,
};

mod analysis;
mod cleanup;
mod graph;
mod regions;
mod switches;
mod try_regions;

#[cfg(test)]
mod tests;

use analysis::{
    collect_leave_targets, collect_structural_uses, compute_loop_headers, compute_postdominators,
    find_irreducible_region, resolve_leave_target_cfg,
};
use cleanup::simplify_unreachable_control;

/// Structure a whole [`SsaForm`] into a single [`IrBlock`] starting from its
/// entry block.
#[must_use]
pub fn structure(ssa: &SsaForm) -> IrBlock {
    structure_with_source_names(ssa, &BTreeMap::new())
}

/// Structure an SSA form while applying source-level display names at the
/// lowering boundary. SSA analysis keeps its stable internal slot identities.
pub(crate) fn structure_with_source_names(
    ssa: &SsaForm,
    source_names: &BTreeMap<String, String>,
) -> IrBlock {
    let loop_headers = compute_loop_headers(&ssa.cfg, &ssa.dominance);
    let postdominators = compute_postdominators(&ssa.cfg);
    let structural_uses = collect_structural_uses(ssa);
    let leave_targets = collect_leave_targets(&ssa.cfg);
    let phi_lowering = PhiLowering::new(ssa, source_names);
    let ctx = StructCtx {
        cfg: &ssa.cfg,
        ssa,
        source_names,
        loop_headers,
        postdominators,
        structural_uses,
        leave_targets,
        phi_lowering,
    };
    let entry = ssa.cfg.blocks().next().map(|b| b.id);
    let mut visited = HashSet::new();
    match entry {
        Some(e) => {
            let mut out = IrBlock::with_stmts(ctx.phi_lowering.entry_statements(e));
            if let Some(region) = find_irreducible_region(&ssa.cfg) {
                out.stmts
                    .extend(ctx.structure_irreducible(e, &region).stmts);
            } else {
                out.stmts
                    .extend(ctx.structure_region(e, None, &mut visited, true).stmts);
            }
            simplify_unreachable_control(&mut out);
            out
        }
        None => IrBlock::new(),
    }
}

pub(super) struct StructCtx<'a> {
    cfg: &'a Cfg,
    ssa: &'a SsaForm,
    source_names: &'a BTreeMap<String, String>,
    loop_headers: HashSet<BlockId>,
    postdominators: BTreeMap<BlockId, BTreeSet<BlockId>>,
    structural_uses: BTreeSet<SsaVariable>,
    leave_targets: BTreeSet<BlockId>,
    phi_lowering: PhiLowering,
}

impl<'a> StructCtx<'a> {
    fn emit_body(&self, out: &mut IrBlock, bid: BlockId) {
        if let Some(block) = self.ssa.block(bid) {
            let mut index = 0;
            while index < block.stmts.len() {
                let stmt = &block.stmts[index];
                if let SsaStmt::Assign {
                    target,
                    value: call @ SsaExpr::Call { .. },
                } = stmt
                {
                    let next_returns_target = matches!(
                        block.stmts.get(index + 1),
                        Some(SsaStmt::Return(Some(SsaExpr::Variable(returned))))
                            if returned == target
                    );
                    let return_site = UseSite::new(bid, index + 1);
                    let has_only_adjacent_return_use = self
                        .ssa
                        .uses_of(target)
                        .is_some_and(|sites| sites.len() == 1 && sites.contains(&return_site));
                    if target.base == "t" && next_returns_target && has_only_adjacent_return_use {
                        out.push(Stmt::Return(Some(ssa_expr_to_ir_with_source_names(
                            call,
                            self.source_names,
                        ))));
                        index += 2;
                        continue;
                    }
                }

                self.emit_ssa_stmt(out, stmt);
                index += 1;
            }
        }

        if !matches!(self.terminator(bid), Terminator::TryEntry { .. }) {
            if let [successor] = self.cfg.successors(bid) {
                out.stmts
                    .extend(self.phi_lowering.edge_statements(bid, *successor));
            }
        }
    }

    fn emit_ssa_stmt(&self, out: &mut IrBlock, stmt: &SsaStmt) {
        match stmt {
            SsaStmt::Assign {
                target,
                value: call @ SsaExpr::Call { .. },
            } if target.base == "t"
                && self.ssa.uses_of(target).is_none_or(BTreeSet::is_empty)
                && !self.structural_uses.contains(target) =>
            {
                out.push(Stmt::ExprStmt(ssa_expr_to_ir_with_source_names(
                    call,
                    self.source_names,
                )));
            }
            SsaStmt::Assign { target, value } => out.push(Stmt::Assign {
                target: ssa_var_name(target, self.source_names),
                value: ssa_expr_to_ir_with_source_names(value, self.source_names),
            }),
            SsaStmt::Expr(value) => out.push(Stmt::ExprStmt(ssa_expr_to_ir_with_source_names(
                value,
                self.source_names,
            ))),
            SsaStmt::Return(value) => {
                out.push(Stmt::Return(value.as_ref().map(|value| {
                    ssa_expr_to_ir_with_source_names(value, self.source_names)
                })));
            }
            SsaStmt::Throw(value) => {
                out.push(Stmt::Throw(value.as_ref().map(|value| {
                    ssa_expr_to_ir_with_source_names(value, self.source_names)
                })));
            }
            SsaStmt::Abort(message) => {
                out.push(Stmt::Abort(message.as_ref().map(|message| {
                    ssa_expr_to_ir_with_source_names(message, self.source_names)
                })));
            }
            SsaStmt::Assert { condition, message } => out.push(Stmt::Assert {
                condition: ssa_expr_to_ir_with_source_names(condition, self.source_names),
                message: message
                    .as_ref()
                    .map(|message| ssa_expr_to_ir_with_source_names(message, self.source_names)),
            }),
            SsaStmt::Other(stmt) => out.push(stmt.clone()),
            SsaStmt::Phi(_) => {}
        }
    }

    fn block_has_explicit_return(&self, bid: BlockId) -> bool {
        self.ssa.block(bid).is_some_and(|block| {
            block
                .stmts
                .iter()
                .any(|stmt| matches!(stmt, SsaStmt::Return(_)))
        })
    }

    fn block_has_explicit_failure(&self, bid: BlockId) -> bool {
        self.ssa.block(bid).is_some_and(|block| {
            block
                .stmts
                .iter()
                .any(|stmt| matches!(stmt, SsaStmt::Throw(_) | SsaStmt::Abort(_)))
        })
    }

    /// Emit a branch block while suppressing only the assignment that defines
    /// its terminator condition. Side effects may legally follow that definition
    /// while the condition remains deeper on the VM stack.
    fn emit_body_except_condition(&self, out: &mut IrBlock, bid: BlockId) {
        let Some(block) = self.ssa.block(bid) else {
            return;
        };
        let condition = self
            .condition_variable_for_block(bid)
            .filter(|condition| self.can_inline_condition(bid, condition));
        for stmt in &block.stmts {
            if matches!(stmt, SsaStmt::Assign { target, .. } if Some(target) == condition) {
                continue;
            }
            self.emit_ssa_stmt(out, stmt);
        }
    }

    /// Recover an `if` / `if-else` from a `Branch` terminator: find the merge
    /// (closest common post-dominator by reachability intersection + predecessor
    /// count), structure each side up to it, and continue at the merge.
    fn structure_edge_region(
        &self,
        from: BlockId,
        entry: BlockId,
        boundary: Option<BlockId>,
        visited: &mut HashSet<BlockId>,
    ) -> IrBlock {
        let mut out = IrBlock::with_stmts(self.phi_lowering.edge_statements(from, entry));
        if let Some(transfer) = self.loop_transfer(from, entry) {
            out.stmts.push(transfer);
            return out;
        }
        if Some(entry) == boundary {
            return out;
        }
        out.stmts
            .extend(self.structure_region(entry, boundary, visited, true).stmts);
        out
    }

    fn loop_follow(&self, header: BlockId) -> Option<BlockId> {
        let Terminator::Branch {
            then_target,
            else_target,
        } = self.terminator(header)
        else {
            return None;
        };
        if self.loop_headers.contains(&header) && self.reachable(then_target).contains(&header) {
            Some(else_target)
        } else if self.loop_headers.contains(&header)
            && self.reachable(else_target).contains(&header)
        {
            Some(then_target)
        } else {
            None
        }
    }

    fn orient_branch_loop(
        &self,
        header: BlockId,
        then_target: BlockId,
        else_target: BlockId,
        condition: Expr,
    ) -> (BlockId, BlockId, Expr) {
        if self.loop_follow(header) == Some(then_target) {
            (
                else_target,
                then_target,
                Expr::unary(crate::decompiler::ir::UnaryOp::LogicalNot, condition),
            )
        } else {
            (then_target, else_target, condition)
        }
    }

    fn loop_transfer(&self, from: BlockId, target: BlockId) -> Option<Stmt> {
        let mut headers: Vec<_> = self
            .loop_headers
            .iter()
            .copied()
            .filter(|header| {
                *header == from || self.ssa.dominance.strictly_dominates(*header, from)
            })
            .collect();
        headers.sort_by_key(|header| std::cmp::Reverse(header.0));
        for header in headers {
            if !self.natural_loop_blocks(header).contains(&from) {
                continue;
            }
            if target == header
                && matches!(self.terminator(header), Terminator::Branch { .. })
                && self.reachable(from).contains(&header)
            {
                return Some(Stmt::Continue);
            }
            if from != header && self.loop_follow(header) == Some(target) {
                return Some(Stmt::Break);
            }
        }
        None
    }

    fn leave_transfer(&self, from: BlockId, continuation: BlockId) -> Stmt {
        let target = resolve_leave_target_cfg(self.cfg, continuation);
        if let Some(loop_transfer) = self.loop_transfer(from, target) {
            return loop_transfer;
        }
        if let Some(return_stmt) = self.return_through_finally(from, target) {
            return return_stmt;
        }
        Stmt::Goto(crate::decompiler::ir::BlockLabel(target.0))
    }

    fn return_through_finally(&self, from: BlockId, target: BlockId) -> Option<Stmt> {
        if !matches!(self.terminator(target), Terminator::Return) {
            return None;
        }
        let target_block = self.ssa.block(target)?;
        let [SsaStmt::Return(returned)] = target_block.stmts.as_slice() else {
            return None;
        };
        let returned = returned.as_ref();
        let Some(returned) = returned else {
            return Some(Stmt::Return(None));
        };
        if let Terminator::EndTryFinally { finally_target, .. } = self.terminator(from) {
            let stack_phi_base = format!("p{}", finally_target.0);
            let operand = self
                .ssa
                .block(finally_target)?
                .phi_nodes
                .iter()
                .filter(|phi| phi.target.base == stack_phi_base)
                .max_by_key(|phi| phi.target.version)
                .and_then(|phi| phi.operands.get(&from));
            if let Some(operand) = operand.filter(|operand| operand.base != "?") {
                return Some(Stmt::Return(Some(ssa_expr_to_ir_with_source_names(
                    &SsaExpr::var(operand.clone()),
                    self.source_names,
                ))));
            }
        }
        match returned {
            SsaExpr::Literal(_) => Some(Stmt::Return(Some(ssa_expr_to_ir_with_source_names(
                returned,
                self.source_names,
            )))),
            SsaExpr::Variable(variable)
                if variable.base != "?"
                    && self
                        .ssa
                        .definitions
                        .get(variable)
                        .is_some_and(|definition| {
                            *definition == from
                                || self.ssa.dominance.strictly_dominates(*definition, from)
                        }) =>
            {
                Some(Stmt::Return(Some(ssa_expr_to_ir_with_source_names(
                    &SsaExpr::var(variable.clone()),
                    self.source_names,
                ))))
            }
            _ => None,
        }
    }

    fn handle_branch(
        &self,
        out: &mut IrBlock,
        bid: BlockId,
        then_target: BlockId,
        else_target: BlockId,
        outer_boundary: Option<BlockId>,
        visited: &mut HashSet<BlockId>,
    ) -> Option<BlockId> {
        // Inline the comparison driving the branch when available (e.g.
        // `(loc0 < 3)` instead of the bare reaching-definition variable).
        let cond = self
            .comparison_condition_for_block(bid)
            .unwrap_or_else(|| self.condition_for_block(bid));

        if then_target == else_target {
            // Degenerate branch (condition has no effect): drop it and continue.
            out.stmts
                .extend(self.phi_lowering.edge_statements(bid, then_target));
            return Some(then_target);
        }

        // An equality cascade `if (x == c0) {...} else if (x == c1) {...} else {...}`
        // (same scrutinee x) is rendered as a switch. Falls back to if/else when
        // fewer than two cases match or the scrutinee changes.
        if let Some(switch) = self.try_switch(bid, then_target, else_target, visited) {
            out.push(Stmt::ControlFlow(Box::new(ControlFlow::Switch {
                expr: switch.scrutinee,
                cases: switch.cases,
                default: switch.default,
            })));
            return match switch.merge {
                Some(m) if Some(m) != outer_boundary => Some(m),
                _ => None,
            };
        }

        let merge = self.find_merge(then_target, else_target);

        // The then/else sub-regions must stop at the merge so neither side
        // duplicates the post-merge code.
        let mut then_visited = visited.clone();
        let mut else_visited = visited.clone();
        let then_block = self.structure_edge_region(bid, then_target, merge, &mut then_visited);
        let else_block = self.structure_edge_region(bid, else_target, merge, &mut else_visited);
        visited.extend(then_visited);
        visited.extend(else_visited);

        let cf = if else_block.is_empty() && then_block.is_empty() {
            // Both sides empty: keep the condition as a bare statement.
            out.push(Stmt::ExprStmt(cond));
            return merge.or(Some(else_target));
        } else if else_block.is_empty() {
            ControlFlow::if_then(cond, then_block)
        } else {
            ControlFlow::if_else(cond, then_block, else_block)
        };
        out.push(Stmt::ControlFlow(Box::new(cf)));

        // Continue at the merge (still respecting the outer boundary).
        match merge {
            Some(m) if Some(m) != outer_boundary => Some(m),
            _ => None,
        }
    }

    fn handle_branch_in_set(
        &self,
        out: &mut IrBlock,
        bid: BlockId,
        then_target: BlockId,
        else_target: BlockId,
        stop: &HashSet<BlockId>,
        visited: &mut HashSet<BlockId>,
    ) -> Option<BlockId> {
        let cond = self
            .comparison_condition_for_block(bid)
            .unwrap_or_else(|| self.condition_for_block(bid));
        if then_target == else_target {
            out.stmts
                .extend(self.phi_lowering.edge_statements(bid, then_target));
            return (!stop.contains(&then_target)).then_some(then_target);
        }

        let then_distances = self.shortest_distances(then_target);
        let else_distances = self.shortest_distances(else_target);
        let merge = self.find_merge(then_target, else_target);
        let merge_score = merge.and_then(|block| {
            Some((
                *then_distances.get(&block)?.max(else_distances.get(&block)?),
                block,
            ))
        });
        let stop_score = stop
            .iter()
            .filter_map(|block| {
                Some((
                    *then_distances.get(block)?.max(else_distances.get(block)?),
                    *block,
                ))
            })
            .min();
        let merge = match (merge_score, stop_score) {
            (Some((merge_distance, merge)), Some((stop_distance, _)))
                if merge_distance < stop_distance =>
            {
                Some(merge)
            }
            (Some((_, merge)), None) => Some(merge),
            _ => None,
        };

        let mut arm_stop = stop.clone();
        if let Some(merge) = merge {
            arm_stop.insert(merge);
        }
        let mut then_visited = visited.clone();
        let mut else_visited = visited.clone();
        let then_block =
            self.structure_set_edge_region(bid, then_target, &arm_stop, &mut then_visited);
        let else_block =
            self.structure_set_edge_region(bid, else_target, &arm_stop, &mut else_visited);
        visited.extend(then_visited);
        visited.extend(else_visited);
        let control = if else_block.is_empty() && then_block.is_empty() {
            out.push(Stmt::ExprStmt(cond));
            return merge;
        } else if else_block.is_empty() {
            ControlFlow::if_then(cond, then_block)
        } else {
            ControlFlow::if_else(cond, then_block, else_block)
        };
        out.push(Stmt::ControlFlow(Box::new(control)));
        merge
    }

    /// If the last def in `bid` is a comparison expression (Binary with a
    /// relational/equality op), return its lowered IR expression so the caller can
    /// inline it into the `if (…)` head. Otherwise return `None` — the caller then
    /// falls back to the bare reaching-definition variable via
    /// [`StructCtx::condition_for_block`].
    fn comparison_condition_for_block(&self, bid: BlockId) -> Option<Expr> {
        let value = self.condition_expression(bid)?;
        let SsaExpr::Binary { op, .. } = value else {
            return None;
        };
        let is_comparison = matches!(
            op,
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
        );
        is_comparison.then(|| ssa_expr_to_ir_with_source_names(&value, self.source_names))
    }

    /// Lower the exact value consumed by the branch terminator. Builder-produced
    /// SSA records this as a synthetic use; hand-built test forms fall back to
    /// the final assignment for compatibility.
    fn condition_for_block(&self, bid: BlockId) -> Expr {
        self.condition_expression(bid).map_or_else(
            || Expr::Variable(format!("cond_{}", bid.0)),
            |condition| ssa_expr_to_ir_with_source_names(&condition, self.source_names),
        )
    }

    fn condition_variable_for_block(&self, bid: BlockId) -> Option<&SsaVariable> {
        let terminator_site = UseSite::terminator(bid);
        self.ssa
            .uses
            .iter()
            .find_map(|(variable, sites)| sites.contains(&terminator_site).then_some(variable))
            .or_else(|| {
                self.ssa.block(bid)?.stmts.iter().rev().find_map(|stmt| {
                    if let SsaStmt::Assign { target, .. } = stmt {
                        Some(target)
                    } else {
                        None
                    }
                })
            })
    }

    /// The value expression consumed by `bid`'s branch terminator.
    fn condition_expression(&self, bid: BlockId) -> Option<SsaExpr> {
        let condition = self.condition_variable_for_block(bid)?;
        if !self.can_inline_condition(bid, condition) {
            return Some(SsaExpr::var(condition.clone()));
        }
        let block = self.ssa.block(bid)?;
        for stmt in &block.stmts {
            if let SsaStmt::Assign { target, value } = stmt {
                if target == condition {
                    return Some(value.clone());
                }
            }
        }
        Some(SsaExpr::var(condition.clone()))
    }

    fn can_inline_condition(&self, bid: BlockId, condition: &SsaVariable) -> bool {
        let terminator_site = UseSite::terminator(bid);
        self.ssa.uses_of(condition).is_none_or(|sites| {
            sites.is_empty() || (sites.len() == 1 && sites.contains(&terminator_site))
        })
    }

    /// Emit a `while` loop for a loop-header branch. (A `for`-form promotion is
    /// intentionally not attempted: SSA versions differ across the init /
    /// condition / update, so a clean `for(i=0; i<n; i++)` would require
    /// de-versioning the loop variable — a cosmetic refinement that isn't worth
    /// the complexity, since the `while` form is semantically exact.)
    fn build_loop(&self, out: &mut IrBlock, bid: BlockId, cond: Expr, body: &mut IrBlock) {
        // Backedge phi copies are already at the body tail. Replay the header
        // after them so its effects run before the next condition check.
        self.emit_body_except_condition(body, bid);
        out.push(Stmt::ControlFlow(Box::new(ControlFlow::while_loop(
            cond,
            std::mem::take(body),
        ))));
    }

    /// Structure a region that halts at any block in `stop` (a set boundary,
    /// used for the mutually-exclusive try/catch/finally sub-regions).
    fn structure_set_edge_region(
        &self,
        from: BlockId,
        entry: BlockId,
        stop: &HashSet<BlockId>,
        visited: &mut HashSet<BlockId>,
    ) -> IrBlock {
        let mut out = IrBlock::with_stmts(self.phi_lowering.edge_statements(from, entry));
        out.stmts
            .extend(self.structure_set(entry, stop, visited).stmts);
        out
    }

    fn structure_set(
        &self,
        entry: BlockId,
        stop: &HashSet<BlockId>,
        visited: &mut HashSet<BlockId>,
    ) -> IrBlock {
        let mut out = IrBlock::new();
        let mut cur = Some(entry);
        while let Some(bid) = cur {
            if stop.contains(&bid) {
                break;
            }
            if !visited.insert(bid) {
                break;
            }
            if self.leave_targets.contains(&bid) {
                out.push(Stmt::Label(crate::decompiler::ir::BlockLabel(bid.0)));
            }
            if let Terminator::Branch {
                then_target,
                else_target,
            } = self.terminator(bid)
            {
                if self.try_emit_infinite_branch_loop(
                    &mut out,
                    bid,
                    then_target,
                    else_target,
                    visited,
                ) {
                    cur = None;
                    continue;
                }
            }
            if matches!(self.terminator(bid), Terminator::Branch { .. }) {
                self.emit_body_except_condition(&mut out, bid);
            } else {
                self.emit_body(&mut out, bid);
            }
            cur = match self.terminator(bid) {
                Terminator::Return
                | Terminator::Throw
                | Terminator::Abort
                | Terminator::NoReturnCall
                | Terminator::Unknown
                | Terminator::EndFinally { .. } => None,
                Terminator::EndTry {
                    continuation,
                    nonlocal,
                } => {
                    if nonlocal {
                        out.push(self.leave_transfer(bid, continuation));
                    }
                    None
                }
                Terminator::EndTryFinally {
                    continuation,
                    nonlocal,
                    ..
                } => {
                    if nonlocal {
                        out.push(self.leave_transfer(bid, continuation));
                    }
                    None
                }
                Terminator::TryEntry {
                    body_target,
                    catch_target,
                    finally_target,
                } => {
                    let has_nonlocal_leave =
                        self.try_has_nonlocal_leave(bid, body_target, catch_target);
                    let continuation = self.handle_try(
                        &mut out,
                        bid,
                        body_target,
                        catch_target,
                        finally_target,
                        visited,
                    );
                    if has_nonlocal_leave {
                        None
                    } else {
                        continuation
                    }
                }
                Terminator::Fallthrough { target } | Terminator::Jump { target } => Some(target),
                Terminator::Branch {
                    then_target,
                    else_target,
                } => {
                    if self.loop_headers.contains(&bid) {
                        let cond = self
                            .comparison_condition_for_block(bid)
                            .unwrap_or_else(|| self.condition_for_block(bid));
                        let (body_target, follow_target, cond) =
                            self.orient_branch_loop(bid, then_target, else_target, cond);
                        let mut body = self.structure_edge_region(
                            bid,
                            body_target,
                            Some(follow_target),
                            visited,
                        );
                        self.build_loop(&mut out, bid, cond, &mut body);
                        out.stmts
                            .extend(self.phi_lowering.edge_statements(bid, follow_target));
                        Some(follow_target)
                    } else {
                        self.handle_branch_in_set(
                            &mut out,
                            bid,
                            then_target,
                            else_target,
                            stop,
                            visited,
                        )
                    }
                }
            };
            // Honour the stop set between steps.
            if matches!(cur, Some(c) if stop.contains(&c)) {
                break;
            }
        }
        out
    }

    /// For a do-while loop header `header`, find its latch: the back-edge
    /// predecessor that is a `Branch` re-entering `header`. Returns `(latch,
    /// exit)` where `exit` is the latch's other (non-back-edge) target.
    fn find_dowhile_latch(&self, header: BlockId) -> Option<(BlockId, BlockId)> {
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

    fn find_unconditional_latch(&self, header: BlockId) -> Option<BlockId> {
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
    fn try_emit_infinite_branch_loop(
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
