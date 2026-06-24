//! SSA optimization passes.
//!
//! Operates on a built [`SsaForm`] (see [`crate::decompiler::cfg::ssa::SsaBuilder`])
//! and returns a simplified, semantically-equivalent form. The passes are the
//! classic SSA optimizations from the project roadmap:
//!
//! - **Constant folding / propagation**: `v2 = (1 + 2)` folds to `v2 = 3`, and
//!   that constant is then substituted into every use of `v2`.
//! - **Copy propagation**: `v1 = v0` substitutes `v0` for `v1` at every use.
//! - **Trivial-φ elimination**: a φ all of whose operands are the same value
//!   (or a single operand) is replaced by that value everywhere.
//! - **Dead-code elimination**: pure defs with no remaining uses are removed.
//!
//! The passes share a single substitution table and run to a fixed point.

use std::collections::{BTreeMap, BTreeSet};

use crate::decompiler::cfg::BlockId;
use crate::decompiler::ir::{BinOp, Literal, UnaryOp};

use super::form::{SsaExpr, SsaForm, SsaStmt};
use super::variable::SsaVariable;

/// Optimize `ssa` in place by running the SSA optimization passes to a fixed
/// point. Returns how many rewrite rounds were applied (0 = already optimal).
pub fn optimize(ssa: &mut SsaForm) -> usize {
    let mut rounds = 0usize;
    loop {
        let rewrites = one_round(ssa);
        if rewrites == 0 {
            break;
        }
        rounds += 1;
        // Guard against pathological non-convergence (monotone rewrites always
        // terminate, but cap defensively).
        if rounds > ssa.blocks.len() + 4 {
            break;
        }
    }
    rounds
}

/// A single substitution table mapping a variable to the simpler expression
/// that replaces every reference to it.
type Subst = BTreeMap<SsaVariable, SsaExpr>;

fn one_round(ssa: &mut SsaForm) -> usize {
    // Gather constants/copies from current assignments.
    let mut subst: Subst = BTreeMap::new();

    for (_bid, block) in ssa.blocks.iter() {
        for stmt in &block.stmts {
            if let SsaStmt::Assign { target, value } = stmt {
                match value {
                    // Direct constant: propagate the literal.
                    SsaExpr::Literal(_) => {
                        subst.insert(target.clone(), value.clone());
                    }
                    // Copy of another var: `t = v` -> use v.
                    SsaExpr::Variable(_) => {
                        subst.insert(target.clone(), value.clone());
                    }
                    // Foldable pure binary on two constants.
                    SsaExpr::Binary { op, left, right } => {
                        if let (Some(a), Some(b)) = (as_literal(left), as_literal(right)) {
                            if let Some(folded) = fold_binary(*op, &a, &b) {
                                subst.insert(target.clone(), SsaExpr::lit(folded));
                            }
                        }
                    }
                    // Foldable pure unary on a constant.
                    SsaExpr::Unary { op, operand } => {
                        if let Some(a) = as_literal(operand) {
                            if let Some(folded) = fold_unary(*op, &a) {
                                subst.insert(target.clone(), SsaExpr::lit(folded));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // Resolve transitive substitutions (v2->3, v1->v2 -> v1->3) and chase copy
    // chains (v1->v0, v0->3 -> v1->3).
    for (_bid, block) in ssa.blocks.iter() {
        for stmt in &block.stmts {
            if let SsaStmt::Assign { target, value } = stmt {
                if !subst.contains_key(target) {
                    continue;
                }
                // Only chase if the current mapping is a var/lit we can improve.
                if let SsaExpr::Variable(src) = value {
                    if let Some(resolved) = subst.get(src) {
                        subst.insert(target.clone(), resolved.clone());
                    }
                }
            }
        }
    }

    // Trivial-φ elimination: φ whose operands are all the same value (after
    // substitution) collapses to that value.
    for (_bid, block) in ssa.blocks.iter() {
        for phi in &block.phi_nodes {
            let operands: Vec<&SsaVariable> = phi.operands.values().collect();
            if operands.is_empty() {
                continue;
            }
            let first = operands[0];
            let mut resolved_first = subst
                .get(first)
                .cloned()
                .unwrap_or_else(|| SsaExpr::var(first.clone()));
            resolved_first = resolve_once(&subst, &resolved_first);
            let all_same = operands.iter().all(|v| {
                let r = subst
                    .get(*v)
                    .cloned()
                    .unwrap_or_else(|| SsaExpr::var((*v).clone()));
                resolve_once(&subst, &r) == resolved_first
            });
            if all_same {
                subst.insert(phi.target.clone(), resolved_first);
            }
        }
    }

    if subst.is_empty() {
        return 0;
    }

    // Apply substitutions to every expression reference and drop defs that have
    // been folded away (an assignment whose target is now a pure literal/copy
    // and is unused is dead).
    let used = collect_used(ssa, &subst);

    let mut rewrites = 0usize;
    for (_bid, block) in ssa.blocks.iter_mut() {
        // Rewrite φ operands.
        for phi in &mut block.phi_nodes {
            for (_pred, var) in phi.operands.iter_mut() {
                if let Some(SsaExpr::Variable(rep_var)) = subst.get(var) {
                    // φ operands are variables; only substitute when the
                    // replacement is itself a variable (constant φ results are
                    // reflected through the target substitution instead).
                    *var = rep_var.clone();
                    rewrites += 1;
                }
            }
        }
        // Rewrite statement RHS expressions and drop dead defs.
        block.stmts.retain(|stmt| match stmt {
            SsaStmt::Assign { target, .. } => {
                // Keep if the target has uses, or if it isn't a pure
                // constant/copy (i.e. we didn't substitute it).
                !subst.contains_key(target) || used.contains(target)
            }
            _ => true,
        });
        for stmt in &mut block.stmts {
            if let SsaStmt::Assign { target, value } = stmt {
                let new_value = rewrite_expr(value, &subst);
                if new_value != *value {
                    *value = new_value;
                    rewrites += 1;
                }
                // Avoid leaving a redundant `vN = vN` after substitution.
                if let SsaExpr::Variable(src) = value {
                    if src == target {
                        // self-assignment: will be DCE'd next round if unused.
                    }
                }
            }
        }
    }

    // Rebuild the definitions/uses indexes so downstream consumers stay correct.
    rebuild_indexes(ssa);
    rewrites.max(1)
}

/// Resolve a single level of substitution through a variable reference.
fn resolve_once(subst: &Subst, expr: &SsaExpr) -> SsaExpr {
    if let SsaExpr::Variable(v) = expr {
        if let Some(rep) = subst.get(v) {
            return rep.clone();
        }
    }
    expr.clone()
}

/// Recursively apply `subst` to every variable reference inside `expr`.
fn rewrite_expr(expr: &SsaExpr, subst: &Subst) -> SsaExpr {
    fn go(e: &SsaExpr, subst: &Subst) -> SsaExpr {
        match e {
            SsaExpr::Variable(v) => subst
                .get(v)
                .cloned()
                .unwrap_or_else(|| SsaExpr::var(v.clone())),
            SsaExpr::Binary { op, left, right } => {
                SsaExpr::binary(*op, go(left, subst), go(right, subst))
            }
            SsaExpr::Unary { op, operand } => SsaExpr::unary(*op, go(operand, subst)),
            SsaExpr::Call { name, args } => {
                SsaExpr::call(name.clone(), args.iter().map(|a| go(a, subst)).collect())
            }
            SsaExpr::Index { base, index } => SsaExpr::Index {
                base: Box::new(go(base, subst)),
                index: Box::new(go(index, subst)),
            },
            SsaExpr::Member { base, name } => SsaExpr::Member {
                base: Box::new(go(base, subst)),
                name: name.clone(),
            },
            SsaExpr::Cast { expr, target_type } => SsaExpr::Cast {
                expr: Box::new(go(expr, subst)),
                target_type: target_type.clone(),
            },
            SsaExpr::Array(els) => SsaExpr::Array(els.iter().map(|e| go(e, subst)).collect()),
            SsaExpr::Map(pairs) => SsaExpr::Map(
                pairs
                    .iter()
                    .map(|(k, v)| (go(k, subst), go(v, subst)))
                    .collect(),
            ),
            SsaExpr::Ternary {
                condition,
                then_expr,
                else_expr,
            } => SsaExpr::Ternary {
                condition: Box::new(go(condition, subst)),
                then_expr: Box::new(go(then_expr, subst)),
                else_expr: Box::new(go(else_expr, subst)),
            },
            other => other.clone(),
        }
    }
    go(expr, subst)
}

/// Extract a [`Literal`] from an expression that is (or substitutes to) one.
fn as_literal(expr: &SsaExpr) -> Option<Literal> {
    if let SsaExpr::Literal(l) = expr {
        Some(l.clone())
    } else {
        None
    }
}

/// Constant-fold a binary operation on two integer literals when it is safe
/// (no division by zero). Boolean/comparison ops fold to `Bool`.
fn fold_binary(op: BinOp, a: &Literal, b: &Literal) -> Option<Literal> {
    let (Literal::Int(x), Literal::Int(y)) = (a, b) else {
        return None;
    };
    let (x, y) = (*x, *y);
    Some(match op {
        BinOp::Add => Literal::Int(x.wrapping_add(y)),
        BinOp::Sub => Literal::Int(x.wrapping_sub(y)),
        BinOp::Mul => Literal::Int(x.wrapping_mul(y)),
        BinOp::Div => {
            if y == 0 {
                return None;
            }
            Literal::Int(x.wrapping_div(y))
        }
        BinOp::Mod => {
            if y == 0 {
                return None;
            }
            Literal::Int(x.wrapping_rem(y))
        }
        BinOp::Pow => (0..y)
            .take(1024)
            .try_fold(1i64, |acc, _| acc.checked_mul(x))
            .map(Literal::Int)?,
        BinOp::And => Literal::Int(x & y),
        BinOp::Or => Literal::Int(x | y),
        BinOp::Xor => Literal::Int(x ^ y),
        BinOp::Shl => Literal::Int(x.wrapping_shl((y & 63) as u32)),
        BinOp::Shr => Literal::Int(x.wrapping_shr((y & 63) as u32)),
        BinOp::Eq => Literal::Bool(x == y),
        BinOp::Ne => Literal::Bool(x != y),
        BinOp::Lt => Literal::Bool(x < y),
        BinOp::Le => Literal::Bool(x <= y),
        BinOp::Gt => Literal::Bool(x > y),
        BinOp::Ge => Literal::Bool(x >= y),
        BinOp::LogicalAnd => Literal::Bool(x != 0 && y != 0),
        BinOp::LogicalOr => Literal::Bool(x != 0 || y != 0),
    })
}

/// Constant-fold a unary operation on an integer literal.
fn fold_unary(op: UnaryOp, a: &Literal) -> Option<Literal> {
    let Literal::Int(x) = a else {
        return None;
    };
    Some(match op {
        UnaryOp::Neg => Literal::Int(x.wrapping_neg()),
        UnaryOp::Not => Literal::Int(!x),
        UnaryOp::Abs => Literal::Int(x.wrapping_abs()),
        UnaryOp::Inc => Literal::Int(x.wrapping_add(1)),
        UnaryOp::Dec => Literal::Int(x.wrapping_sub(1)),
        UnaryOp::Sign => Literal::Int(x.signum()),
        UnaryOp::LogicalNot => Literal::Bool(*x == 0),
    })
}

/// Variables that are still referenced (in φ operands or expression uses)
/// after substitution is applied. A substituted def with no remaining uses is
/// dead and can be dropped.
fn collect_used(ssa: &SsaForm, _subst: &Subst) -> BTreeSet<SsaVariable> {
    let mut used = BTreeSet::new();
    for (_bid, block) in ssa.blocks.iter() {
        for phi in &block.phi_nodes {
            for v in phi.operands.values() {
                used.insert(v.clone());
            }
        }
        for stmt in &block.stmts {
            if let SsaStmt::Assign { value, .. } = stmt {
                for v in collect_expr_vars(value) {
                    used.insert(v);
                }
            }
        }
    }
    used
}

fn collect_expr_vars(expr: &SsaExpr) -> Vec<SsaVariable> {
    let mut out = Vec::new();
    fn go(e: &SsaExpr, out: &mut Vec<SsaVariable>) {
        match e {
            SsaExpr::Variable(v) => out.push(v.clone()),
            SsaExpr::Binary { left, right, .. } => {
                go(left, out);
                go(right, out);
            }
            SsaExpr::Unary { operand, .. } => go(operand, out),
            SsaExpr::Call { args, .. } => args.iter().for_each(|a| go(a, out)),
            SsaExpr::Index { base, index } => {
                go(base, out);
                go(index, out);
            }
            SsaExpr::Member { base, .. } => go(base, out),
            SsaExpr::Cast { expr, .. } => go(expr, out),
            SsaExpr::Array(els) => els.iter().for_each(|e| go(e, out)),
            SsaExpr::Map(pairs) => pairs.iter().for_each(|(k, v)| {
                go(k, out);
                go(v, out);
            }),
            SsaExpr::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                go(condition, out);
                go(then_expr, out);
                go(else_expr, out);
            }
            SsaExpr::Literal(_) => {}
        }
    }
    go(expr, &mut out);
    out
}

/// Recompute the `definitions` and `uses` indexes from the current blocks.
fn rebuild_indexes(ssa: &mut SsaForm) {
    use super::form::UseSite;
    let mut definitions: BTreeMap<SsaVariable, BlockId> = BTreeMap::new();
    let mut uses: BTreeMap<SsaVariable, BTreeSet<UseSite>> = BTreeMap::new();
    for (bid, block) in ssa.blocks.iter() {
        for phi in &block.phi_nodes {
            definitions.insert(phi.target.clone(), *bid);
            for v in phi.operands.values() {
                uses.entry(v.clone())
                    .or_default()
                    .insert(UseSite::new(*bid, 0));
            }
        }
        for (i, stmt) in block.stmts.iter().enumerate() {
            if let SsaStmt::Assign { target, value } = stmt {
                definitions.insert(target.clone(), *bid);
                for v in collect_expr_vars(value) {
                    uses.entry(v).or_default().insert(UseSite::new(*bid, i));
                }
            }
        }
    }
    ssa.definitions = definitions;
    ssa.uses = uses;
}

// Rebuild a fresh SsaForm (used only by tests to assemble hand-built forms).
#[cfg(test)]
fn rebuild_test_form(
    cfg: crate::decompiler::cfg::Cfg,
    dominance: super::dominance::DominanceInfo,
    blocks: BTreeMap<BlockId, super::form::SsaBlock>,
) -> SsaForm {
    let mut ssa = SsaForm::new(cfg, dominance);
    for (id, block) in blocks {
        ssa.add_block(id, block);
    }
    let mut tmp = SsaForm {
        cfg: ssa.cfg.clone(),
        dominance: ssa.dominance.clone(),
        blocks: ssa.blocks.clone(),
        definitions: BTreeMap::new(),
        uses: BTreeMap::new(),
    };
    rebuild_indexes(&mut tmp);
    tmp
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decompiler::cfg::ssa::{DominanceInfo, SsaBlock, SsaExpr, SsaStmt, SsaVariable};
    use crate::decompiler::cfg::{BlockId, Cfg};
    use crate::decompiler::ir::{BinOp, Literal};

    fn v(base: &str, ver: usize) -> SsaVariable {
        SsaVariable::new(base.to_string(), ver)
    }

    fn assign_str(target: SsaVariable, value: SsaExpr) -> SsaStmt {
        SsaStmt::assign(target, value)
    }

    /// `(1 + 2)` then use of the result must fold to `3`.
    #[test]
    fn folds_constant_binary_and_propagates() {
        let mut block = SsaBlock::new();
        block.add_stmt(assign_str(v("b0", 0), SsaExpr::lit(Literal::Int(1))));
        block.add_stmt(assign_str(v("b0", 1), SsaExpr::lit(Literal::Int(2))));
        block.add_stmt(assign_str(
            v("b0", 2),
            SsaExpr::binary(
                BinOp::Add,
                SsaExpr::var(v("b0", 0)),
                SsaExpr::var(v("b0", 1)),
            ),
        ));
        block.add_stmt(assign_str(
            v("b0", 3),
            SsaExpr::call("use".to_string(), vec![SsaExpr::var(v("b0", 2))]),
        ));

        let mut blocks = BTreeMap::new();
        blocks.insert(BlockId(0), block);
        let mut ssa = rebuild_test_form(Cfg::new(), DominanceInfo::new(), blocks);

        let rounds = optimize(&mut ssa);
        assert!(rounds >= 1, "expected at least one optimization round");

        // The use site must now reference the folded literal 3 directly.
        let b0 = ssa.block(BlockId(0)).unwrap();
        let use_stmt = b0.stmts.last().unwrap();
        let SsaStmt::Assign { value, .. } = use_stmt else {
            panic!();
        };
        let SsaExpr::Call { args, .. } = value else {
            panic!("expected the use call, got {value:?}");
        };
        assert!(
            matches!(args[0], SsaExpr::Literal(Literal::Int(3))),
            "constant (1+2) should propagate as 3 into the use, got {:?}",
            args[0]
        );
    }

    /// Copy chain `v1 = v0; v0 = 7` resolves so `v1`'s users see `7`.
    #[test]
    fn propagates_copy_chains() {
        let mut block = SsaBlock::new();
        block.add_stmt(assign_str(v("b0", 0), SsaExpr::lit(Literal::Int(7))));
        block.add_stmt(assign_str(v("b0", 1), SsaExpr::var(v("b0", 0))));
        block.add_stmt(assign_str(
            v("b0", 2),
            SsaExpr::call("use".to_string(), vec![SsaExpr::var(v("b0", 1))]),
        ));

        let mut blocks = BTreeMap::new();
        blocks.insert(BlockId(0), block);
        let mut ssa = rebuild_test_form(Cfg::new(), DominanceInfo::new(), blocks);
        optimize(&mut ssa);

        let b0 = ssa.block(BlockId(0)).unwrap();
        let SsaStmt::Assign { value, .. } = b0.stmts.last().unwrap() else {
            panic!();
        };
        let SsaExpr::Call { args, .. } = value else {
            panic!();
        };
        assert!(
            matches!(args[0], SsaExpr::Literal(Literal::Int(7))),
            "copy chain should resolve to 7, got {:?}",
            args[0]
        );
    }

    /// A trivial φ (single operand) collapses to that operand everywhere.
    #[test]
    fn eliminates_trivial_phi() {
        use crate::decompiler::cfg::ssa::PhiNode;
        let mut block = SsaBlock::new();
        // phi p0_0 = φ(BB1: b1_0)  — single operand, trivial.
        let mut phi = PhiNode::new(v("p0", 0));
        phi.add_operand(BlockId(1), v("b1", 0));
        block.add_phi(phi);
        // b1_0 = 5
        let mut pred = SsaBlock::new();
        pred.add_stmt(assign_str(v("b1", 0), SsaExpr::lit(Literal::Int(5))));
        // use of p0_0
        block.add_stmt(assign_str(
            v("b0", 0),
            SsaExpr::call("use".to_string(), vec![SsaExpr::var(v("p0", 0))]),
        ));

        let mut blocks = BTreeMap::new();
        blocks.insert(BlockId(0), block);
        blocks.insert(BlockId(1), pred);
        let mut ssa = rebuild_test_form(Cfg::new(), DominanceInfo::new(), blocks);
        optimize(&mut ssa);

        let b0 = ssa.block(BlockId(0)).unwrap();
        let SsaStmt::Assign { value, .. } = b0.stmts.last().unwrap() else {
            panic!();
        };
        let SsaExpr::Call { args, .. } = value else {
            panic!();
        };
        assert!(
            matches!(args[0], SsaExpr::Literal(Literal::Int(5))),
            "trivial phi should resolve through to 5, got {:?}",
            args[0]
        );
    }

    /// `v0 = 7` with no uses is dead and is removed.
    #[test]
    fn eliminates_dead_constant_def() {
        let mut block = SsaBlock::new();
        block.add_stmt(assign_str(v("b0", 0), SsaExpr::lit(Literal::Int(7))));
        // A *used* def so the block isn't empty and used-tracking is exercised.
        block.add_stmt(assign_str(
            v("b0", 1),
            SsaExpr::call("use".to_string(), vec![SsaExpr::lit(Literal::Int(1))]),
        ));

        let mut blocks = BTreeMap::new();
        blocks.insert(BlockId(0), block);
        let mut ssa = rebuild_test_form(Cfg::new(), DominanceInfo::new(), blocks);
        let before = ssa.block(BlockId(0)).unwrap().stmt_count();
        optimize(&mut ssa);
        let after = ssa.block(BlockId(0)).unwrap().stmt_count();
        assert!(after < before, "dead constant def b0#0 should be removed");
        assert!(!ssa.definitions.contains_key(&v("b0", 0)));
    }
}
