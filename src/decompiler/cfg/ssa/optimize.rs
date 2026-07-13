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

#[cfg(test)]
use crate::decompiler::cfg::BlockId;
use crate::decompiler::ir::{BinOp, Literal, UnaryOp};

use super::form::{SsaExpr, SsaForm, SsaStmt, UseSite};
use super::variable::SsaVariable;

mod indexes;

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

/// A single substitution table mapping a variable to the simpler expression
/// that replaces every reference to it.
type Subst = BTreeMap<SsaVariable, SsaExpr>;

fn one_round(ssa: &mut SsaForm) -> usize {
    // A phi whose target is never consumed is dead. Remove it before gathering
    // substitutions so its operands do not remain artificially live.
    let live_phi_targets = collect_used(ssa, &BTreeMap::new());
    let mut rewrites = 0usize;
    for block in ssa.blocks.values_mut() {
        let before = block.phi_nodes.len();
        block
            .phi_nodes
            .retain(|phi| live_phi_targets.contains(&phi.target));
        rewrites += before - block.phi_nodes.len();
    }

    // Gather constants/copies from current assignments.
    let mut subst: Subst = BTreeMap::new();

    for (_bid, block) in ssa.blocks.iter() {
        for stmt in &block.stmts {
            if let SsaStmt::Assign { target, value } = stmt {
                match value {
                    // Direct constant: propagate the literal — but NOT out of a
                    // named slot variable (loc/arg/static). A slot is a
                    // user-visible variable; substituting its constant into uses
                    // would erase the variable and fold away branch conditions /
                    // arithmetic the structurer needs (e.g. `loc0 = 0; if loc0<3`
                    // must survive, not become `if true`). Anonymous temps fold.
                    SsaExpr::Literal(_) => {
                        if !is_slot_var(target) {
                            subst.insert(target.clone(), value.clone());
                        }
                    }
                    // Copy of another var: `t = v` -> use v. For a slot target,
                    // only collapse to ANOTHER slot (a redundant load-alias);
                    // never to a temp/literal, so the slot reference stays
                    // visible at use sites.
                    SsaExpr::Variable(src) => {
                        if !is_slot_var(target) || is_slot_var(src) || src.is_vm_null() {
                            subst.insert(target.clone(), value.clone());
                        }
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

    // Collapse acyclic substitution chains to their terminal expression and
    // discard cycles. Consumers must never be rewritten only partway through a
    // chain whose intermediate definition is removed in this round.
    subst = normalize_substitutions(&subst);

    if subst.is_empty() {
        if rewrites > 0 {
            indexes::rebuild_indexes(ssa);
        }
        return rewrites;
    }

    // Phi operands and terminator conditions can only name variables. A
    // non-variable phi result therefore has to retain its defining phi while
    // either kind of consumer still references it. Determine phi consumers
    // from the nodes themselves: their synthetic UseSite indexes are not
    // distinguishable from ordinary statement uses.
    let phi_operand_targets: BTreeSet<_> = ssa
        .blocks
        .values()
        .flat_map(|block| &block.phi_nodes)
        .flat_map(|phi| {
            phi.operands
                .values()
                .filter(|operand| *operand != &phi.target)
                .cloned()
        })
        .collect();
    let terminator_targets: BTreeSet<_> = ssa
        .uses
        .iter()
        .filter(|(_, sites)| sites.iter().any(UseSite::is_terminator))
        .map(|(variable, _)| variable.clone())
        .collect();

    // Chase variable substitutions before deleting their definitions. This is
    // required for chains of removable phis: every variable-only consumer must
    // be rewritten to a target that will still be defined after this round.
    let variable_replacements: BTreeMap<_, _> = subst
        .iter()
        .filter_map(|(target, replacement)| match replacement {
            SsaExpr::Variable(resolved) => Some((target.clone(), resolved.clone())),
            _ => None,
        })
        .collect();
    let removable_phi_targets: BTreeSet<_> = ssa
        .blocks
        .values()
        .flat_map(|block| &block.phi_nodes)
        .filter_map(|phi| {
            let replacement = subst.get(&phi.target)?;
            let removable = match replacement {
                SsaExpr::Variable(_) => variable_replacements.contains_key(&phi.target),
                _ => {
                    !terminator_targets.contains(&phi.target)
                        && !phi_operand_targets.contains(&phi.target)
                }
            };
            removable.then(|| phi.target.clone())
        })
        .collect();

    // Rewrite variable-only consumers before liveness and DCE. Rebuilding the
    // indexes later preserves the retargeted terminator entries, since no
    // mutable terminator expression exists in SsaForm.
    for block in ssa.blocks.values_mut() {
        for phi in &mut block.phi_nodes {
            for var in phi.operands.values_mut() {
                if let Some(replacement) = variable_replacements.get(var) {
                    if replacement != var {
                        *var = replacement.clone();
                        rewrites += 1;
                    }
                }
            }
        }
    }
    for target in &removable_phi_targets {
        let Some(replacement) = variable_replacements.get(target) else {
            continue;
        };
        let terminator_uses: Vec<_> = ssa
            .uses
            .get(target)
            .into_iter()
            .flatten()
            .filter(|site| site.is_terminator())
            .cloned()
            .collect();
        if terminator_uses.is_empty() {
            continue;
        }
        let mut remove_target_entry = false;
        if let Some(sites) = ssa.uses.get_mut(target) {
            sites.retain(|site| !site.is_terminator());
            remove_target_entry = sites.is_empty();
        }
        if remove_target_entry {
            ssa.uses.remove(target);
        }
        let replacement_sites = ssa.uses.entry(replacement.clone()).or_default();
        for site in terminator_uses {
            rewrites += usize::from(replacement_sites.insert(site));
        }
    }
    for block in ssa.blocks.values_mut() {
        let before = block.phi_nodes.len();
        block
            .phi_nodes
            .retain(|phi| !removable_phi_targets.contains(&phi.target));
        rewrites += before - block.phi_nodes.len();
    }

    // Rewrite expression roots before liveness. A removed alias phi may point
    // at a surviving nontrivial phi; its rewritten expression use must keep the
    // surviving phi and its incoming definitions live.
    for (_bid, block) in ssa.blocks.iter_mut() {
        for stmt in &mut block.stmts {
            match stmt {
                SsaStmt::Assign { value, .. } => {
                    let new_value = rewrite_expr(value, &subst);
                    if new_value != *value {
                        *value = new_value;
                        rewrites += 1;
                    }
                }
                SsaStmt::Return(Some(value)) => {
                    let new_value = rewrite_expr(value, &subst);
                    if new_value != *value {
                        *value = new_value;
                        rewrites += 1;
                    }
                }
                SsaStmt::Expr(value) => {
                    let new_value = rewrite_expr(value, &subst);
                    if new_value != *value {
                        *value = new_value;
                        rewrites += 1;
                    }
                }
                SsaStmt::Throw(Some(value)) | SsaStmt::Abort(Some(value)) => {
                    let new_value = rewrite_expr(value, &subst);
                    if new_value != *value {
                        *value = new_value;
                        rewrites += 1;
                    }
                }
                SsaStmt::Assert { condition, message } => {
                    let new_condition = rewrite_expr(condition, &subst);
                    if new_condition != *condition {
                        *condition = new_condition;
                        rewrites += 1;
                    }
                    if let Some(message) = message {
                        let new_message = rewrite_expr(message, &subst);
                        if new_message != *message {
                            *message = new_message;
                            rewrites += 1;
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

    // Drop pure substituted definitions only after every representable consumer
    // has been rewritten to its final value.
    let used = collect_used(ssa, &subst);
    for block in ssa.blocks.values_mut() {
        let before = block.stmts.len();
        block.stmts.retain(|stmt| match stmt {
            SsaStmt::Assign { target, .. } => !subst.contains_key(target) || used.contains(target),
            _ => true,
        });
        rewrites += before - block.stmts.len();
    }

    // Rebuild the definitions/uses indexes so downstream consumers stay correct.
    if rewrites > 0 {
        indexes::rebuild_indexes(ssa);
    }
    rewrites
}

/// Resolve every acyclic variable chain to its terminal variable or expression.
/// A cyclic mapping is omitted so it cannot cause oscillating rewrites.
fn normalize_substitutions(subst: &Subst) -> Subst {
    subst
        .keys()
        .filter_map(|target| {
            let mut seen = BTreeSet::new();
            let mut current = target.clone();
            while seen.insert(current.clone()) {
                match subst.get(&current) {
                    Some(SsaExpr::Variable(next)) => current = next.clone(),
                    Some(replacement) => return Some((target.clone(), replacement.clone())),
                    None => {
                        return (current != *target)
                            .then(|| (target.clone(), SsaExpr::var(current)));
                    }
                }
            }
            None
        })
        .collect()
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

/// Extract a [`Literal`] from an expression that is (or substitutes to) one.
fn as_literal(expr: &SsaExpr) -> Option<Literal> {
    if let SsaExpr::Literal(l) = expr {
        Some(l.clone())
    } else {
        None
    }
}

/// Whether `v` is a named slot variable (a local/argument/static field). Such
/// variables are user-visible; the optimizer keeps them symbolic instead of
/// substituting their constant value into uses (see `one_round`). Slot bases are
/// `locN` / `argN` / `staticN` as produced by the SSA builder's `slot_name_for`.
fn is_slot_var(v: &SsaVariable) -> bool {
    let b = v.base.as_str();
    b.starts_with("loc") || b.starts_with("arg") || b.starts_with("static")
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
    for (_bid, block) in ssa.blocks.iter() {
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
