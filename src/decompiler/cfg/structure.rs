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

use std::collections::{BTreeSet, HashSet};

use crate::decompiler::cfg::{BlockId, Cfg, Terminator};
use crate::decompiler::ir::{Block as IrBlock, ControlFlow, Expr, Stmt};

use super::ssa::ssa_expr_to_ir;
use super::ssa::{DominanceInfo, SsaExpr, SsaForm, SsaStmt, SsaVariable};

/// Structure a whole [`SsaForm`] into a single [`IrBlock`] starting from its
/// entry block.
#[must_use]
pub fn structure(ssa: &SsaForm) -> IrBlock {
    let loop_headers = compute_loop_headers(&ssa.cfg, &ssa.dominance);
    let ctx = StructCtx {
        cfg: &ssa.cfg,
        ssa,
        loop_headers,
    };
    let entry = ssa.cfg.blocks().next().map(|b| b.id);
    let mut visited = HashSet::new();
    match entry {
        Some(e) => ctx.structure_region(e, None, &mut visited, true),
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

struct StructCtx<'a> {
    cfg: &'a Cfg,
    ssa: &'a SsaForm,
    loop_headers: HashSet<BlockId>,
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
                    let body = self.structure_region(bid, Some(latch), visited, false);
                    let cond = self.condition_for_block(latch);
                    out.push(Stmt::ControlFlow(Box::new(ControlFlow::do_while(
                        body, cond,
                    ))));
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

            self.emit_body(&mut out, bid);

            cur = match self.terminator(bid) {
                Terminator::Return
                | Terminator::Throw
                | Terminator::Abort
                | Terminator::Unknown => {
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
                        let cond = self.condition_for_block(bid);
                        let body = self.structure_region(then_target, Some(bid), visited, true);
                        out.push(Stmt::ControlFlow(Box::new(ControlFlow::while_loop(
                            cond, body,
                        ))));
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
        let Some(block) = self.ssa.block(bid) else {
            return;
        };
        for stmt in &block.stmts {
            if let SsaStmt::Assign { target, value } = stmt {
                out.push(Stmt::Assign {
                    target: var_name(target),
                    value: ssa_expr_to_ir(value),
                });
            }
        }
    }

    /// Recover an `if` / `if-else` from a `Branch` terminator: find the merge
    /// (closest common post-dominator by reachability intersection + predecessor
    /// count), structure each side up to it, and continue at the merge.
    fn handle_branch(
        &self,
        out: &mut IrBlock,
        bid: BlockId,
        then_target: BlockId,
        else_target: BlockId,
        outer_boundary: Option<BlockId>,
        visited: &mut HashSet<BlockId>,
    ) -> Option<BlockId> {
        let cond = self.condition_for_block(bid);

        if then_target == else_target {
            // Degenerate branch (condition has no effect): drop it and continue.
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
        let then_block = self.structure_region(then_target, merge, visited, true);
        let else_block = self.structure_region(else_target, merge, visited, true);

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

    /// The branch condition: the last assignment in the block is the value the
    /// conditional jump consumes (e.g. `t = (loc0 < 3); JMPIFNOT`). Use its
    /// target name as a variable reference; the def stays emitted in the body.
    fn condition_for_block(&self, bid: BlockId) -> Expr {
        if let Some(block) = self.ssa.block(bid) {
            for stmt in block.stmts.iter().rev() {
                if let SsaStmt::Assign { target, .. } = stmt {
                    return Expr::Variable(var_name(target));
                }
            }
        }
        Expr::Variable(format!("cond_{}", bid.0))
    }

    /// The last assignment's value expression in `bid`, if any (the raw SSA
    /// expression driving the branch, e.g. `(loc0 == 3)`).
    fn condition_expression(&self, bid: BlockId) -> Option<SsaExpr> {
        let block = self.ssa.block(bid)?;
        for stmt in block.stmts.iter().rev() {
            if let SsaStmt::Assign { value, .. } = stmt {
                return Some(value.clone());
            }
        }
        None
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
        let mut cases: Vec<(Expr, BlockId)> = vec![(ssa_expr_to_ir(&first_val), then_target)];
        let mut cur = else_target;
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
                            cases.push((ssa_expr_to_ir(&val), then_target));
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
        let merge = self.find_merge(cases[0].1, default_entry);

        let mut case_blocks: Vec<(Expr, IrBlock)> = Vec::with_capacity(cases.len());
        for (val, body_entry) in &cases {
            let body = self.structure_region(*body_entry, merge, visited, true);
            case_blocks.push((val.clone(), body));
        }
        let default_body = self.structure_region(default_entry, merge, visited, true);
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
        // The post-try continuation is the EndTry reachable from the try region.
        let continuation = self.find_endtry_continuation(body_target);

        // Handlers are boundaries for the body (and vice-versa) so each region
        // is structured in isolation.
        let mut body_stop: HashSet<BlockId> = HashSet::new();
        if let Some(c) = catch_target {
            body_stop.insert(c);
        }
        if let Some(f) = finally_target {
            body_stop.insert(f);
        }
        if let Some(c) = continuation {
            body_stop.insert(c);
        }
        let try_body = self.structure_set(body_target, &body_stop, visited);

        let catch_body = catch_target.map(|c| {
            let mut stop = HashSet::new();
            if let Some(f) = finally_target {
                stop.insert(f);
            }
            if let Some(cont) = continuation {
                stop.insert(cont);
            }
            self.structure_set(c, &stop, visited)
        });
        let finally_body = finally_target.map(|f| {
            let mut stop = HashSet::new();
            if let Some(cont) = continuation {
                stop.insert(cont);
            }
            self.structure_set(f, &stop, visited)
        });

        out.push(Stmt::ControlFlow(Box::new(ControlFlow::try_catch(
            try_body,
            None,
            catch_body,
            finally_body,
        ))));
        let _ = bid;
        continuation
    }

    /// Find the `EndTry` continuation reachable from `start`: the post-try merge
    /// block. Returns `None` when no `EndTry` is reachable.
    fn find_endtry_continuation(&self, start: BlockId) -> Option<BlockId> {
        let mut seen = BTreeSet::new();
        let mut stack = vec![start];
        while let Some(b) = stack.pop() {
            if !seen.insert(b) {
                continue;
            }
            if let Some(block) = self.cfg.block(b) {
                if let Terminator::EndTry { continuation } = &block.terminator {
                    return Some(*continuation);
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
            self.emit_body(&mut out, bid);
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
                        let cond = self.condition_for_block(bid);
                        let body = self.structure_region(then_target, Some(bid), visited, true);
                        out.push(Stmt::ControlFlow(Box::new(ControlFlow::while_loop(
                            cond, body,
                        ))));
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
    use crate::decompiler::cfg::ssa::{SsaBlock, SsaExpr, SsaStmt, SsaVariable};
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
        blocks.insert(
            BlockId(2),
            block_with(vec![SsaStmt::assign(
                v("loc0", 4),
                SsaExpr::lit(Literal::Int(12)),
            )]),
        );
        blocks.insert(
            BlockId(3),
            block_with(vec![SsaStmt::assign(
                v("loc0", 2),
                SsaExpr::lit(Literal::Int(10)),
            )]),
        );
        blocks.insert(
            BlockId(4),
            block_with(vec![SsaStmt::assign(
                v("loc0", 3),
                SsaExpr::lit(Literal::Int(11)),
            )]),
        );
        blocks.insert(BlockId(5), SsaBlock::new());

        let ssa = SsaForm {
            cfg,
            dominance,
            blocks,
            definitions: std::collections::BTreeMap::new(),
            uses: std::collections::BTreeMap::new(),
        };

        let structured = structure(&ssa);
        let has_switch = structured.stmts.iter().any(
            |s| matches!(s, Stmt::ControlFlow(cf) if matches!(**cf, ControlFlow::Switch { .. })),
        );
        assert!(
            has_switch,
            "an equality cascade on one scrutinee should structure as a Switch; got {:?}",
            structured.stmts
        );
    }
}
