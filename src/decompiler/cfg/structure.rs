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
use super::ssa::{DominanceInfo, SsaForm, SsaStmt, SsaVariable};

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
        Some(e) => ctx.structure_region(e, None, &mut visited),
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
    ) -> IrBlock {
        let mut out = IrBlock::new();
        let mut cur = Some(entry);

        while let Some(bid) = cur {
            if Some(bid) == boundary {
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
                        let body = self.structure_region(then_target, Some(bid), visited);
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
                Terminator::TryEntry { .. } | Terminator::EndTry { .. } => {
                    // Try/catch/finally recovery is a follow-up: emit a marker
                    // and stop this region to keep the output well-formed.
                    out.push(Stmt::Comment(format!("try/catch region at {:?}", bid)));
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

        let merge = self.find_merge(then_target, else_target);

        // The then/else sub-regions must stop at the merge so neither side
        // duplicates the post-merge code.
        let then_block = self.structure_region(then_target, merge, visited);
        let else_block = self.structure_region(else_target, merge, visited);

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

    fn terminator(&self, bid: BlockId) -> Terminator {
        self.cfg
            .block(bid)
            .map(|b| b.terminator.clone())
            .unwrap_or(Terminator::Unknown)
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
}
