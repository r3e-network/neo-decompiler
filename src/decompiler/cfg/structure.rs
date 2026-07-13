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

use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};

use crate::decompiler::cfg::{BasicBlock, BlockId, Cfg, EdgeKind, Terminator};
use crate::decompiler::ir::{BinOp, Block as IrBlock, ControlFlow, Expr, Stmt};

use super::phi_lowering::PhiLowering;
use super::ssa::{
    ssa_expr_to_ir_with_source_names, ssa_var_name, DominanceInfo, SsaExpr, SsaForm, SsaStmt,
    SsaVariable, UseSite,
};

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
    let structural_uses = collect_structural_uses(ssa);
    let phi_lowering = PhiLowering::new(ssa, source_names);
    let ctx = StructCtx {
        cfg: &ssa.cfg,
        ssa,
        source_names,
        loop_headers,
        structural_uses,
        phi_lowering,
    };
    let entry = ssa.cfg.blocks().next().map(|b| b.id);
    let mut visited = HashSet::new();
    match entry {
        Some(e) => {
            let mut out = IrBlock::with_stmts(ctx.phi_lowering.entry_statements(e));
            out.stmts
                .extend(ctx.structure_region(e, None, &mut visited, true).stmts);
            out
        }
        None => IrBlock::new(),
    }
}

/// Identify loop-header blocks: a block `H` is a loop header if some predecessor
/// `P` is dominated by `H` (edge `P → H` is a back-edge). Uses the SSA form's
/// precomputed dominance information.
fn compute_loop_headers(cfg: &Cfg, dominance: &DominanceInfo) -> HashSet<BlockId> {
    let mut headers = HashSet::new();
    for block in cfg.blocks() {
        for pred in cfg.predecessors(block.id) {
            if *pred == block.id || dominance.strictly_dominates(block.id, *pred) {
                headers.insert(block.id);
                break;
            }
        }
    }
    headers
}

fn collect_structural_uses(ssa: &SsaForm) -> BTreeSet<SsaVariable> {
    let mut uses = BTreeSet::new();
    for block in ssa.blocks.values() {
        for phi in &block.phi_nodes {
            uses.extend(phi.operands.values().cloned());
        }
        for stmt in &block.stmts {
            match stmt {
                SsaStmt::Assign { value, .. }
                | SsaStmt::Expr(value)
                | SsaStmt::Return(Some(value)) => collect_expr_uses(value, &mut uses),
                SsaStmt::Return(None) | SsaStmt::Phi(_) | SsaStmt::Other(_) => {}
            }
        }
    }
    uses
}

fn collect_expr_uses(expr: &SsaExpr, uses: &mut BTreeSet<SsaVariable>) {
    match expr {
        SsaExpr::Variable(variable) => {
            uses.insert(variable.clone());
        }
        SsaExpr::Binary { left, right, .. } => {
            collect_expr_uses(left, uses);
            collect_expr_uses(right, uses);
        }
        SsaExpr::Unary { operand, .. } => collect_expr_uses(operand, uses),
        SsaExpr::Call { args, .. } => {
            for argument in args {
                collect_expr_uses(argument, uses);
            }
        }
        SsaExpr::Index { base, index } => {
            collect_expr_uses(base, uses);
            collect_expr_uses(index, uses);
        }
        SsaExpr::Member { base, .. } => collect_expr_uses(base, uses),
        SsaExpr::Cast { expr, .. } => collect_expr_uses(expr, uses),
        SsaExpr::Array(elements) => {
            for element in elements {
                collect_expr_uses(element, uses);
            }
        }
        SsaExpr::Map(pairs) => {
            for (key, value) in pairs {
                collect_expr_uses(key, uses);
                collect_expr_uses(value, uses);
            }
        }
        SsaExpr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_expr_uses(condition, uses);
            collect_expr_uses(then_expr, uses);
            collect_expr_uses(else_expr, uses);
        }
        SsaExpr::Literal(_) => {}
    }
}

struct StructCtx<'a> {
    cfg: &'a Cfg,
    ssa: &'a SsaForm,
    source_names: &'a BTreeMap<String, String>,
    loop_headers: HashSet<BlockId>,
    structural_uses: BTreeSet<SsaVariable>,
    phi_lowering: PhiLowering,
}

impl<'a> StructCtx<'a> {
    /// Structure the region reachable from `entry`, stopping without traversing
    /// into `boundary` (used so an `if`'s then/else sub-regions halt at the
    /// merge block, which the outer loop then emits in sequence).
    fn structure_region(
        &self,
        entry: BlockId,
        boundary: Option<BlockId>,
        visited: &mut HashSet<BlockId>,
        detect_dowhile: bool,
    ) -> IrBlock {
        let mut out = IrBlock::new();
        let mut cur = Some(entry);

        while let Some(bid) = cur {
            if Some(bid) == boundary {
                break;
            }

            // do-while: a loop header whose own terminator is NOT a Branch (the
            // test sits at the bottom, on the latch). Detect it at the header so
            // the whole body is collected before the test, then resume at the
            // latch's exit. `detect_dowhile` is cleared for the body sub-walk so
            // the header isn't re-detected on re-entry.
            if detect_dowhile
                && self.loop_headers.contains(&bid)
                && !matches!(self.terminator(bid), Terminator::Branch { .. })
            {
                if let Some((latch, exit)) = self.find_dowhile_latch(bid) {
                    let mut body = self.structure_region(bid, Some(latch), visited, false);
                    let backedge_copies = self.phi_lowering.edge_statements(latch, bid);
                    if !backedge_copies.is_empty() {
                        let first_iteration = self
                            .phi_lowering
                            .fresh_name(&format!("do_while_first_{}", bid.index()));
                        out.push(Stmt::assign(
                            first_iteration.clone(),
                            Expr::Literal(crate::decompiler::ir::Literal::Bool(true)),
                        ));
                        body.stmts.splice(
                            0..0,
                            [
                                Stmt::ControlFlow(Box::new(ControlFlow::if_then(
                                    Expr::unary(
                                        crate::decompiler::ir::UnaryOp::LogicalNot,
                                        Expr::var(first_iteration.clone()),
                                    ),
                                    IrBlock::with_stmts(backedge_copies),
                                ))),
                                Stmt::assign(
                                    first_iteration,
                                    Expr::Literal(crate::decompiler::ir::Literal::Bool(false)),
                                ),
                            ],
                        );
                    }
                    let cond = self.condition_for_block(latch);
                    out.push(Stmt::ControlFlow(Box::new(ControlFlow::do_while(
                        body, cond,
                    ))));
                    out.stmts
                        .extend(self.phi_lowering.edge_statements(latch, exit));
                    // The latch's test is consumed by the do-while; mark it
                    // visited so the outer walk does not re-emit it, and resume
                    // at its exit.
                    visited.insert(latch);
                    cur = Some(exit);
                    continue;
                }
            }

            if !visited.insert(bid) {
                break;
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

            // For a Branch, the last def IS the branch condition and is
            // consumed (inlined) into the `if (…)` head by handle_branch /
            // the loop-header path — emit all-but-last to avoid duplicating it.
            // For other terminators (Return / Jump / Fallthrough / TryEntry /
            // EndTry / Unknown) the last def is regular body code and is
            // emitted in full.
            if matches!(self.terminator(bid), Terminator::Branch { .. }) {
                self.emit_body_except_condition(&mut out, bid);
            } else {
                self.emit_body(&mut out, bid);
            }

            cur = match self.terminator(bid) {
                Terminator::Return => {
                    if !self.block_has_explicit_return(bid) {
                        out.push(Stmt::Comment(format!("return at {:?}", bid)));
                    }
                    None
                }
                Terminator::Throw | Terminator::Abort | Terminator::Unknown => {
                    out.push(Stmt::Comment(format!("return/throw/abort at {:?}", bid)));
                    None
                }
                Terminator::Fallthrough { target } | Terminator::Jump { target } => Some(target),
                Terminator::Branch {
                    then_target,
                    else_target,
                } => {
                    // A loop header branch (has a back-edge predecessor) is a
                    // `while`: the condition is the branch's last def, the body
                    // is the then-target, and control resumes at the else-target
                    // (the loop exit). The body's back-edge naturally stops at
                    // the header (already visited).
                    if self.loop_headers.contains(&bid) {
                        let cond = self
                            .comparison_condition_for_block(bid)
                            .unwrap_or_else(|| self.condition_for_block(bid));
                        let mut body =
                            self.structure_edge_region(bid, then_target, Some(bid), visited);
                        self.build_loop(&mut out, bid, cond, &mut body);
                        out.stmts
                            .extend(self.phi_lowering.edge_statements(bid, else_target));
                        Some(else_target)
                    } else {
                        self.handle_branch(
                            &mut out,
                            bid,
                            then_target,
                            else_target,
                            boundary,
                            visited,
                        )
                    }
                }
                Terminator::TryEntry {
                    body_target,
                    catch_target,
                    finally_target,
                } => self.handle_try(
                    &mut out,
                    bid,
                    body_target,
                    catch_target,
                    finally_target,
                    visited,
                ),
                Terminator::EndTry { .. } => {
                    // An EndTry is the join of a try construct; it is consumed
                    // by handle_try's continuation logic. Stop if reached bare.
                    None
                }
            };
        }

        out
    }

    /// Emit a block's straight-line SSA assignments as IR statements.
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

    /// Emit a branch block while suppressing only the assignment that defines
    /// its terminator condition. Side effects may legally follow that definition
    /// while the condition remains deeper on the VM stack.
    fn emit_body_except_condition(&self, out: &mut IrBlock, bid: BlockId) {
        let Some(block) = self.ssa.block(bid) else {
            return;
        };
        let condition = self.condition_variable_for_block(bid);
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
        out.stmts
            .extend(self.structure_region(entry, boundary, visited, true).stmts);
        out
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
        let then_block = self.structure_edge_region(bid, then_target, merge, visited);
        let else_block = self.structure_edge_region(bid, else_target, merge, visited);

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

    /// Emit a `while` loop for a loop-header branch. (A `for`-form promotion is
    /// intentionally not attempted: SSA versions differ across the init /
    /// condition / update, so a clean `for(i=0; i<n; i++)` would require
    /// de-versioning the loop variable — a cosmetic refinement that isn't worth
    /// the complexity, since the `while` form is semantically exact.)
    fn build_loop(&self, out: &mut IrBlock, _bid: BlockId, cond: Expr, body: &mut IrBlock) {
        out.push(Stmt::ControlFlow(Box::new(ControlFlow::while_loop(
            cond,
            std::mem::take(body),
        ))));
    }

    /// Try to recognise a `switch` equality-cascade starting at `bid`.
    ///
    /// A Neo C# `switch` lowers to a cascade of `if (scrut == const)` branches
    /// sharing one scrutinee, terminated by a default tail. Each case body is
    /// the branch's then-target; the else-chain either continues the cascade or
    /// reaches the default. Requires at least two equality cases on the same
    /// scrutinee; otherwise returns `None` (caller falls back to if/else).
    fn try_switch(
        &self,
        bid: BlockId,
        then_target: BlockId,
        else_target: BlockId,
        visited: &mut HashSet<BlockId>,
    ) -> Option<SwitchResult> {
        // The first comparison must be `scrut == const` (or `const == scrut`).
        let first = self.condition_expression(bid)?;
        let (scrut_base, first_val) = extract_eq_cond(&first)?;

        // Collect the cascade along the else-chain.
        let mut cases: Vec<(Expr, BlockId, BlockId)> = vec![(
            ssa_expr_to_ir_with_source_names(&first_val, self.source_names),
            bid,
            then_target,
        )];
        let mut cur = else_target;
        let mut default_from = bid;
        let default_entry;
        loop {
            // Stop if `cur` has multiple predecessors (a join / merge): the
            // cascade has reconverged, so what follows is shared code, not a
            // case comparison.
            if self.cfg.predecessors(cur).len() >= 2 {
                default_entry = cur;
                break;
            }
            match self.terminator(cur) {
                Terminator::Branch {
                    then_target,
                    else_target,
                } => {
                    let cond = self.condition_expression(cur);
                    match cond.and_then(|c| extract_eq_cond(&c)) {
                        Some((base, val)) if base == scrut_base => {
                            cases.push((
                                ssa_expr_to_ir_with_source_names(&val, self.source_names),
                                cur,
                                then_target,
                            ));
                            default_from = cur;
                            cur = else_target;
                        }
                        _ => {
                            default_entry = cur;
                            break;
                        }
                    }
                }
                _ => {
                    default_entry = cur;
                    break;
                }
            }
        }

        // Need at least two real equality cases to be worth a switch.
        if cases.len() < 2 {
            return None;
        }

        // The merge: where the case bodies and default reconverge. Use the join
        // of the first case body and the default tail.
        let merge = self.find_merge(cases[0].2, default_entry);

        let mut case_blocks: Vec<(Expr, IrBlock)> = Vec::with_capacity(cases.len());
        for (val, comparison_block, body_entry) in &cases {
            let body = self.structure_edge_region(*comparison_block, *body_entry, merge, visited);
            case_blocks.push((val.clone(), body));
        }
        let default_body = self.structure_edge_region(default_from, default_entry, merge, visited);
        let default = if default_body.is_empty() {
            None
        } else {
            Some(default_body)
        };

        Some(SwitchResult {
            scrutinee: Expr::Variable(scrut_base),
            cases: case_blocks,
            default,
            merge,
        })
    }

    /// Find the merge of two branch arms: the closest block reachable from both
    /// that has ≥ 2 predecessors (i.e. a real join). Returns `None` when the
    /// arms do not reconverge within the CFG.
    fn find_merge(&self, then_target: BlockId, else_target: BlockId) -> Option<BlockId> {
        let from_then = self.reachable(then_target);
        let from_else = self.reachable(else_target);
        let common: BTreeSet<BlockId> = from_then.intersection(&from_else).copied().collect();
        // The merge is the common block with ≥2 predecessors; prefer the lowest
        // id (closest to the branch in well-structured code).
        common
            .into_iter()
            .find(|b| self.cfg.predecessors(*b).len() >= 2)
    }

    /// All blocks reachable from `start` (inclusive) via successor edges.
    fn reachable(&self, start: BlockId) -> BTreeSet<BlockId> {
        let mut seen = BTreeSet::new();
        let mut stack = vec![start];
        while let Some(b) = stack.pop() {
            if !seen.insert(b) {
                continue;
            }
            if let Some(block) = self.cfg.block(b) {
                for s in block.terminator.successors() {
                    stack.push(s);
                }
            }
        }
        seen
    }

    /// Recover a `try` / `catch` / `finally` from a `TryEntry` terminator. The
    /// body, catch, and finally regions are structured independently; the
    /// construct resumes at the `EndTry` continuation (the post-try merge).
    fn handle_try(
        &self,
        out: &mut IrBlock,
        bid: BlockId,
        body_target: BlockId,
        catch_target: Option<BlockId>,
        finally_target: Option<BlockId>,
        visited: &mut HashSet<BlockId>,
    ) -> Option<BlockId> {
        // The shared EndTry block executes after whichever protected region was
        // selected. Keep it outside every individual arm.
        let end_try = self.find_endtry(body_target);
        let continuation = end_try.map(|(_, continuation)| continuation);

        // Handlers are boundaries for the body (and vice-versa) so each region
        // is structured in isolation.
        let mut body_stop: HashSet<BlockId> = HashSet::new();
        if let Some(c) = catch_target {
            body_stop.insert(c);
        }
        if let Some(f) = finally_target {
            body_stop.insert(f);
        }
        if let Some((end_try, _)) = end_try {
            body_stop.insert(end_try);
        }
        let try_body = self.structure_set_edge_region(bid, body_target, &body_stop, visited);

        let catch_body = catch_target.map(|c| {
            let mut stop = HashSet::new();
            if let Some(f) = finally_target {
                stop.insert(f);
            }
            if let Some((end_try, _)) = end_try {
                stop.insert(end_try);
            }
            self.structure_set_edge_region(bid, c, &stop, visited)
        });
        let finally_body = finally_target.map(|f| {
            let mut stop = HashSet::new();
            if let Some((end_try, _)) = end_try {
                stop.insert(end_try);
            }
            self.structure_set_edge_region(bid, f, &stop, visited)
        });

        out.push(Stmt::ControlFlow(Box::new(ControlFlow::try_catch(
            try_body,
            None,
            catch_body,
            finally_body,
        ))));
        if let Some((end_try, _)) = end_try {
            if visited.insert(end_try) {
                self.emit_body(out, end_try);
            }
        }
        continuation
    }

    /// Find the shared `EndTry` block reachable from `start` and its
    /// continuation. Returns `None` when no `EndTry` is reachable.
    fn find_endtry(&self, start: BlockId) -> Option<(BlockId, BlockId)> {
        let mut seen = BTreeSet::new();
        let mut stack = vec![start];
        while let Some(b) = stack.pop() {
            if !seen.insert(b) {
                continue;
            }
            if let Some(block) = self.cfg.block(b) {
                if let Terminator::EndTry { continuation } = &block.terminator {
                    return Some((b, *continuation));
                }
                for s in block.terminator.successors() {
                    stack.push(s);
                }
            }
        }
        None
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
                | Terminator::Unknown
                | Terminator::EndTry { .. }
                | Terminator::TryEntry { .. } => None,
                Terminator::Fallthrough { target } | Terminator::Jump { target } => Some(target),
                Terminator::Branch {
                    then_target,
                    else_target,
                } => {
                    if self.loop_headers.contains(&bid) {
                        let cond = self
                            .comparison_condition_for_block(bid)
                            .unwrap_or_else(|| self.condition_for_block(bid));
                        let mut body =
                            self.structure_edge_region(bid, then_target, Some(bid), visited);
                        self.build_loop(&mut out, bid, cond, &mut body);
                        out.stmts
                            .extend(self.phi_lowering.edge_statements(bid, else_target));
                        Some(else_target)
                    } else {
                        self.handle_branch(&mut out, bid, then_target, else_target, None, visited)
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

    /// Render a branch-headed natural loop with no outgoing edge as an
    /// unconditional loop. In this shape both branch successors stay inside
    /// the loop, so treating one as the exit changes program semantics.
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
        if !members.contains(&then_target)
            || !members.contains(&else_target)
            || members.iter().any(|member| {
                self.cfg
                    .successors(*member)
                    .iter()
                    .any(|successor| !members.contains(successor))
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

    /// Compute the standard natural-loop node set for all back-edges entering
    /// `header` by walking predecessor edges from each dominated latch.
    fn natural_loop_blocks(&self, header: BlockId) -> HashSet<BlockId> {
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

    /// Find the first reconvergence of two loop-internal branch arms without
    /// traversing through the header and beginning another iteration.
    fn closest_loop_merge(
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

    /// Compute nodes that lie on every path from both branch arms back to the
    /// loop header. Dominance in the loop's reverse graph is post-dominance in
    /// the original graph, so validation is computed once per loop rather than
    /// traversing the graph separately for every merge candidate.
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

    fn terminator(&self, bid: BlockId) -> Terminator {
        self.cfg
            .block(bid)
            .map(|b| b.terminator.clone())
            .unwrap_or(Terminator::Unknown)
    }
}

/// Result of recognising a switch cascade: scrutinee, cases, optional default,
/// and the merge block to continue from.
struct SwitchResult {
    scrutinee: Expr,
    cases: Vec<(Expr, IrBlock)>,
    default: Option<IrBlock>,
    merge: Option<BlockId>,
}

/// If `expr` is an equality comparison `scrut == literal` (either order), return
/// the scrutinee's base variable name and the literal operand. Used to recognise
/// switch-case comparisons; the base name lets cases match across SSA versions.
fn extract_eq_cond(expr: &SsaExpr) -> Option<(String, SsaExpr)> {
    use crate::decompiler::ir::BinOp;
    let SsaExpr::Binary { op, left, right } = expr else {
        return None;
    };
    if !matches!(*op, BinOp::Eq) {
        return None;
    }
    match (left.as_ref(), right.as_ref()) {
        (SsaExpr::Variable(v), lit) if is_literal(lit) => {
            Some((strip_version(&var_name(v)), lit.clone()))
        }
        (lit, SsaExpr::Variable(v)) if is_literal(lit) => {
            Some((strip_version(&var_name(v)), lit.clone()))
        }
        _ => None,
    }
}

fn is_literal(e: &SsaExpr) -> bool {
    matches!(e, SsaExpr::Literal(_))
}

/// Strip an SSA variable's version suffix (`loc0_3` → `loc0`) so switch cases
/// match on the slot identity regardless of which version each comparison reads.
fn strip_version(name: &str) -> String {
    match name.rsplit_once('_') {
        Some((base, digits))
            if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) =>
        {
            base.to_string()
        }
        _ => name.to_string(),
    }
}

/// Display name for an SSA variable (base + version), matching the optimized
/// SSA renderer so cross-views stay consistent.
fn var_name(var: &SsaVariable) -> String {
    if var.base == "?" {
        "?".to_string()
    } else {
        format!("{}_{}", var.base, var.version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decompiler::cfg::ssa::DominanceInfo;
    use crate::decompiler::cfg::ssa::SsaForm;
    use crate::decompiler::cfg::ssa::{PhiNode, SsaBlock, SsaExpr, SsaStmt, SsaVariable};
    use crate::decompiler::cfg::{BasicBlock, BlockId, Cfg, EdgeKind, Terminator};
    use crate::decompiler::ir::{BinOp, Literal, Stmt};

    fn v(base: &str, n: usize) -> SsaVariable {
        SsaVariable::new(base.to_string(), n)
    }

    /// Build a diamond: BB0 branches to BB1/BB2, both jump to BB3 (merge/ret).
    fn diamond_cfg() -> Cfg {
        let mut cfg = Cfg::new();
        cfg.add_block(BasicBlock::new(
            BlockId(0),
            0,
            1,
            0..1,
            Terminator::Branch {
                then_target: BlockId(1),
                else_target: BlockId(2),
            },
        ));
        cfg.add_block(BasicBlock::new(
            BlockId(1),
            1,
            2,
            1..2,
            Terminator::Jump { target: BlockId(3) },
        ));
        cfg.add_block(BasicBlock::new(
            BlockId(2),
            2,
            3,
            2..3,
            Terminator::Jump { target: BlockId(3) },
        ));
        cfg.add_block(BasicBlock::new(BlockId(3), 3, 4, 3..4, Terminator::Return));
        cfg.add_edge(BlockId(0), BlockId(1), EdgeKind::ConditionalTrue);
        cfg.add_edge(BlockId(0), BlockId(2), EdgeKind::ConditionalFalse);
        cfg.add_edge(BlockId(1), BlockId(3), EdgeKind::Unconditional);
        cfg.add_edge(BlockId(2), BlockId(3), EdgeKind::Unconditional);
        cfg
    }

    fn block_with(stmts: Vec<SsaStmt>) -> SsaBlock {
        let mut b = SsaBlock::new();
        for s in stmts {
            b.add_stmt(s);
        }
        b
    }

    fn phi(target: SsaVariable, operands: &[(BlockId, SsaVariable)]) -> PhiNode {
        let mut phi = PhiNode::new(target);
        for (predecessor, operand) in operands {
            phi.add_operand(*predecessor, operand.clone());
        }
        phi
    }

    fn block_contains_call(block: &IrBlock, expected: &str) -> bool {
        block
            .stmts
            .iter()
            .any(|stmt| stmt_contains_call(stmt, expected))
    }

    fn stmt_contains_call(stmt: &Stmt, expected: &str) -> bool {
        match stmt {
            Stmt::Assign { value, .. } | Stmt::ExprStmt(value) => {
                expr_contains_call(value, expected)
            }
            Stmt::Return(value) => value
                .as_ref()
                .is_some_and(|value| expr_contains_call(value, expected)),
            Stmt::ControlFlow(control_flow) => control_flow_contains_call(control_flow, expected),
            Stmt::Comment(_) => false,
        }
    }

    fn control_flow_contains_call(control_flow: &ControlFlow, expected: &str) -> bool {
        match control_flow {
            ControlFlow::If {
                condition,
                then_branch,
                else_branch,
            } => {
                expr_contains_call(condition, expected)
                    || block_contains_call(then_branch, expected)
                    || else_branch
                        .as_ref()
                        .is_some_and(|branch| block_contains_call(branch, expected))
            }
            ControlFlow::While { condition, body } | ControlFlow::DoWhile { body, condition } => {
                expr_contains_call(condition, expected) || block_contains_call(body, expected)
            }
            ControlFlow::For {
                init,
                condition,
                update,
                body,
            } => {
                init.as_ref()
                    .is_some_and(|stmt| stmt_contains_call(stmt, expected))
                    || condition
                        .as_ref()
                        .is_some_and(|expr| expr_contains_call(expr, expected))
                    || update
                        .as_ref()
                        .is_some_and(|expr| expr_contains_call(expr, expected))
                    || block_contains_call(body, expected)
            }
            ControlFlow::TryCatch {
                try_body,
                catch_body,
                finally_body,
                ..
            } => {
                block_contains_call(try_body, expected)
                    || catch_body
                        .as_ref()
                        .is_some_and(|body| block_contains_call(body, expected))
                    || finally_body
                        .as_ref()
                        .is_some_and(|body| block_contains_call(body, expected))
            }
            ControlFlow::Switch {
                expr,
                cases,
                default,
            } => {
                expr_contains_call(expr, expected)
                    || cases.iter().any(|(value, body)| {
                        expr_contains_call(value, expected) || block_contains_call(body, expected)
                    })
                    || default
                        .as_ref()
                        .is_some_and(|body| block_contains_call(body, expected))
            }
        }
    }

    fn expr_contains_call(expr: &Expr, expected: &str) -> bool {
        match expr {
            Expr::Call { name, args } => {
                name == expected
                    || args
                        .iter()
                        .any(|argument| expr_contains_call(argument, expected))
            }
            Expr::Binary { left, right, .. } => {
                expr_contains_call(left, expected) || expr_contains_call(right, expected)
            }
            Expr::Unary { operand, .. } => expr_contains_call(operand, expected),
            Expr::Index { base, index } => {
                expr_contains_call(base, expected) || expr_contains_call(index, expected)
            }
            Expr::Member { base, .. } => expr_contains_call(base, expected),
            Expr::Cast { expr, .. } => expr_contains_call(expr, expected),
            Expr::Array(elements) => elements
                .iter()
                .any(|element| expr_contains_call(element, expected)),
            Expr::Map(pairs) => pairs.iter().any(|(key, value)| {
                expr_contains_call(key, expected) || expr_contains_call(value, expected)
            }),
            Expr::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                expr_contains_call(condition, expected)
                    || expr_contains_call(then_expr, expected)
                    || expr_contains_call(else_expr, expected)
            }
            Expr::Literal(_) | Expr::Variable(_) | Expr::StackTemp(_) => false,
        }
    }

    fn entry_self_loop_structure() -> IrBlock {
        const VIRTUAL_ENTRY: BlockId = BlockId(usize::MAX);

        let entry = BlockId(0);
        let mut cfg = Cfg::new();
        cfg.add_block(BasicBlock::new(
            entry,
            0,
            1,
            0..1,
            Terminator::Jump { target: entry },
        ));
        cfg.add_edge(entry, entry, EdgeKind::Unconditional);

        let state = v("state", 1);
        let initial = v("initial", 0);
        let next = v("next", 0);
        let mut block = block_with(vec![
            SsaStmt::assign(next.clone(), SsaExpr::lit(Literal::Int(2))),
            SsaStmt::expr(SsaExpr::call(
                "check".to_string(),
                vec![SsaExpr::var(state.clone())],
            )),
        ]);
        block.add_phi(phi(
            state.clone(),
            &[(VIRTUAL_ENTRY, initial.clone()), (entry, next.clone())],
        ));
        let ssa = SsaForm {
            dominance: crate::decompiler::cfg::ssa::compute(&cfg),
            cfg,
            blocks: BTreeMap::from([(entry, block)]),
            definitions: BTreeMap::new(),
            uses: BTreeMap::from([(state, BTreeSet::from([UseSite::new(entry, 1)]))]),
        };

        structure(&ssa)
    }

    #[test]
    fn structure_initializes_virtual_entry_phi_once() {
        let structured = entry_self_loop_structure();

        assert_eq!(
            structured.stmts,
            vec![
                Stmt::Assign {
                    target: "state_1".to_string(),
                    value: Expr::var("initial_0"),
                },
                Stmt::Assign {
                    target: "next_0".to_string(),
                    value: Expr::int(2),
                },
                Stmt::ExprStmt(Expr::call("check", vec![Expr::var("state_1")])),
                Stmt::Assign {
                    target: "state_1".to_string(),
                    value: Expr::var("next_0"),
                },
            ]
        );
        assert!(!block_contains_call(&structured, "phi"));
    }

    #[test]
    fn entry_self_loop_keeps_virtual_initialization_separate() {
        let structured = entry_self_loop_structure();

        assert!(matches!(
            structured.stmts.first(),
            Some(Stmt::Assign {
                target,
                value: Expr::Variable(source),
            }) if target == "state_1" && source == "initial_0"
        ));
        assert!(matches!(
            structured.stmts.last(),
            Some(Stmt::Assign {
                target,
                value: Expr::Variable(source),
            }) if target == "state_1" && source == "next_0"
        ));
        assert_eq!(
            structured
                .stmts
                .iter()
                .filter(|stmt| matches!(stmt, Stmt::Assign { target, .. } if target == "state_1"))
                .count(),
            2
        );
        assert!(!block_contains_call(&structured, "phi"));
    }

    #[test]
    fn structure_emits_jump_edge_copy_before_merge_body() {
        let source = BlockId(0);
        let merge = BlockId(1);
        let mut cfg = Cfg::new();
        cfg.add_block(BasicBlock::new(
            source,
            0,
            1,
            0..1,
            Terminator::Jump { target: merge },
        ));
        cfg.add_block(BasicBlock::new(merge, 1, 2, 1..2, Terminator::Return));
        cfg.add_edge(source, merge, EdgeKind::Unconditional);

        let incoming = v("incoming", 0);
        let merged = v("merged", 0);
        let source_block = block_with(vec![SsaStmt::assign(
            incoming.clone(),
            SsaExpr::lit(Literal::Int(7)),
        )]);
        let mut merge_block = block_with(vec![
            SsaStmt::expr(SsaExpr::call(
                "check".to_string(),
                vec![SsaExpr::var(merged.clone())],
            )),
            SsaStmt::ret(None),
        ]);
        merge_block.add_phi(phi(merged.clone(), &[(source, incoming)]));
        let ssa = SsaForm {
            dominance: crate::decompiler::cfg::ssa::compute(&cfg),
            cfg,
            blocks: BTreeMap::from([(source, source_block), (merge, merge_block)]),
            definitions: BTreeMap::new(),
            uses: BTreeMap::from([(merged, BTreeSet::from([UseSite::new(merge, 0)]))]),
        };

        let structured = structure(&ssa);

        assert_eq!(
            structured.stmts,
            vec![
                Stmt::Assign {
                    target: "incoming_0".to_string(),
                    value: Expr::int(7),
                },
                Stmt::Assign {
                    target: "merged_0".to_string(),
                    value: Expr::var("incoming_0"),
                },
                Stmt::ExprStmt(Expr::call("check", vec![Expr::var("merged_0")])),
                Stmt::Return(None),
            ]
        );
        assert!(!block_contains_call(&structured, "phi"));
    }

    fn single_block_ssa(
        statements: Vec<SsaStmt>,
        uses: BTreeMap<SsaVariable, BTreeSet<UseSite>>,
    ) -> SsaForm {
        let mut cfg = Cfg::new();
        cfg.add_block(BasicBlock::new(BlockId(0), 0, 1, 0..1, Terminator::Return));
        let dominance = crate::decompiler::cfg::ssa::compute(&cfg);
        let blocks = BTreeMap::from([(BlockId(0), block_with(statements))]);
        SsaForm {
            cfg,
            dominance,
            blocks,
            definitions: BTreeMap::new(),
            uses,
        }
    }

    #[test]
    fn adjacent_single_use_call_temp_is_returned_directly() {
        let temp = v("t", 0);
        let statements = vec![
            SsaStmt::assign(temp.clone(), SsaExpr::call("read".to_string(), vec![])),
            SsaStmt::ret(Some(SsaExpr::var(temp.clone()))),
        ];
        let uses = BTreeMap::from([(temp, BTreeSet::from([UseSite::new(BlockId(0), 1)]))]);

        let structured = structure(&single_block_ssa(statements, uses));

        assert_eq!(
            structured.stmts,
            vec![Stmt::Return(Some(Expr::call("read", vec![])))]
        );
    }

    #[test]
    fn unused_call_temp_is_an_expression_statement() {
        let structured = structure(&single_block_ssa(
            vec![
                SsaStmt::assign(v("t", 0), SsaExpr::call("notify".to_string(), vec![])),
                SsaStmt::ret(None),
            ],
            BTreeMap::new(),
        ));

        assert_eq!(
            structured.stmts,
            vec![
                Stmt::ExprStmt(Expr::call("notify", vec![])),
                Stmt::Return(None),
            ]
        );
    }

    #[test]
    fn missing_use_index_keeps_referenced_call_temp_assigned() {
        let temp = v("t", 0);
        let structured = structure(&single_block_ssa(
            vec![
                SsaStmt::assign(temp.clone(), SsaExpr::call("read".to_string(), vec![])),
                SsaStmt::ret(Some(SsaExpr::var(temp))),
            ],
            BTreeMap::new(),
        ));

        assert!(matches!(
            structured.stmts.as_slice(),
            [
                Stmt::Assign { target, .. },
                Stmt::Return(Some(Expr::Variable(returned)))
            ] if target == "t_0" && returned == "t_0"
        ));
    }

    #[test]
    fn missing_cross_block_use_index_keeps_call_temp_assigned() {
        let temp = v("t", 0);
        let mut cfg = Cfg::new();
        cfg.add_block(BasicBlock::new(
            BlockId(0),
            0,
            1,
            0..1,
            Terminator::Jump { target: BlockId(1) },
        ));
        cfg.add_block(BasicBlock::new(BlockId(1), 1, 2, 1..2, Terminator::Return));
        cfg.add_edge(BlockId(0), BlockId(1), EdgeKind::Unconditional);
        let dominance = crate::decompiler::cfg::ssa::compute(&cfg);
        let blocks = BTreeMap::from([
            (
                BlockId(0),
                block_with(vec![SsaStmt::assign(
                    temp.clone(),
                    SsaExpr::call("read".to_string(), vec![]),
                )]),
            ),
            (
                BlockId(1),
                block_with(vec![SsaStmt::ret(Some(SsaExpr::var(temp)))]),
            ),
        ]);
        let ssa = SsaForm {
            cfg,
            dominance,
            blocks,
            definitions: BTreeMap::new(),
            uses: BTreeMap::new(),
        };

        let structured = structure(&ssa);

        assert!(matches!(
            structured.stmts.as_slice(),
            [
                Stmt::Assign { target, .. },
                Stmt::Return(Some(Expr::Variable(returned)))
            ] if target == "t_0" && returned == "t_0"
        ));
    }

    #[test]
    fn multi_use_call_temp_remains_assigned() {
        let temp = v("t", 0);
        let structured = structure(&single_block_ssa(
            vec![
                SsaStmt::assign(temp.clone(), SsaExpr::call("read".to_string(), vec![])),
                SsaStmt::assign(v("loc0", 0), SsaExpr::var(temp.clone())),
                SsaStmt::ret(Some(SsaExpr::var(temp.clone()))),
            ],
            BTreeMap::from([(
                temp,
                BTreeSet::from([UseSite::new(BlockId(0), 1), UseSite::new(BlockId(0), 2)]),
            )]),
        ));

        assert!(matches!(
            structured.stmts.first(),
            Some(Stmt::Assign { target, .. }) if target == "t_0"
        ));
    }

    #[test]
    fn named_slot_call_remains_assigned_when_unused() {
        let structured = structure(&single_block_ssa(
            vec![
                SsaStmt::assign(v("loc0", 0), SsaExpr::call("read".to_string(), vec![])),
                SsaStmt::ret(None),
            ],
            BTreeMap::new(),
        ));

        assert!(matches!(
            structured.stmts.first(),
            Some(Stmt::Assign { target, .. }) if target == "loc0_0"
        ));
    }

    #[test]
    fn unused_non_call_temp_remains_assigned() {
        let structured = structure(&single_block_ssa(
            vec![
                SsaStmt::assign(v("t", 0), SsaExpr::lit(Literal::Int(7))),
                SsaStmt::ret(None),
            ],
            BTreeMap::new(),
        ));

        assert!(matches!(
            structured.stmts.first(),
            Some(Stmt::Assign { target, .. }) if target == "t_0"
        ));
    }

    #[test]
    fn call_temp_used_as_call_argument_remains_assigned() {
        let temp = v("t", 0);
        let structured = structure(&single_block_ssa(
            vec![
                SsaStmt::assign(temp.clone(), SsaExpr::call("read".to_string(), vec![])),
                SsaStmt::expr(SsaExpr::call(
                    "consume".to_string(),
                    vec![SsaExpr::var(temp.clone())],
                )),
                SsaStmt::ret(None),
            ],
            BTreeMap::from([(temp, BTreeSet::from([UseSite::new(BlockId(0), 1)]))]),
        ));

        assert!(matches!(
            structured.stmts.first(),
            Some(Stmt::Assign { target, .. }) if target == "t_0"
        ));
    }

    #[test]
    fn bypassable_loop_node_is_not_a_shared_merge() {
        // B0 is an infinite-loop header. B1 can reach B3 or bypass it and
        // return directly to B0, while B2 always reaches B3. Reachability from
        // both arms is therefore insufficient to make B3 a shared tail.
        let mut cfg = Cfg::new();
        cfg.add_block(BasicBlock::new(
            BlockId(0),
            0,
            1,
            0..1,
            Terminator::Branch {
                then_target: BlockId(1),
                else_target: BlockId(2),
            },
        ));
        cfg.add_block(BasicBlock::new(
            BlockId(1),
            1,
            2,
            1..2,
            Terminator::Branch {
                then_target: BlockId(3),
                else_target: BlockId(0),
            },
        ));
        cfg.add_block(BasicBlock::new(
            BlockId(2),
            2,
            3,
            2..3,
            Terminator::Jump { target: BlockId(3) },
        ));
        cfg.add_block(BasicBlock::new(
            BlockId(3),
            3,
            4,
            3..4,
            Terminator::Jump { target: BlockId(0) },
        ));
        cfg.add_edge(BlockId(0), BlockId(1), EdgeKind::ConditionalTrue);
        cfg.add_edge(BlockId(0), BlockId(2), EdgeKind::ConditionalFalse);
        cfg.add_edge(BlockId(1), BlockId(3), EdgeKind::ConditionalTrue);
        cfg.add_edge(BlockId(1), BlockId(0), EdgeKind::ConditionalFalse);
        cfg.add_edge(BlockId(2), BlockId(3), EdgeKind::Unconditional);
        cfg.add_edge(BlockId(3), BlockId(0), EdgeKind::Unconditional);

        let dominance = crate::decompiler::cfg::ssa::compute(&cfg);
        let loop_headers = compute_loop_headers(&cfg, &dominance);
        let mut blocks = std::collections::BTreeMap::new();
        blocks.insert(
            BlockId(0),
            block_with(vec![SsaStmt::assign(
                v("header_cond", 0),
                SsaExpr::var(v("arg0", 0)),
            )]),
        );
        blocks.insert(
            BlockId(1),
            block_with(vec![SsaStmt::assign(
                v("inner_cond", 0),
                SsaExpr::var(v("arg1", 0)),
            )]),
        );
        blocks.insert(
            BlockId(2),
            block_with(vec![SsaStmt::assign(
                v("else_value", 0),
                SsaExpr::lit(Literal::Int(7)),
            )]),
        );
        blocks.insert(
            BlockId(3),
            block_with(vec![SsaStmt::assign(
                v("shared", 0),
                SsaExpr::lit(Literal::Int(42)),
            )]),
        );
        let ssa = SsaForm {
            cfg: cfg.clone(),
            dominance,
            blocks,
            definitions: std::collections::BTreeMap::new(),
            uses: std::collections::BTreeMap::new(),
        };
        let source_names = BTreeMap::new();
        let structural_uses = collect_structural_uses(&ssa);
        let phi_lowering = PhiLowering::new(&ssa, &source_names);
        let ctx = StructCtx {
            cfg: &ssa.cfg,
            ssa: &ssa,
            source_names: &source_names,
            loop_headers,
            structural_uses,
            phi_lowering,
        };
        let members = ctx.natural_loop_blocks(BlockId(0));

        assert_eq!(
            ctx.closest_loop_merge(BlockId(1), BlockId(2), BlockId(0), &members),
            None,
            "B3 is reachable from both arms but does not post-dominate B1"
        );

        let rendered = crate::decompiler::ir::render_block(&structure(&ssa), 0);
        assert_eq!(
            rendered.matches("shared_0 = 42;").count(),
            2,
            "bypassable shared code must remain in every branch that executes it:\n{rendered}"
        );
    }

    #[test]
    fn structures_a_diamond_into_an_if_else() {
        let cfg = diamond_cfg();
        let mut blocks = std::collections::BTreeMap::new();
        // BB0: condition def  b0_0 = (loc0 < 1)
        blocks.insert(
            BlockId(0),
            block_with(vec![SsaStmt::assign(
                v("b0", 0),
                SsaExpr::binary(
                    BinOp::Lt,
                    SsaExpr::var(v("loc0", 0)),
                    SsaExpr::lit(Literal::Int(1)),
                ),
            )]),
        );
        // BB1 (then): b1_0 = 10
        blocks.insert(
            BlockId(1),
            block_with(vec![SsaStmt::assign(
                v("b1", 0),
                SsaExpr::lit(Literal::Int(10)),
            )]),
        );
        // BB2 (else): b2_0 = 20
        blocks.insert(
            BlockId(2),
            block_with(vec![SsaStmt::assign(
                v("b2", 0),
                SsaExpr::lit(Literal::Int(20)),
            )]),
        );
        blocks.insert(BlockId(3), SsaBlock::new());

        let ssa = SsaForm {
            cfg,
            dominance: DominanceInfo::new(),
            blocks,
            definitions: std::collections::BTreeMap::new(),
            uses: std::collections::BTreeMap::new(),
        };

        let structured = structure(&ssa);

        // Expect: condition assign, then an If ControlFlow with both branches.
        let has_if = structured
            .stmts
            .iter()
            .any(|s| matches!(s, Stmt::ControlFlow(cf) if matches!(**cf, ControlFlow::If { .. })));
        assert!(
            has_if,
            "expected an If ControlFlow; got {:?}",
            structured.stmts
        );

        let if_cf = structured
            .stmts
            .iter()
            .rev()
            .find_map(|s| match s {
                Stmt::ControlFlow(cf) => Some(cf),
                _ => None,
            })
            .expect("an If node");
        let ControlFlow::If {
            then_branch,
            else_branch,
            ..
        } = if_cf.as_ref()
        else {
            panic!("expected If, got {if_cf:?}");
        };
        assert!(!then_branch.is_empty(), "then-branch should carry BB1 body");
        assert!(
            else_branch.is_some(),
            "an if-else diamond should yield an else branch"
        );
    }

    #[test]
    fn direct_branch_to_merge_copy_stays_inside_selected_arm() {
        let branch = BlockId(0);
        let merge = BlockId(1);
        let indirect = BlockId(2);
        let mut cfg = Cfg::new();
        cfg.add_block(BasicBlock::new(
            branch,
            0,
            1,
            0..1,
            Terminator::Branch {
                then_target: merge,
                else_target: indirect,
            },
        ));
        cfg.add_block(BasicBlock::new(merge, 1, 2, 1..2, Terminator::Return));
        cfg.add_block(BasicBlock::new(
            indirect,
            2,
            3,
            2..3,
            Terminator::Jump { target: merge },
        ));
        cfg.add_edge(branch, merge, EdgeKind::ConditionalTrue);
        cfg.add_edge(branch, indirect, EdgeKind::ConditionalFalse);
        cfg.add_edge(indirect, merge, EdgeKind::Unconditional);

        let direct_value = v("direct", 0);
        let indirect_value = v("indirect", 0);
        let condition = v("condition", 0);
        let merged = v("merged", 0);
        let branch_block = block_with(vec![
            SsaStmt::assign(direct_value.clone(), SsaExpr::lit(Literal::Int(10))),
            SsaStmt::assign(condition.clone(), SsaExpr::var(v("arg0", 0))),
        ]);
        let indirect_block = block_with(vec![SsaStmt::assign(
            indirect_value.clone(),
            SsaExpr::lit(Literal::Int(20)),
        )]);
        let mut merge_block = block_with(vec![
            SsaStmt::expr(SsaExpr::call(
                "check".to_string(),
                vec![SsaExpr::var(merged.clone())],
            )),
            SsaStmt::ret(None),
        ]);
        merge_block.add_phi(phi(
            merged.clone(),
            &[(branch, direct_value), (indirect, indirect_value)],
        ));
        let ssa = SsaForm {
            dominance: crate::decompiler::cfg::ssa::compute(&cfg),
            cfg,
            blocks: BTreeMap::from([
                (branch, branch_block),
                (merge, merge_block),
                (indirect, indirect_block),
            ]),
            definitions: BTreeMap::new(),
            uses: BTreeMap::from([
                (condition, BTreeSet::from([UseSite::terminator(branch)])),
                (merged, BTreeSet::from([UseSite::new(merge, 0)])),
            ]),
        };

        let structured = structure(&ssa);
        let branch_stmt = structured
            .stmts
            .iter()
            .find_map(|stmt| match stmt {
                Stmt::ControlFlow(control_flow) => Some(control_flow.as_ref()),
                _ => None,
            })
            .expect("structured branch");
        let ControlFlow::If {
            then_branch,
            else_branch: Some(else_branch),
            ..
        } = branch_stmt
        else {
            panic!("expected an if/else branch, got {branch_stmt:?}");
        };

        assert_eq!(
            then_branch.stmts,
            vec![Stmt::Assign {
                target: "merged_0".to_string(),
                value: Expr::var("direct_0"),
            }],
            "the direct critical-edge copy must stay in the selected arm"
        );
        assert!(else_branch.stmts.contains(&Stmt::Assign {
            target: "merged_0".to_string(),
            value: Expr::var("indirect_0"),
        }));
        assert!(!block_contains_call(&structured, "phi"));
    }

    #[test]
    fn degenerate_same_target_branch_emits_one_edge_copy() {
        let branch = BlockId(0);
        let target = BlockId(1);
        let mut cfg = Cfg::new();
        cfg.add_block(BasicBlock::new(
            branch,
            0,
            1,
            0..1,
            Terminator::Branch {
                then_target: target,
                else_target: target,
            },
        ));
        cfg.add_block(BasicBlock::new(target, 1, 2, 1..2, Terminator::Return));
        cfg.add_edge(branch, target, EdgeKind::ConditionalTrue);
        cfg.add_edge(branch, target, EdgeKind::ConditionalFalse);

        let incoming = v("incoming", 0);
        let condition = v("condition", 0);
        let merged = v("merged", 0);
        let mut target_block = block_with(vec![SsaStmt::ret(Some(SsaExpr::var(merged.clone())))]);
        target_block.add_phi(phi(merged.clone(), &[(branch, incoming.clone())]));
        let ssa = SsaForm {
            dominance: crate::decompiler::cfg::ssa::compute(&cfg),
            cfg,
            blocks: BTreeMap::from([
                (
                    branch,
                    block_with(vec![
                        SsaStmt::assign(incoming, SsaExpr::lit(Literal::Int(4))),
                        SsaStmt::assign(condition.clone(), SsaExpr::var(v("arg0", 0))),
                    ]),
                ),
                (target, target_block),
            ]),
            definitions: BTreeMap::new(),
            uses: BTreeMap::from([
                (condition, BTreeSet::from([UseSite::terminator(branch)])),
                (merged, BTreeSet::from([UseSite::new(target, 0)])),
            ]),
        };

        let structured = structure(&ssa);
        let copy = Stmt::Assign {
            target: "merged_0".to_string(),
            value: Expr::var("incoming_0"),
        };
        assert_eq!(
            structured
                .stmts
                .iter()
                .filter(|stmt| *stmt == &copy)
                .count(),
            1
        );
        let copy_index = structured
            .stmts
            .iter()
            .position(|stmt| stmt == &copy)
            .expect("degenerate edge copy");
        assert!(matches!(
            structured.stmts.get(copy_index + 1),
            Some(Stmt::Return(Some(Expr::Variable(value)))) if value == "merged_0"
        ));
        assert!(!block_contains_call(&structured, "phi"));
    }

    #[test]
    fn analysis_ssa_retains_phi_while_structured_ir_lowers_it() {
        let cfg = diamond_cfg();
        let left = v("left", 0);
        let right = v("right", 0);
        let merged = v("merged", 0);
        let mut merge_block = block_with(vec![
            SsaStmt::expr(SsaExpr::call(
                "consume".to_string(),
                vec![SsaExpr::var(merged.clone())],
            )),
            SsaStmt::ret(None),
        ]);
        merge_block.add_phi(phi(
            merged.clone(),
            &[(BlockId(1), left.clone()), (BlockId(2), right.clone())],
        ));
        let ssa = SsaForm {
            dominance: crate::decompiler::cfg::ssa::compute(&cfg),
            cfg,
            blocks: BTreeMap::from([
                (
                    BlockId(0),
                    block_with(vec![SsaStmt::assign(
                        v("condition", 0),
                        SsaExpr::var(v("arg0", 0)),
                    )]),
                ),
                (
                    BlockId(1),
                    block_with(vec![SsaStmt::assign(left, SsaExpr::lit(Literal::Int(1)))]),
                ),
                (
                    BlockId(2),
                    block_with(vec![SsaStmt::assign(right, SsaExpr::lit(Literal::Int(2)))]),
                ),
                (BlockId(3), merge_block),
            ]),
            definitions: BTreeMap::new(),
            uses: BTreeMap::from([(merged, BTreeSet::from([UseSite::new(BlockId(3), 0)]))]),
        };

        let analysis = crate::decompiler::cfg::ssa::render_ssa_form(&ssa);
        assert!(
            analysis.contains("merged_0 = φ(1: left_0, 2: right_0)"),
            "analysis SSA must retain predecessor-labelled phi semantics:\n{analysis}"
        );

        let structured = structure(&ssa);
        assert!(!block_contains_call(&structured, "phi"));
    }

    /// The if-condition must inline the comparison that drives the branch, not
    /// render the bare reaching-definition variable. When BB0's last def is
    /// `t = (loc0 < 1)`, the condition must be `(loc0 < 1)` and the def must
    /// NOT be duplicated as a body statement.
    #[test]
    fn inlines_branch_comparison_condition_and_does_not_duplicate_it() {
        let mut cfg = Cfg::new();
        cfg.add_block(BasicBlock::new(
            BlockId(0),
            0,
            1,
            0..1,
            Terminator::Branch {
                then_target: BlockId(1),
                else_target: BlockId(2),
            },
        ));
        cfg.add_block(BasicBlock::new(
            BlockId(1),
            1,
            2,
            1..2,
            Terminator::Jump { target: BlockId(3) },
        ));
        cfg.add_block(BasicBlock::new(
            BlockId(2),
            2,
            3,
            2..3,
            Terminator::Jump { target: BlockId(3) },
        ));
        cfg.add_block(BasicBlock::new(BlockId(3), 3, 4, 3..4, Terminator::Return));
        cfg.add_edge(BlockId(0), BlockId(1), EdgeKind::ConditionalTrue);
        cfg.add_edge(BlockId(0), BlockId(2), EdgeKind::ConditionalFalse);
        cfg.add_edge(BlockId(1), BlockId(3), EdgeKind::Unconditional);
        cfg.add_edge(BlockId(2), BlockId(3), EdgeKind::Unconditional);

        let mut blocks = std::collections::BTreeMap::new();
        // BB0: only the comparison def — it IS the branch condition.
        blocks.insert(
            BlockId(0),
            block_with(vec![SsaStmt::assign(
                v("t", 0),
                SsaExpr::binary(
                    BinOp::Lt,
                    SsaExpr::var(v("loc0", 0)),
                    SsaExpr::lit(Literal::Int(1)),
                ),
            )]),
        );
        blocks.insert(
            BlockId(1),
            block_with(vec![SsaStmt::assign(
                v("b1", 0),
                SsaExpr::lit(Literal::Int(10)),
            )]),
        );
        blocks.insert(
            BlockId(2),
            block_with(vec![SsaStmt::assign(
                v("b2", 0),
                SsaExpr::lit(Literal::Int(20)),
            )]),
        );
        blocks.insert(BlockId(3), SsaBlock::new());

        let ssa = SsaForm {
            cfg,
            dominance: DominanceInfo::new(),
            blocks,
            definitions: std::collections::BTreeMap::new(),
            uses: std::collections::BTreeMap::new(),
        };

        let structured = structure(&ssa);
        let rendered = crate::decompiler::ir::render_block(&structured, 0);

        // The condition must be the inlined comparison (versioned SSA name `loc0_0`,
        // double parens are a renderer quirk around a parenthesised Binary).
        assert!(
            rendered.contains("loc0_0 < 1"),
            "branch condition should inline the comparison; got:\n{rendered}"
        );
        // And it must NOT render the bare reaching-definition variable as the
        // condition.
        assert!(
            !rendered.contains("if (t_0)") && !rendered.contains("if (t)"),
            "branch condition should not be the bare t_0; got:\n{rendered}"
        );
        // The comparison def must not be duplicated as a body assignment.
        assert!(
            !rendered.contains("t_0 ="),
            "the comparison def must be consumed by the condition, not emitted in the body; got:\n{rendered}"
        );
    }

    #[test]
    fn straight_line_cfg_emits_flat_block() {
        let mut cfg = Cfg::new();
        cfg.add_block(BasicBlock::new(BlockId(0), 0, 1, 0..2, Terminator::Return));
        let mut blocks = std::collections::BTreeMap::new();
        blocks.insert(
            BlockId(0),
            block_with(vec![
                SsaStmt::assign(v("b0", 0), SsaExpr::lit(Literal::Int(1))),
                SsaStmt::assign(v("b0", 1), SsaExpr::lit(Literal::Int(2))),
            ]),
        );
        let ssa = SsaForm {
            cfg,
            dominance: DominanceInfo::new(),
            blocks,
            definitions: std::collections::BTreeMap::new(),
            uses: std::collections::BTreeMap::new(),
        };
        let structured = structure(&ssa);
        let assigns = structured
            .stmts
            .iter()
            .filter(|s| matches!(s, Stmt::Assign { .. }))
            .count();
        assert_eq!(assigns, 2, "two assignments should be emitted as-is");
        assert!(matches!(structured.stmts[0], Stmt::Assign { .. }));
    }

    /// A while loop: BB0 (header) branches to BB1 (body) / BB2 (exit); BB1
    /// jumps back to BB0. dominance(BB0 ≥ BB1) makes BB0 a loop header.
    #[test]
    fn structures_a_back_edge_into_a_while_loop() {
        let mut cfg = Cfg::new();
        cfg.add_block(BasicBlock::new(
            BlockId(0),
            0,
            1,
            0..1,
            Terminator::Branch {
                then_target: BlockId(1),
                else_target: BlockId(2),
            },
        ));
        cfg.add_block(BasicBlock::new(
            BlockId(1),
            1,
            2,
            1..2,
            Terminator::Jump { target: BlockId(0) },
        ));
        cfg.add_block(BasicBlock::new(BlockId(2), 2, 3, 2..3, Terminator::Return));
        cfg.add_edge(BlockId(0), BlockId(1), EdgeKind::ConditionalTrue);
        cfg.add_edge(BlockId(0), BlockId(2), EdgeKind::ConditionalFalse);
        cfg.add_edge(BlockId(1), BlockId(0), EdgeKind::Unconditional);
        let dominance = crate::decompiler::cfg::ssa::compute(&cfg);

        let mut blocks = std::collections::BTreeMap::new();
        // header condition def: b0_0 = (loc0 < 3)
        blocks.insert(
            BlockId(0),
            block_with(vec![SsaStmt::assign(
                v("t", 0),
                SsaExpr::binary(
                    BinOp::Lt,
                    SsaExpr::var(v("loc0", 0)),
                    SsaExpr::lit(Literal::Int(3)),
                ),
            )]),
        );
        // body: b1_0 = 1
        blocks.insert(
            BlockId(1),
            block_with(vec![SsaStmt::assign(
                v("t", 1),
                SsaExpr::lit(Literal::Int(1)),
            )]),
        );
        blocks.insert(BlockId(2), SsaBlock::new());

        let ssa = SsaForm {
            cfg,
            dominance,
            blocks,
            definitions: std::collections::BTreeMap::new(),
            uses: std::collections::BTreeMap::new(),
        };

        let structured = structure(&ssa);
        let has_while = structured.stmts.iter().any(
            |s| matches!(s, Stmt::ControlFlow(cf) if matches!(**cf, ControlFlow::While { .. })),
        );
        assert!(
            has_while,
            "a back-edge branch should structure as a While; got {:?}",
            structured.stmts
        );
    }

    #[test]
    fn infinite_loop_phi_copies_cover_both_arms_and_backedge() {
        const VIRTUAL_ENTRY: BlockId = BlockId(usize::MAX);

        let header = BlockId(0);
        let left_arm = BlockId(1);
        let right_arm = BlockId(2);
        let latch = BlockId(3);
        let mut cfg = Cfg::new();
        cfg.add_block(BasicBlock::new(
            header,
            0,
            1,
            0..1,
            Terminator::Branch {
                then_target: left_arm,
                else_target: right_arm,
            },
        ));
        for (id, offset) in [(left_arm, 1), (right_arm, 2)] {
            cfg.add_block(BasicBlock::new(
                id,
                offset,
                offset + 1,
                offset..offset + 1,
                Terminator::Jump { target: latch },
            ));
        }
        cfg.add_block(BasicBlock::new(
            latch,
            3,
            4,
            3..4,
            Terminator::Jump { target: header },
        ));
        cfg.add_edge(header, left_arm, EdgeKind::ConditionalTrue);
        cfg.add_edge(header, right_arm, EdgeKind::ConditionalFalse);
        cfg.add_edge(left_arm, latch, EdgeKind::Unconditional);
        cfg.add_edge(right_arm, latch, EdgeKind::Unconditional);
        cfg.add_edge(latch, header, EdgeKind::Unconditional);

        let state = v("state", 0);
        let condition = v("condition", 0);
        let left_entry = v("left_entry", 0);
        let left = v("left", 0);
        let right_entry = v("right_entry", 0);
        let right = v("right", 0);
        let merged = v("merged", 0);
        let next = v("next", 0);
        let mut header_block = block_with(vec![SsaStmt::assign(
            condition.clone(),
            SsaExpr::binary(
                BinOp::Lt,
                SsaExpr::var(state.clone()),
                SsaExpr::lit(Literal::Int(10)),
            ),
        )]);
        header_block.add_phi(phi(
            state.clone(),
            &[(VIRTUAL_ENTRY, v("initial", 0)), (latch, next.clone())],
        ));
        let mut left_block = block_with(vec![SsaStmt::assign(
            left.clone(),
            SsaExpr::binary(
                BinOp::Add,
                SsaExpr::var(left_entry.clone()),
                SsaExpr::lit(Literal::Int(1)),
            ),
        )]);
        left_block.add_phi(phi(left_entry.clone(), &[(header, state.clone())]));
        let mut right_block = block_with(vec![SsaStmt::assign(
            right.clone(),
            SsaExpr::binary(
                BinOp::Add,
                SsaExpr::var(right_entry.clone()),
                SsaExpr::lit(Literal::Int(2)),
            ),
        )]);
        right_block.add_phi(phi(right_entry.clone(), &[(header, state.clone())]));
        let mut latch_block = block_with(vec![SsaStmt::assign(
            next.clone(),
            SsaExpr::var(merged.clone()),
        )]);
        latch_block.add_phi(phi(
            merged.clone(),
            &[(left_arm, left.clone()), (right_arm, right.clone())],
        ));
        let ssa = SsaForm {
            dominance: crate::decompiler::cfg::ssa::compute(&cfg),
            cfg,
            blocks: BTreeMap::from([
                (header, header_block),
                (left_arm, left_block),
                (right_arm, right_block),
                (latch, latch_block),
            ]),
            definitions: BTreeMap::new(),
            uses: BTreeMap::from([
                (state, BTreeSet::from([UseSite::new(header, 0)])),
                (condition, BTreeSet::from([UseSite::terminator(header)])),
                (left_entry, BTreeSet::from([UseSite::new(left_arm, 0)])),
                (right_entry, BTreeSet::from([UseSite::new(right_arm, 0)])),
                (merged, BTreeSet::from([UseSite::new(latch, 0)])),
            ]),
        };

        let structured = structure(&ssa);
        let infinite = structured.stmts.iter().find_map(|stmt| match stmt {
            Stmt::ControlFlow(control_flow) => match control_flow.as_ref() {
                ControlFlow::While {
                    condition: Expr::Literal(Literal::Bool(true)),
                    body,
                } => Some(body),
                _ => None,
            },
            _ => None,
        });
        let body = infinite.expect("infinite loop");
        let branch = body.stmts.iter().find_map(|stmt| match stmt {
            Stmt::ControlFlow(control_flow) => match control_flow.as_ref() {
                ControlFlow::If {
                    then_branch,
                    else_branch: Some(else_branch),
                    ..
                } => Some((then_branch, else_branch)),
                _ => None,
            },
            _ => None,
        });
        let (then_branch, else_branch) = branch.expect("infinite loop branch");
        assert!(then_branch.stmts.contains(&Stmt::Assign {
            target: "left_entry_0".to_string(),
            value: Expr::var("state_0"),
        }));
        assert!(else_branch.stmts.contains(&Stmt::Assign {
            target: "right_entry_0".to_string(),
            value: Expr::var("state_0"),
        }));
        assert!(matches!(
            body.stmts.last(),
            Some(Stmt::Assign {
                target,
                value: Expr::Variable(source),
            }) if target == "state_0" && source == "next_0"
        ));
        assert!(!block_contains_call(&structured, "phi"));
    }

    #[test]
    fn while_phi_copies_run_in_preheader_and_latch() {
        let preheader = BlockId(0);
        let header = BlockId(1);
        let body = BlockId(2);
        let exit = BlockId(3);
        let mut cfg = Cfg::new();
        cfg.add_block(BasicBlock::new(
            preheader,
            0,
            1,
            0..1,
            Terminator::Jump { target: header },
        ));
        cfg.add_block(BasicBlock::new(
            header,
            1,
            2,
            1..2,
            Terminator::Branch {
                then_target: body,
                else_target: exit,
            },
        ));
        cfg.add_block(BasicBlock::new(
            body,
            2,
            3,
            2..3,
            Terminator::Jump { target: header },
        ));
        cfg.add_block(BasicBlock::new(exit, 3, 4, 3..4, Terminator::Return));
        cfg.add_edge(preheader, header, EdgeKind::Unconditional);
        cfg.add_edge(header, body, EdgeKind::ConditionalTrue);
        cfg.add_edge(header, exit, EdgeKind::ConditionalFalse);
        cfg.add_edge(body, header, EdgeKind::Unconditional);

        let seed = v("seed", 0);
        let state = v("state", 0);
        let condition = v("condition", 0);
        let body_value = v("body_value", 0);
        let next = v("next", 0);
        let exit_value = v("exit_value", 0);

        let preheader_block = block_with(vec![SsaStmt::assign(
            seed.clone(),
            SsaExpr::lit(Literal::Int(0)),
        )]);
        let mut header_block = block_with(vec![SsaStmt::assign(
            condition.clone(),
            SsaExpr::binary(
                BinOp::Lt,
                SsaExpr::var(state.clone()),
                SsaExpr::lit(Literal::Int(3)),
            ),
        )]);
        header_block.add_phi(phi(
            state.clone(),
            &[(preheader, seed.clone()), (body, next.clone())],
        ));
        let mut body_block = block_with(vec![SsaStmt::assign(
            next.clone(),
            SsaExpr::binary(
                BinOp::Add,
                SsaExpr::var(body_value.clone()),
                SsaExpr::lit(Literal::Int(1)),
            ),
        )]);
        body_block.add_phi(phi(body_value.clone(), &[(header, state.clone())]));
        let mut exit_block = block_with(vec![
            SsaStmt::expr(SsaExpr::call(
                "consume".to_string(),
                vec![SsaExpr::var(exit_value.clone())],
            )),
            SsaStmt::ret(None),
        ]);
        exit_block.add_phi(phi(exit_value.clone(), &[(header, state.clone())]));
        let ssa = SsaForm {
            dominance: crate::decompiler::cfg::ssa::compute(&cfg),
            cfg,
            blocks: BTreeMap::from([
                (preheader, preheader_block),
                (header, header_block),
                (body, body_block),
                (exit, exit_block),
            ]),
            definitions: BTreeMap::new(),
            uses: BTreeMap::from([
                (state, BTreeSet::from([UseSite::new(header, 0)])),
                (condition, BTreeSet::from([UseSite::terminator(header)])),
                (body_value, BTreeSet::from([UseSite::new(body, 0)])),
                (exit_value, BTreeSet::from([UseSite::new(exit, 0)])),
            ]),
        };

        let structured = structure(&ssa);
        let while_index = structured
            .stmts
            .iter()
            .position(|stmt| {
                matches!(stmt, Stmt::ControlFlow(control_flow) if matches!(control_flow.as_ref(), ControlFlow::While { .. }))
            })
            .expect("while loop");
        let Stmt::ControlFlow(control_flow) = &structured.stmts[while_index] else {
            unreachable!();
        };
        let ControlFlow::While { body, .. } = control_flow.as_ref() else {
            unreachable!();
        };

        assert!(structured.stmts[..while_index].contains(&Stmt::Assign {
            target: "state_0".to_string(),
            value: Expr::var("seed_0"),
        }));
        assert!(matches!(
            body.stmts.first(),
            Some(Stmt::Assign {
                target,
                value: Expr::Variable(source),
            }) if target == "body_value_0" && source == "state_0"
        ));
        assert!(matches!(
            body.stmts.last(),
            Some(Stmt::Assign {
                target,
                value: Expr::Variable(source),
            }) if target == "state_0" && source == "next_0"
        ));
        assert!(matches!(
            structured.stmts.get(while_index + 1),
            Some(Stmt::Assign {
                target,
                value: Expr::Variable(source),
            }) if target == "exit_value_0" && source == "state_0"
        ));
        assert!(!block_contains_call(&structured, "phi"));
    }

    /// A try/catch: TryEntry{body, catch, finally=None}; body and catch both
    /// reach an EndTry whose continuation is the post-try block.
    #[test]
    fn structures_a_try_entry_into_try_catch() {
        let mut cfg = Cfg::new();
        cfg.add_block(BasicBlock::new(
            BlockId(0),
            0,
            1,
            0..1,
            Terminator::TryEntry {
                body_target: BlockId(1),
                catch_target: Some(BlockId(2)),
                finally_target: None,
            },
        ));
        cfg.add_block(BasicBlock::new(
            BlockId(1),
            1,
            2,
            1..2,
            Terminator::Jump { target: BlockId(3) },
        ));
        cfg.add_block(BasicBlock::new(
            BlockId(2),
            2,
            3,
            2..3,
            Terminator::Jump { target: BlockId(3) },
        ));
        cfg.add_block(BasicBlock::new(
            BlockId(3),
            3,
            4,
            3..4,
            Terminator::EndTry {
                continuation: BlockId(4),
            },
        ));
        cfg.add_block(BasicBlock::new(BlockId(4), 4, 5, 4..5, Terminator::Return));
        cfg.add_edge(BlockId(0), BlockId(1), EdgeKind::Unconditional);
        cfg.add_edge(BlockId(0), BlockId(2), EdgeKind::Unconditional);
        cfg.add_edge(BlockId(1), BlockId(3), EdgeKind::Unconditional);
        cfg.add_edge(BlockId(2), BlockId(3), EdgeKind::Unconditional);
        let dominance = crate::decompiler::cfg::ssa::compute(&cfg);

        let mut blocks = std::collections::BTreeMap::new();
        blocks.insert(
            BlockId(1),
            block_with(vec![SsaStmt::assign(
                v("t", 0),
                SsaExpr::lit(Literal::Int(1)),
            )]),
        );
        blocks.insert(
            BlockId(2),
            block_with(vec![SsaStmt::assign(
                v("t", 1),
                SsaExpr::lit(Literal::Int(2)),
            )]),
        );
        blocks.insert(BlockId(0), SsaBlock::new());
        blocks.insert(BlockId(3), SsaBlock::new());
        blocks.insert(BlockId(4), SsaBlock::new());

        let ssa = SsaForm {
            cfg,
            dominance,
            blocks,
            definitions: std::collections::BTreeMap::new(),
            uses: std::collections::BTreeMap::new(),
        };

        let structured = structure(&ssa);
        let has_try = structured.stmts.iter().any(
            |s| matches!(s, Stmt::ControlFlow(cf) if matches!(**cf, ControlFlow::TryCatch { .. })),
        );
        assert!(
            has_try,
            "a TryEntry should structure as TryCatch; got {:?}",
            structured.stmts
        );
    }

    #[test]
    fn try_phi_copies_stay_in_their_selected_region() {
        let entry = BlockId(0);
        let body = BlockId(1);
        let catch = BlockId(2);
        let end_try = BlockId(3);
        let continuation = BlockId(4);
        let mut cfg = Cfg::new();
        cfg.add_block(BasicBlock::new(
            entry,
            0,
            1,
            0..1,
            Terminator::TryEntry {
                body_target: body,
                catch_target: Some(catch),
                finally_target: None,
            },
        ));
        cfg.add_block(BasicBlock::new(
            body,
            1,
            2,
            1..2,
            Terminator::Jump { target: end_try },
        ));
        cfg.add_block(BasicBlock::new(
            catch,
            2,
            3,
            2..3,
            Terminator::Jump { target: end_try },
        ));
        cfg.add_block(BasicBlock::new(
            end_try,
            3,
            4,
            3..4,
            Terminator::EndTry { continuation },
        ));
        cfg.add_block(BasicBlock::new(
            continuation,
            4,
            5,
            4..5,
            Terminator::Return,
        ));
        cfg.add_edge(entry, body, EdgeKind::Unconditional);
        cfg.add_edge(entry, catch, EdgeKind::Exception);
        cfg.add_edge(body, end_try, EdgeKind::Unconditional);
        cfg.add_edge(catch, end_try, EdgeKind::Unconditional);
        cfg.add_edge(end_try, continuation, EdgeKind::Unconditional);

        let body_value = v("body_value", 0);
        let catch_value = v("catch_value", 0);
        let mut body_block = block_with(vec![SsaStmt::expr(SsaExpr::call(
            "consume_body".to_string(),
            vec![SsaExpr::var(body_value.clone())],
        ))]);
        body_block.add_phi(phi(body_value.clone(), &[(entry, v("arg_body", 0))]));
        let mut catch_block = block_with(vec![SsaStmt::expr(SsaExpr::call(
            "consume_catch".to_string(),
            vec![SsaExpr::var(catch_value.clone())],
        ))]);
        catch_block.add_phi(phi(catch_value.clone(), &[(entry, v("arg_catch", 0))]));
        let ssa = SsaForm {
            dominance: crate::decompiler::cfg::ssa::compute(&cfg),
            cfg,
            blocks: BTreeMap::from([
                (entry, SsaBlock::new()),
                (body, body_block),
                (catch, catch_block),
                (end_try, SsaBlock::new()),
                (continuation, block_with(vec![SsaStmt::ret(None)])),
            ]),
            definitions: BTreeMap::new(),
            uses: BTreeMap::from([
                (body_value, BTreeSet::from([UseSite::new(body, 0)])),
                (catch_value, BTreeSet::from([UseSite::new(catch, 0)])),
            ]),
        };

        let structured = structure(&ssa);
        let try_catch = structured.stmts.iter().find_map(|stmt| match stmt {
            Stmt::ControlFlow(control_flow)
                if matches!(control_flow.as_ref(), ControlFlow::TryCatch { .. }) =>
            {
                Some(control_flow.as_ref())
            }
            _ => None,
        });
        let Some(ControlFlow::TryCatch {
            try_body,
            catch_body: Some(catch_body),
            ..
        }) = try_catch
        else {
            panic!("expected try/catch, got {:?}", structured.stmts);
        };
        let body_copy = Stmt::Assign {
            target: "body_value_0".to_string(),
            value: Expr::var("arg_body_0"),
        };
        let catch_copy = Stmt::Assign {
            target: "catch_value_0".to_string(),
            value: Expr::var("arg_catch_0"),
        };

        assert!(try_body.stmts.contains(&body_copy));
        assert!(!try_body.stmts.contains(&catch_copy));
        assert!(catch_body.stmts.contains(&catch_copy));
        assert!(!catch_body.stmts.contains(&body_copy));
        assert!(!block_contains_call(&structured, "phi"));
    }

    #[test]
    fn endtry_continuation_copy_is_shared_after_all_regions() {
        let entry = BlockId(0);
        let body = BlockId(1);
        let catch = BlockId(2);
        let finally = BlockId(3);
        let end_try = BlockId(4);
        let continuation = BlockId(5);
        let mut cfg = Cfg::new();
        cfg.add_block(BasicBlock::new(
            entry,
            0,
            1,
            0..1,
            Terminator::TryEntry {
                body_target: body,
                catch_target: Some(catch),
                finally_target: Some(finally),
            },
        ));
        for (id, offset) in [(body, 1), (catch, 2), (finally, 3)] {
            cfg.add_block(BasicBlock::new(
                id,
                offset,
                offset + 1,
                offset..offset + 1,
                Terminator::Jump { target: end_try },
            ));
        }
        cfg.add_block(BasicBlock::new(
            end_try,
            4,
            5,
            4..5,
            Terminator::EndTry { continuation },
        ));
        cfg.add_block(BasicBlock::new(
            continuation,
            5,
            6,
            5..6,
            Terminator::Return,
        ));
        cfg.add_edge(entry, body, EdgeKind::Unconditional);
        cfg.add_edge(entry, catch, EdgeKind::Exception);
        cfg.add_edge(entry, finally, EdgeKind::Finally);
        cfg.add_edge(body, end_try, EdgeKind::Unconditional);
        cfg.add_edge(catch, end_try, EdgeKind::Unconditional);
        cfg.add_edge(finally, end_try, EdgeKind::Unconditional);
        cfg.add_edge(end_try, continuation, EdgeKind::Unconditional);

        let shared = v("shared", 0);
        let continued = v("continued", 0);
        let finally_value = v("finally_value", 0);
        let mut finally_block = block_with(vec![SsaStmt::expr(SsaExpr::call(
            "finally".to_string(),
            vec![SsaExpr::var(finally_value.clone())],
        ))]);
        finally_block.add_phi(phi(finally_value.clone(), &[(entry, v("arg_finally", 0))]));
        let mut continuation_block = block_with(vec![
            SsaStmt::expr(SsaExpr::call(
                "consume".to_string(),
                vec![SsaExpr::var(continued.clone())],
            )),
            SsaStmt::ret(None),
        ]);
        continuation_block.add_phi(phi(continued.clone(), &[(end_try, shared.clone())]));
        let ssa = SsaForm {
            dominance: crate::decompiler::cfg::ssa::compute(&cfg),
            cfg,
            blocks: BTreeMap::from([
                (entry, SsaBlock::new()),
                (
                    body,
                    block_with(vec![SsaStmt::expr(SsaExpr::call(
                        "body".to_string(),
                        vec![],
                    ))]),
                ),
                (
                    catch,
                    block_with(vec![SsaStmt::expr(SsaExpr::call(
                        "catch".to_string(),
                        vec![],
                    ))]),
                ),
                (finally, finally_block),
                (
                    end_try,
                    block_with(vec![SsaStmt::assign(shared, SsaExpr::lit(Literal::Int(9)))]),
                ),
                (continuation, continuation_block),
            ]),
            definitions: BTreeMap::new(),
            uses: BTreeMap::from([
                (finally_value, BTreeSet::from([UseSite::new(finally, 0)])),
                (continued, BTreeSet::from([UseSite::new(continuation, 0)])),
            ]),
        };

        let structured = structure(&ssa);
        let try_index = structured
            .stmts
            .iter()
            .position(|stmt| {
                matches!(stmt, Stmt::ControlFlow(control_flow) if matches!(control_flow.as_ref(), ControlFlow::TryCatch { .. }))
            })
            .expect("try/catch/finally");
        let Stmt::ControlFlow(control_flow) = &structured.stmts[try_index] else {
            unreachable!();
        };
        let ControlFlow::TryCatch {
            try_body,
            catch_body: Some(catch_body),
            finally_body: Some(finally_body),
            ..
        } = control_flow.as_ref()
        else {
            unreachable!();
        };
        let shared_copy = Stmt::Assign {
            target: "continued_0".to_string(),
            value: Expr::var("shared_0"),
        };
        assert!(finally_body.stmts.contains(&Stmt::Assign {
            target: "finally_value_0".to_string(),
            value: Expr::var("arg_finally_0"),
        }));

        assert!(!try_body.stmts.contains(&shared_copy));
        assert!(!catch_body.stmts.contains(&shared_copy));
        assert!(!finally_body.stmts.contains(&shared_copy));
        assert!(matches!(
            structured.stmts.get(try_index + 1),
            Some(Stmt::Assign {
                target,
                value: Expr::Literal(Literal::Int(9)),
            }) if target == "shared_0"
        ));
        assert_eq!(structured.stmts.get(try_index + 2), Some(&shared_copy));
        assert_eq!(
            structured
                .stmts
                .iter()
                .filter(|stmt| *stmt == &shared_copy)
                .count(),
            1
        );
    }

    /// A do-while: BB0 (body entry, falls through to the latch) is the loop
    /// header; BB1 (latch) tests the condition and branches back to BB0 or out
    /// to BB2. BB0 dominates BB1, so BB0 is a loop header whose terminator is
    /// not a Branch → do-while.
    #[test]
    fn structures_a_bottom_tested_loop_into_do_while() {
        let mut cfg = Cfg::new();
        cfg.add_block(BasicBlock::new(
            BlockId(0),
            0,
            1,
            0..1,
            Terminator::Fallthrough { target: BlockId(1) },
        ));
        cfg.add_block(BasicBlock::new(
            BlockId(1),
            1,
            2,
            1..2,
            Terminator::Branch {
                then_target: BlockId(0),
                else_target: BlockId(2),
            },
        ));
        cfg.add_block(BasicBlock::new(BlockId(2), 2, 3, 2..3, Terminator::Return));
        cfg.add_edge(BlockId(0), BlockId(1), EdgeKind::Unconditional);
        cfg.add_edge(BlockId(1), BlockId(0), EdgeKind::ConditionalTrue);
        cfg.add_edge(BlockId(1), BlockId(2), EdgeKind::ConditionalFalse);
        let dominance = crate::decompiler::cfg::ssa::compute(&cfg);

        let mut blocks = std::collections::BTreeMap::new();
        // body: b0_0 = step()
        blocks.insert(
            BlockId(0),
            block_with(vec![SsaStmt::assign(
                v("t", 0),
                SsaExpr::call("step".to_string(), vec![]),
            )]),
        );
        // latch condition: b1_0 = (loc0 < 3)
        blocks.insert(
            BlockId(1),
            block_with(vec![SsaStmt::assign(
                v("t", 1),
                SsaExpr::binary(
                    BinOp::Lt,
                    SsaExpr::var(v("loc0", 0)),
                    SsaExpr::lit(Literal::Int(3)),
                ),
            )]),
        );
        blocks.insert(BlockId(2), SsaBlock::new());

        let ssa = SsaForm {
            cfg,
            dominance,
            blocks,
            definitions: std::collections::BTreeMap::new(),
            uses: std::collections::BTreeMap::new(),
        };

        let structured = structure(&ssa);
        let has_dowhile = structured.stmts.iter().any(
            |s| matches!(s, Stmt::ControlFlow(cf) if matches!(**cf, ControlFlow::DoWhile { .. })),
        );
        assert!(
            has_dowhile,
            "a bottom-tested loop should structure as DoWhile; got {:?}",
            structured.stmts
        );
    }

    #[test]
    fn do_while_phi_backedge_copy_stays_in_body() {
        const VIRTUAL_ENTRY: BlockId = BlockId(usize::MAX);

        let header = BlockId(0);
        let latch = BlockId(1);
        let exit = BlockId(2);
        let mut cfg = Cfg::new();
        cfg.add_block(BasicBlock::new(
            header,
            0,
            1,
            0..1,
            Terminator::Fallthrough { target: latch },
        ));
        cfg.add_block(BasicBlock::new(
            latch,
            1,
            2,
            1..2,
            Terminator::Branch {
                then_target: header,
                else_target: exit,
            },
        ));
        cfg.add_block(BasicBlock::new(exit, 2, 3, 2..3, Terminator::Return));
        cfg.add_edge(header, latch, EdgeKind::Unconditional);
        cfg.add_edge(latch, header, EdgeKind::ConditionalTrue);
        cfg.add_edge(latch, exit, EdgeKind::ConditionalFalse);

        let state = v("state", 0);
        let initial = v("initial", 0);
        let next = v("next", 0);
        let condition = v("condition", 0);
        let exit_value = v("exit_value", 0);
        let mut header_block = block_with(vec![SsaStmt::assign(
            next.clone(),
            SsaExpr::binary(
                BinOp::Add,
                SsaExpr::var(state.clone()),
                SsaExpr::lit(Literal::Int(1)),
            ),
        )]);
        header_block.add_phi(phi(
            state.clone(),
            &[(VIRTUAL_ENTRY, initial), (latch, next.clone())],
        ));
        let latch_block = block_with(vec![SsaStmt::assign(
            condition.clone(),
            SsaExpr::binary(
                BinOp::Lt,
                SsaExpr::var(next.clone()),
                SsaExpr::lit(Literal::Int(3)),
            ),
        )]);
        let mut exit_block = block_with(vec![
            SsaStmt::expr(SsaExpr::call(
                "consume".to_string(),
                vec![SsaExpr::var(exit_value.clone())],
            )),
            SsaStmt::ret(None),
        ]);
        exit_block.add_phi(phi(exit_value.clone(), &[(latch, state.clone())]));
        let ssa = SsaForm {
            dominance: crate::decompiler::cfg::ssa::compute(&cfg),
            cfg,
            blocks: BTreeMap::from([
                (header, header_block),
                (latch, latch_block),
                (exit, exit_block),
            ]),
            definitions: BTreeMap::new(),
            uses: BTreeMap::from([
                (state, BTreeSet::from([UseSite::new(header, 0)])),
                (condition, BTreeSet::from([UseSite::terminator(latch)])),
                (exit_value, BTreeSet::from([UseSite::new(exit, 0)])),
            ]),
        };

        let structured = structure(&ssa);
        let loop_index = structured
            .stmts
            .iter()
            .position(|stmt| {
                matches!(stmt, Stmt::ControlFlow(control_flow) if matches!(control_flow.as_ref(), ControlFlow::DoWhile { .. }))
            })
            .expect("do-while loop");
        let Stmt::ControlFlow(control_flow) = &structured.stmts[loop_index] else {
            unreachable!();
        };
        let ControlFlow::DoWhile { body, .. } = control_flow.as_ref() else {
            unreachable!();
        };

        assert!(matches!(
            structured.stmts.first(),
            Some(Stmt::Assign {
                target,
                value: Expr::Variable(source),
            }) if target == "state_0" && source == "initial_0"
        ));
        let guarded_backedge = body.stmts.iter().find_map(|stmt| match stmt {
            Stmt::ControlFlow(control_flow) => match control_flow.as_ref() {
                ControlFlow::If {
                    then_branch,
                    else_branch: None,
                    ..
                } if then_branch.stmts.contains(&Stmt::Assign {
                    target: "state_0".to_string(),
                    value: Expr::var("next_0"),
                }) =>
                {
                    Some(then_branch)
                }
                _ => None,
            },
            _ => None,
        });
        assert!(
            guarded_backedge.is_some(),
            "the backedge copy must be guarded inside the do-while body: {body:?}"
        );
        assert!(
            !body.stmts.iter().any(|stmt| matches!(
                stmt,
                Stmt::Assign {
                    target,
                    value: Expr::Variable(source),
                } if target == "state_0" && source == "next_0"
            )),
            "the false exit must not execute the backedge copy: {body:?}"
        );
        assert!(matches!(
            structured.stmts.get(loop_index + 1),
            Some(Stmt::Assign {
                target,
                value: Expr::Variable(source),
            }) if target == "exit_value_0" && source == "state_0"
        ));
        assert!(!block_contains_call(&structured, "phi"));
    }

    /// A switch: an equality cascade on one scrutinee. B0 compares `loc0 == 0`
    /// → case0 body, else B1; B1 compares `loc0 == 1` → case1 body, else B2
    /// (default); all bodies join at the merge B5.
    #[test]
    fn structures_an_equality_cascade_into_a_switch() {
        let mut cfg = Cfg::new();
        // B0: loc0 == 0 ? case0 : B1
        cfg.add_block(BasicBlock::new(
            BlockId(0),
            0,
            1,
            0..1,
            Terminator::Branch {
                then_target: BlockId(3),
                else_target: BlockId(1),
            },
        ));
        // B1: loc0 == 1 ? case1 : default(B2)
        cfg.add_block(BasicBlock::new(
            BlockId(1),
            1,
            2,
            1..2,
            Terminator::Branch {
                then_target: BlockId(4),
                else_target: BlockId(2),
            },
        ));
        // B2 (default) -> merge
        cfg.add_block(BasicBlock::new(
            BlockId(2),
            2,
            3,
            2..3,
            Terminator::Jump { target: BlockId(5) },
        ));
        // B3 (case0 body) -> merge
        cfg.add_block(BasicBlock::new(
            BlockId(3),
            3,
            4,
            3..4,
            Terminator::Jump { target: BlockId(5) },
        ));
        // B4 (case1 body) -> merge
        cfg.add_block(BasicBlock::new(
            BlockId(4),
            4,
            5,
            4..5,
            Terminator::Jump { target: BlockId(5) },
        ));
        // B5 (merge)
        cfg.add_block(BasicBlock::new(BlockId(5), 5, 6, 5..6, Terminator::Return));
        cfg.add_edge(BlockId(0), BlockId(3), EdgeKind::ConditionalTrue);
        cfg.add_edge(BlockId(0), BlockId(1), EdgeKind::ConditionalFalse);
        cfg.add_edge(BlockId(1), BlockId(4), EdgeKind::ConditionalTrue);
        cfg.add_edge(BlockId(1), BlockId(2), EdgeKind::ConditionalFalse);
        cfg.add_edge(BlockId(2), BlockId(5), EdgeKind::Unconditional);
        cfg.add_edge(BlockId(3), BlockId(5), EdgeKind::Unconditional);
        cfg.add_edge(BlockId(4), BlockId(5), EdgeKind::Unconditional);
        let dominance = crate::decompiler::cfg::ssa::compute(&cfg);

        let mut blocks = std::collections::BTreeMap::new();
        // B0: t0 = (loc0 == 0)
        blocks.insert(
            BlockId(0),
            block_with(vec![SsaStmt::assign(
                v("t", 0),
                SsaExpr::binary(
                    BinOp::Eq,
                    SsaExpr::var(v("loc0", 0)),
                    SsaExpr::lit(Literal::Int(0)),
                ),
            )]),
        );
        // B1: t1 = (loc0 == 1)
        blocks.insert(
            BlockId(1),
            block_with(vec![SsaStmt::assign(
                v("t", 1),
                SsaExpr::binary(
                    BinOp::Eq,
                    SsaExpr::var(v("loc0", 1)),
                    SsaExpr::lit(Literal::Int(1)),
                ),
            )]),
        );
        let default_value = v("default_value", 0);
        let case0_value = v("case0_value", 0);
        let case1_value = v("case1_value", 0);
        let mut default_block = block_with(vec![SsaStmt::expr(SsaExpr::call(
            "consume_default".to_string(),
            vec![SsaExpr::var(default_value.clone())],
        ))]);
        default_block.add_phi(phi(
            default_value.clone(),
            &[(BlockId(1), v("arg_default", 0))],
        ));
        blocks.insert(BlockId(2), default_block);
        let mut case0_block = block_with(vec![SsaStmt::expr(SsaExpr::call(
            "consume_case0".to_string(),
            vec![SsaExpr::var(case0_value.clone())],
        ))]);
        case0_block.add_phi(phi(case0_value.clone(), &[(BlockId(0), v("arg_case0", 0))]));
        blocks.insert(BlockId(3), case0_block);
        let mut case1_block = block_with(vec![SsaStmt::expr(SsaExpr::call(
            "consume_case1".to_string(),
            vec![SsaExpr::var(case1_value.clone())],
        ))]);
        case1_block.add_phi(phi(case1_value.clone(), &[(BlockId(1), v("arg_case1", 0))]));
        blocks.insert(BlockId(4), case1_block);
        blocks.insert(BlockId(5), SsaBlock::new());

        let ssa = SsaForm {
            cfg,
            dominance,
            blocks,
            definitions: std::collections::BTreeMap::new(),
            uses: BTreeMap::from([
                (default_value, BTreeSet::from([UseSite::new(BlockId(2), 0)])),
                (case0_value, BTreeSet::from([UseSite::new(BlockId(3), 0)])),
                (case1_value, BTreeSet::from([UseSite::new(BlockId(4), 0)])),
            ]),
        };

        let structured = structure(&ssa);
        let switch = structured.stmts.iter().find_map(|stmt| match stmt {
            Stmt::ControlFlow(control_flow)
                if matches!(control_flow.as_ref(), ControlFlow::Switch { .. }) =>
            {
                Some(control_flow.as_ref())
            }
            _ => None,
        });
        let Some(ControlFlow::Switch { cases, default, .. }) = switch else {
            panic!(
                "an equality cascade on one scrutinee should structure as a Switch; got {:?}",
                structured.stmts
            );
        };
        assert!(cases[0].1.stmts.contains(&Stmt::Assign {
            target: "case0_value_0".to_string(),
            value: Expr::var("arg_case0_0"),
        }));
        assert!(cases[1].1.stmts.contains(&Stmt::Assign {
            target: "case1_value_0".to_string(),
            value: Expr::var("arg_case1_0"),
        }));
        assert!(default
            .as_ref()
            .expect("switch default")
            .stmts
            .contains(&Stmt::Assign {
                target: "default_value_0".to_string(),
                value: Expr::var("arg_default_0"),
            }));
    }
}
