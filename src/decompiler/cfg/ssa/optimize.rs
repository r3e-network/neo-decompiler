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

use std::collections::BTreeSet;

#[cfg(test)]
use std::collections::BTreeMap;
#[cfg(test)]
use crate::decompiler::cfg::BlockId;
use crate::decompiler::ir::{BinOp, Literal, UnaryOp};

use super::form::{SsaExpr, SsaForm, SsaStmt, UseSite};
use super::variable::SsaVariable;

mod indexes;
mod passes;

use passes::Subst;

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
    }
    rounds
}

/// Run a complete optimization round: dead phi elimination, substitution building,
/// trivial phi elimination, normalization, phi removal, expression rewriting, and DCE.
/// Run a complete optimization round: dead phi elimination, substitution building,
/// trivial phi elimination, normalization, phi removal, expression rewriting, and DCE.
fn one_round(ssa: &mut SsaForm) -> usize {
    let mut rewrites = 0usize;

    // Pass 1: Remove dead phi nodes
    rewrites += passes::dead_phi_elimination_pass(ssa);

    // Pass 2: Build substitution map (constant folding, copy propagation)
    let mut subst = passes::build_substitution_map(ssa);

    // Pass 3: Trivial-φ elimination
    passes::trivial_phi_elimination_pass(ssa, &mut subst);

    // Pass 4: Normalize substitution chains
    subst = passes::normalize_substitutions(&subst);

    if subst.is_empty() {
        if rewrites > 0 {
            indexes::rebuild_indexes(ssa);
        }
        return rewrites;
    }

    // Pass 5: Remove substituted phi nodes
    rewrites += passes::removable_phi_pass(ssa, &subst);

    // Pass 6: Apply substitutions to all expressions
    rewrites += passes::apply_substitutions_pass(ssa, &subst);

    // Pass 7: Dead code elimination
    rewrites += passes::dead_code_elimination_pass(ssa, &subst);

    // Rebuild indexes if any rewrites occurred
    if rewrites > 0 {
        indexes::rebuild_indexes(ssa);
    }

    rewrites
}

// Helper functions used by passes module

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
            SsaExpr::Call { target, args } => {
                SsaExpr::call(target.clone(), args.iter().map(|a| go(a, subst)).collect())
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
            SsaExpr::Convert { value, target } => SsaExpr::Convert {
                value: Box::new(go(value, subst)),
                target: *target,
            },
            SsaExpr::IsType { value, target } => SsaExpr::IsType {
                value: Box::new(go(value, subst)),
                target: *target,
            },
            SsaExpr::NewArray {
                length,
                element_type,
            } => SsaExpr::NewArray {
                length: Box::new(go(length, subst)),
                element_type: *element_type,
            },
            SsaExpr::Array(els) => SsaExpr::Array(els.iter().map(|e| go(e, subst)).collect()),
            SsaExpr::Struct(elements) => {
                SsaExpr::Struct(elements.iter().map(|element| go(element, subst)).collect())
            }
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

/// Constant-fold a binary operation on two integer literals when it is safe
/// (no division by zero). Boolean/comparison ops fold to `Bool`.
fn fold_binary(op: BinOp, a: &Literal, b: &Literal) -> Option<Literal> {
    let (Literal::Int(x), Literal::Int(y)) = (a, b) else {
        return None;
    };
    let (x, y) = (*x, *y);
    Some(match op {
        BinOp::Add => Literal::Int(x.checked_add(y)?),
        BinOp::Sub => Literal::Int(x.checked_sub(y)?),
        BinOp::Mul => Literal::Int(x.checked_mul(y)?),
        BinOp::Div => {
            if y == 0 {
                return None;
            }
            Literal::Int(x.checked_div(y)?)
        }
        BinOp::Mod => {
            if y == 0 {
                return None;
            }
            Literal::Int(x.checked_rem(y)?)
        }
        BinOp::Pow => {
            // Neo's integer exponentiation is not represented by the i64
            // fallback for negative exponents. Do not turn an unsupported
            // case into the mathematically unrelated value `1`.
            if y < 0 {
                return None;
            }
            (0..y)
                .take(1024)
                .try_fold(1i64, |acc, _| acc.checked_mul(x))
                .map(Literal::Int)?
        }
        BinOp::And => Literal::Int(x & y),
        BinOp::Or => Literal::Int(x | y),
        BinOp::Xor => Literal::Int(x ^ y),
        BinOp::Shl => {
            let shift = u32::try_from(y).ok().filter(|shift| *shift < 64)?;
            Literal::Int(x.checked_shl(shift)?)
        }
        BinOp::Shr => {
            let shift = u32::try_from(y).ok().filter(|shift| *shift < 64)?;
            Literal::Int(x.checked_shr(shift)?)
        }
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
        UnaryOp::Neg => Literal::Int(x.checked_neg()?),
        UnaryOp::Not => Literal::Int(!x),
        UnaryOp::Abs => Literal::Int(x.checked_abs()?),
        UnaryOp::Inc => Literal::Int(x.checked_add(1)?),
        UnaryOp::Dec => Literal::Int(x.checked_sub(1)?),
        UnaryOp::Sign => Literal::Int(x.signum()),
        UnaryOp::LogicalNot => Literal::Bool(*x == 0),
    })
}

/// Variables reachable from statement/terminator roots. Phi operands become
/// live only when their target is live, so an unreferenced phi component cannot
/// keep itself alive.
fn collect_used(ssa: &SsaForm, _subst: &Subst) -> BTreeSet<SsaVariable> {
    let mut used = BTreeSet::new();
    for (variable, sites) in &ssa.uses {
        if sites.iter().any(UseSite::is_terminator) {
            used.insert(variable.clone());
        }
    }
    for block in ssa.blocks.values() {
        for stmt in &block.stmts {
            match stmt {
                SsaStmt::Assign { value, .. }
                | SsaStmt::Expr(value)
                | SsaStmt::Return(Some(value))
                | SsaStmt::Throw(Some(value))
                | SsaStmt::Abort(Some(value)) => {
                    for v in collect_expr_vars(value) {
                        used.insert(v);
                    }
                }
                SsaStmt::Assert { condition, message } => {
                    for v in collect_expr_vars(condition) {
                        used.insert(v);
                    }
                    if let Some(message) = message {
                        for v in collect_expr_vars(message) {
                            used.insert(v);
                        }
                    }
                }
                SsaStmt::Return(None)
                | SsaStmt::Throw(None)
                | SsaStmt::Abort(None)
                | SsaStmt::Phi(_)
                | SsaStmt::Other(_) => {}
            }
        }
    }

    loop {
        let mut changed = false;
        for block in ssa.blocks.values() {
            for phi in &block.phi_nodes {
                if used.contains(&phi.target) {
                    for operand in phi.operands.values() {
                        changed |= used.insert(operand.clone());
                    }
                }
            }
        }
        if !changed {
            break;
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
            SsaExpr::Convert { value, .. } | SsaExpr::IsType { value, .. } => go(value, out),
            SsaExpr::NewArray { length, .. } => go(length, out),
            SsaExpr::Array(els) => els.iter().for_each(|e| go(e, out)),
            SsaExpr::Struct(elements) => elements.iter().for_each(|element| go(element, out)),
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
    indexes::rebuild_indexes(&mut tmp);
    tmp
}

#[cfg(test)]
mod tests;
