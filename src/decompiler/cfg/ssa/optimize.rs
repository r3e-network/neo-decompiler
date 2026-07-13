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

    /// A named slot's constant value must NOT be substituted into its uses: a
    /// decompiler preserves the user-visible variable reference (`loc0 < 3`)
    /// rather than erasing it (`0 < 3` → `true`), which would dissolve the
    /// branch condition the structurer needs. Temps (`b0`) still fold.
    #[test]
    fn does_not_propagate_constant_through_a_slot_variable() {
        let mut block = SsaBlock::new();
        // loc0_0 = 0  (a named local slot, not an anonymous temp)
        block.add_stmt(assign_str(v("loc0", 0), SsaExpr::lit(Literal::Int(0))));
        // t_0 = (loc0_0 < 3)  — must stay referencing loc0_0, not fold to `true`.
        block.add_stmt(assign_str(
            v("t", 0),
            SsaExpr::binary(
                BinOp::Lt,
                SsaExpr::var(v("loc0", 0)),
                SsaExpr::lit(Literal::Int(3)),
            ),
        ));
        // Keep t_0 live.
        block.add_stmt(assign_str(
            v("t", 1),
            SsaExpr::unresolved_call("use", vec![SsaExpr::var(v("t", 0))]),
        ));

        let mut blocks = BTreeMap::new();
        blocks.insert(BlockId(0), block);
        let mut ssa = rebuild_test_form(Cfg::new(), DominanceInfo::new(), blocks);
        optimize(&mut ssa);

        let b0 = ssa.block(BlockId(0)).unwrap();
        let cmp_stmt = b0
            .stmts
            .iter()
            .find(|s| matches!(s, SsaStmt::Assign { target, .. } if target == &v("t", 0)))
            .expect("t_0 def should survive");
        let SsaStmt::Assign { value, .. } = cmp_stmt else {
            panic!();
        };
        // The left operand must still be the slot VARIABLE, not Literal(0).
        let SsaExpr::Binary { left, .. } = value else {
            panic!("expected the comparison to survive, got {value:?}");
        };
        assert!(
            matches!(left.as_ref(), SsaExpr::Variable(var) if var == &v("loc0", 0)),
            "loc0_0 should stay symbolic in `loc0 < 3`, not be replaced by 0; got {value:?}"
        );
    }

    #[test]
    fn propagates_vm_null_through_a_slot_load_alias() {
        let local = v("loc0", 0);
        let mut block = SsaBlock::new();
        block.add_stmt(assign_str(
            local.clone(),
            SsaExpr::var(SsaVariable::vm_null()),
        ));
        block.add_stmt(SsaStmt::ret(Some(SsaExpr::var(local.clone()))));

        let mut blocks = BTreeMap::new();
        blocks.insert(BlockId(0), block);
        let mut ssa = rebuild_test_form(Cfg::new(), DominanceInfo::new(), blocks);

        optimize(&mut ssa);

        let block = ssa.block(BlockId(0)).expect("entry block");
        assert!(!block.stmts.iter().any(
            |statement| matches!(statement, SsaStmt::Assign { target, .. } if target == &local)
        ));
        assert!(block.stmts.iter().any(|statement| matches!(
            statement,
            SsaStmt::Return(Some(SsaExpr::Variable(value))) if value.is_vm_null()
        )));
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
            SsaExpr::unresolved_call("use", vec![SsaExpr::var(v("b0", 2))]),
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
            SsaExpr::unresolved_call("use", vec![SsaExpr::var(v("b0", 1))]),
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
            SsaExpr::unresolved_call("use", vec![SsaExpr::var(v("p0", 0))]),
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
        assert!(
            ssa.block(BlockId(0))
                .expect("merge block")
                .phi_nodes
                .is_empty(),
            "substituted phi node must be removed"
        );
        assert!(!ssa.definitions.contains_key(&v("p0", 0)));
        assert_eq!(optimize(&mut ssa), 0, "optimized form must converge");
    }

    #[test]
    fn retargets_terminator_use_when_removing_variable_trivial_phi() {
        let incoming = v("condition", 0);
        let target = v("merged_condition", 0);
        let merge_id = BlockId(0);
        let mut merge = SsaBlock::new();
        let mut phi = crate::decompiler::cfg::ssa::PhiNode::new(target.clone());
        phi.add_operand(BlockId(1), incoming.clone());
        merge.add_phi(phi);

        let mut predecessor = SsaBlock::new();
        predecessor.add_stmt(assign_str(
            incoming.clone(),
            SsaExpr::unresolved_call("condition", vec![]),
        ));

        let blocks = BTreeMap::from([(merge_id, merge), (BlockId(1), predecessor)]);
        let mut ssa = rebuild_test_form(Cfg::new(), DominanceInfo::new(), blocks);
        let terminator_use = UseSite::terminator(merge_id);
        ssa.uses
            .entry(target.clone())
            .or_default()
            .insert(terminator_use.clone());

        optimize(&mut ssa);

        assert!(ssa
            .block(merge_id)
            .expect("merge block")
            .phi_nodes
            .is_empty());
        assert!(!ssa.definitions.contains_key(&target));
        assert!(!ssa.uses.contains_key(&target));
        assert!(ssa.definitions.contains_key(&incoming));
        assert_eq!(
            ssa.uses.get(&incoming),
            Some(&BTreeSet::from([terminator_use]))
        );
        assert_eq!(optimize(&mut ssa), 0, "optimized form must converge");
    }

    #[test]
    fn retargets_terminator_through_variable_trivial_phi_chain() {
        let incoming = v("condition", 0);
        let intermediate = v("intermediate_condition", 0);
        let target = v("merged_condition", 0);
        let merge_id = BlockId(0);
        let mut merge = SsaBlock::new();
        let mut outer_phi = crate::decompiler::cfg::ssa::PhiNode::new(target.clone());
        outer_phi.add_operand(BlockId(1), intermediate.clone());
        merge.add_phi(outer_phi);
        let mut inner_phi = crate::decompiler::cfg::ssa::PhiNode::new(intermediate.clone());
        inner_phi.add_operand(BlockId(1), incoming.clone());
        merge.add_phi(inner_phi);

        let mut predecessor = SsaBlock::new();
        predecessor.add_stmt(assign_str(
            incoming.clone(),
            SsaExpr::unresolved_call("condition", vec![]),
        ));

        let blocks = BTreeMap::from([(merge_id, merge), (BlockId(1), predecessor)]);
        let mut ssa = rebuild_test_form(Cfg::new(), DominanceInfo::new(), blocks);
        let terminator_use = UseSite::terminator(merge_id);
        ssa.uses
            .entry(target.clone())
            .or_default()
            .insert(terminator_use.clone());

        optimize(&mut ssa);

        assert!(ssa
            .block(merge_id)
            .expect("merge block")
            .phi_nodes
            .is_empty());
        assert!(!ssa.definitions.contains_key(&target));
        assert!(!ssa.definitions.contains_key(&intermediate));
        assert!(!ssa.uses.contains_key(&target));
        assert!(!ssa.uses.contains_key(&intermediate));
        assert!(ssa.definitions.contains_key(&incoming));
        assert_eq!(
            ssa.uses.get(&incoming),
            Some(&BTreeSet::from([terminator_use]))
        );
        assert_eq!(optimize(&mut ssa), 0, "optimized form must converge");
    }

    #[test]
    fn rewrites_expression_through_variable_trivial_phi_chain() {
        let incoming = v("value", 0);
        let intermediate = v("intermediate_value", 0);
        let target = v("merged_value", 0);
        let mut merge = SsaBlock::new();
        let mut outer_phi = crate::decompiler::cfg::ssa::PhiNode::new(target.clone());
        outer_phi.add_operand(BlockId(1), intermediate.clone());
        merge.add_phi(outer_phi);
        let mut inner_phi = crate::decompiler::cfg::ssa::PhiNode::new(intermediate.clone());
        inner_phi.add_operand(BlockId(1), incoming.clone());
        merge.add_phi(inner_phi);
        merge.add_stmt(SsaStmt::expr(SsaExpr::unresolved_call(
            "use".to_string(),
            vec![SsaExpr::var(target.clone())],
        )));

        let mut predecessor = SsaBlock::new();
        predecessor.add_stmt(assign_str(
            incoming.clone(),
            SsaExpr::unresolved_call("value", vec![]),
        ));

        let blocks = BTreeMap::from([(BlockId(0), merge), (BlockId(1), predecessor)]);
        let mut ssa = rebuild_test_form(Cfg::new(), DominanceInfo::new(), blocks);

        optimize(&mut ssa);

        let effect = ssa
            .block(BlockId(0))
            .expect("merge block")
            .stmts
            .iter()
            .find_map(|stmt| match stmt {
                SsaStmt::Expr(SsaExpr::Call { target, args }) if target.display_name() == "use" => {
                    Some(args)
                }
                _ => None,
            })
            .expect("effect use");
        assert_eq!(effect, &[SsaExpr::var(incoming.clone())]);
        assert!(!ssa.definitions.contains_key(&target));
        assert!(!ssa.definitions.contains_key(&intermediate));
        assert!(ssa.definitions.contains_key(&incoming));
        assert_eq!(optimize(&mut ssa), 0, "optimized form must converge");
    }

    #[test]
    fn rewrites_expression_before_pruning_surviving_phi_operands() {
        let left = v("left", 0);
        let right = v("right", 0);
        let live_phi_target = v("live_phi", 0);
        let alias_target = v("alias", 0);
        let mut merge = SsaBlock::new();
        let mut live_phi = crate::decompiler::cfg::ssa::PhiNode::new(live_phi_target.clone());
        live_phi.add_operand(BlockId(1), left.clone());
        live_phi.add_operand(BlockId(2), right.clone());
        merge.add_phi(live_phi);
        let mut alias_phi = crate::decompiler::cfg::ssa::PhiNode::new(alias_target.clone());
        alias_phi.add_operand(BlockId(1), live_phi_target.clone());
        merge.add_phi(alias_phi);
        merge.add_stmt(SsaStmt::expr(SsaExpr::unresolved_call(
            "use".to_string(),
            vec![SsaExpr::var(alias_target.clone())],
        )));

        let mut left_predecessor = SsaBlock::new();
        left_predecessor.add_stmt(assign_str(left.clone(), SsaExpr::lit(Literal::Int(1))));
        let mut right_predecessor = SsaBlock::new();
        right_predecessor.add_stmt(assign_str(right.clone(), SsaExpr::lit(Literal::Int(2))));

        let blocks = BTreeMap::from([
            (BlockId(0), merge),
            (BlockId(1), left_predecessor),
            (BlockId(2), right_predecessor),
        ]);
        let mut ssa = rebuild_test_form(Cfg::new(), DominanceInfo::new(), blocks);

        optimize(&mut ssa);

        let merge = ssa.block(BlockId(0)).expect("merge block");
        assert!(merge
            .phi_nodes
            .iter()
            .any(|phi| phi.target == live_phi_target));
        assert!(!merge.phi_nodes.iter().any(|phi| phi.target == alias_target));
        let effect = merge
            .stmts
            .iter()
            .find_map(|stmt| match stmt {
                SsaStmt::Expr(SsaExpr::Call { target, args }) if target.display_name() == "use" => {
                    Some(args)
                }
                _ => None,
            })
            .expect("effect use");
        assert_eq!(effect, &[SsaExpr::var(live_phi_target)]);
        assert!(ssa.definitions.contains_key(&left));
        assert!(ssa.definitions.contains_key(&right));
        assert_eq!(optimize(&mut ssa), 0, "optimized form must converge");
    }

    #[test]
    fn leaves_rooted_cyclic_phis_stable() {
        let a = v("a", 0);
        let b = v("b", 0);
        let mut phi_a = crate::decompiler::cfg::ssa::PhiNode::new(a.clone());
        phi_a.add_operand(BlockId(1), b.clone());
        let mut phi_b = crate::decompiler::cfg::ssa::PhiNode::new(b.clone());
        phi_b.add_operand(BlockId(1), a.clone());
        let mut block = SsaBlock::new();
        block.add_phi(phi_a);
        block.add_phi(phi_b);
        block.add_stmt(SsaStmt::expr(SsaExpr::unresolved_call(
            "use".to_string(),
            vec![SsaExpr::var(a.clone())],
        )));

        let blocks = BTreeMap::from([(BlockId(0), block)]);
        let mut ssa = rebuild_test_form(Cfg::new(), DominanceInfo::new(), blocks);

        assert_eq!(
            one_round(&mut ssa),
            0,
            "cyclic substitutions are not rewrites"
        );
        assert_eq!(optimize(&mut ssa), 0, "cyclic phis must remain converged");
        assert!(ssa.definitions.contains_key(&a));
        assert!(ssa.definitions.contains_key(&b));
    }

    #[test]
    fn preserves_literal_trivial_phi_used_by_terminator() {
        let incoming = v("condition", 0);
        let target = v("merged_condition", 0);
        let merge_id = BlockId(0);
        let mut merge = SsaBlock::new();
        let mut phi = crate::decompiler::cfg::ssa::PhiNode::new(target.clone());
        phi.add_operand(BlockId(1), incoming.clone());
        merge.add_phi(phi);

        let mut predecessor = SsaBlock::new();
        predecessor.add_stmt(assign_str(
            incoming.clone(),
            SsaExpr::lit(Literal::Bool(true)),
        ));

        let blocks = BTreeMap::from([(merge_id, merge), (BlockId(1), predecessor)]);
        let mut ssa = rebuild_test_form(Cfg::new(), DominanceInfo::new(), blocks);
        let terminator_use = UseSite::terminator(merge_id);
        ssa.uses
            .entry(target.clone())
            .or_default()
            .insert(terminator_use.clone());

        optimize(&mut ssa);

        assert!(ssa
            .block(merge_id)
            .expect("merge block")
            .phi_nodes
            .iter()
            .any(|phi| phi.target == target));
        assert!(ssa.definitions.contains_key(&target));
        assert_eq!(
            ssa.uses.get(&target),
            Some(&BTreeSet::from([terminator_use]))
        );
        assert_eq!(optimize(&mut ssa), 0, "optimized form must converge");
    }

    #[test]
    fn preserves_literal_trivial_phi_used_by_nontrivial_phi() {
        let incoming = v("constant", 0);
        let literal_phi_target = v("literal_phi", 0);
        let other = v("other", 0);
        let live_phi_target = v("live_phi", 0);

        let mut merge = SsaBlock::new();
        let mut literal_phi = crate::decompiler::cfg::ssa::PhiNode::new(literal_phi_target.clone());
        literal_phi.add_operand(BlockId(1), incoming.clone());
        merge.add_phi(literal_phi);
        let mut live_phi = crate::decompiler::cfg::ssa::PhiNode::new(live_phi_target.clone());
        live_phi.add_operand(BlockId(1), literal_phi_target.clone());
        live_phi.add_operand(BlockId(2), other.clone());
        merge.add_phi(live_phi);
        merge.add_stmt(SsaStmt::expr(SsaExpr::unresolved_call(
            "use".to_string(),
            vec![SsaExpr::var(live_phi_target.clone())],
        )));

        let mut constant_predecessor = SsaBlock::new();
        constant_predecessor.add_stmt(assign_str(incoming, SsaExpr::lit(Literal::Int(7))));
        let mut other_predecessor = SsaBlock::new();
        other_predecessor.add_stmt(assign_str(other, SsaExpr::unresolved_call("other", vec![])));

        let blocks = BTreeMap::from([
            (BlockId(0), merge),
            (BlockId(1), constant_predecessor),
            (BlockId(2), other_predecessor),
        ]);
        let mut ssa = rebuild_test_form(Cfg::new(), DominanceInfo::new(), blocks);

        optimize(&mut ssa);

        let merge = ssa.block(BlockId(0)).expect("merge block");
        assert!(merge
            .phi_nodes
            .iter()
            .any(|phi| phi.target == literal_phi_target));
        let live_phi = merge
            .phi_nodes
            .iter()
            .find(|phi| phi.target == live_phi_target)
            .expect("live nontrivial phi");
        assert_eq!(
            live_phi.operands.get(&BlockId(1)),
            Some(&literal_phi_target)
        );
        assert!(ssa.definitions.contains_key(&literal_phi_target));
        assert_eq!(optimize(&mut ssa), 0, "optimized form must converge");
    }

    #[test]
    fn removes_dead_phi_and_releases_operand_definition() {
        let incoming = v("b1", 0);
        let target = v("p0", 0);
        let mut merge = SsaBlock::new();
        let mut phi = crate::decompiler::cfg::ssa::PhiNode::new(target.clone());
        phi.add_operand(BlockId(1), incoming.clone());
        merge.add_phi(phi);
        merge.add_stmt(SsaStmt::ret(None));

        let mut predecessor = SsaBlock::new();
        predecessor.add_stmt(assign_str(incoming.clone(), SsaExpr::lit(Literal::Int(5))));

        let blocks = BTreeMap::from([(BlockId(0), merge), (BlockId(1), predecessor)]);
        let mut ssa = rebuild_test_form(Cfg::new(), DominanceInfo::new(), blocks);

        optimize(&mut ssa);

        assert!(ssa
            .block(BlockId(0))
            .expect("merge block")
            .phi_nodes
            .is_empty());
        assert!(!ssa.definitions.contains_key(&target));
        assert!(!ssa.definitions.contains_key(&incoming));
        assert!(!ssa.uses.contains_key(&incoming));
    }

    #[test]
    fn converges_long_reverse_copy_chain_in_one_call() {
        let chain_len = 512usize;
        let mut block = SsaBlock::new();
        for index in 0..chain_len {
            let value = if index + 1 == chain_len {
                SsaExpr::lit(Literal::Int(7))
            } else {
                SsaExpr::var(v("t", index + 1))
            };
            block.add_stmt(assign_str(v("t", index), value));
        }
        block.add_stmt(SsaStmt::expr(SsaExpr::unresolved_call(
            "use".to_string(),
            vec![SsaExpr::var(v("t", 0))],
        )));
        block.add_stmt(SsaStmt::ret(None));

        let blocks = BTreeMap::from([(BlockId(0), block)]);
        let mut ssa = rebuild_test_form(Cfg::new(), DominanceInfo::new(), blocks);

        optimize(&mut ssa);

        let effect = ssa
            .block(BlockId(0))
            .expect("chain block")
            .stmts
            .iter()
            .find_map(|stmt| match stmt {
                SsaStmt::Expr(SsaExpr::Call { target, args }) if target.display_name() == "use" => {
                    Some(args)
                }
                _ => None,
            })
            .expect("effect use");
        assert_eq!(effect, &[SsaExpr::lit(Literal::Int(7))]);
        assert_eq!(
            optimize(&mut ssa),
            0,
            "a single optimize call must reach its fixed point"
        );
    }

    #[test]
    fn removes_dead_mutually_dependent_phi_component() {
        let a = v("a", 0);
        let b = v("b", 0);
        let mut phi_a = crate::decompiler::cfg::ssa::PhiNode::new(a.clone());
        phi_a.add_operand(BlockId(1), b.clone());
        phi_a.add_operand(BlockId(2), v("x", 0));
        let mut phi_b = crate::decompiler::cfg::ssa::PhiNode::new(b.clone());
        phi_b.add_operand(BlockId(1), a.clone());
        phi_b.add_operand(BlockId(2), v("y", 0));
        let mut block = SsaBlock::new();
        block.add_phi(phi_a);
        block.add_phi(phi_b);
        block.add_stmt(SsaStmt::ret(None));

        let blocks = BTreeMap::from([(BlockId(0), block)]);
        let mut ssa = rebuild_test_form(Cfg::new(), DominanceInfo::new(), blocks);

        optimize(&mut ssa);

        assert!(ssa
            .block(BlockId(0))
            .expect("phi block")
            .phi_nodes
            .is_empty());
        assert!(!ssa.definitions.contains_key(&a));
        assert!(!ssa.definitions.contains_key(&b));
        assert_eq!(optimize(&mut ssa), 0);
    }

    /// `v0 = 7` with no uses is dead and is removed.
    #[test]
    fn eliminates_dead_constant_def() {
        let mut block = SsaBlock::new();
        block.add_stmt(assign_str(v("b0", 0), SsaExpr::lit(Literal::Int(7))));
        // A *used* def so the block isn't empty and used-tracking is exercised.
        block.add_stmt(assign_str(
            v("b0", 1),
            SsaExpr::unresolved_call("use", vec![SsaExpr::lit(Literal::Int(1))]),
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

    #[test]
    fn effect_statement_keeps_input_definition_live() {
        let input = v("t", 0);
        let key = v("t", 1);
        let mut block = SsaBlock::new();
        block.add_stmt(assign_str(
            input.clone(),
            SsaExpr::unresolved_call("newmap", vec![]),
        ));
        block.add_stmt(assign_str(key.clone(), SsaExpr::lit(Literal::Int(1))));
        block.add_stmt(SsaStmt::expr(SsaExpr::unresolved_call(
            "set_item".to_string(),
            vec![
                SsaExpr::var(input.clone()),
                SsaExpr::var(key.clone()),
                SsaExpr::lit(Literal::Int(2)),
            ],
        )));
        block.add_stmt(SsaStmt::ret(None));

        let mut blocks = BTreeMap::new();
        blocks.insert(BlockId(0), block);
        let mut ssa = rebuild_test_form(Cfg::new(), DominanceInfo::new(), blocks);
        let rounds = optimize(&mut ssa);

        assert!(
            rounds > 0,
            "the regression must exercise an optimization round"
        );

        let block = ssa.block(BlockId(0)).expect("test block exists");
        assert!(
            block.stmts.iter().any(|stmt| matches!(
                stmt,
                SsaStmt::Assign {
                    target,
                    value: SsaExpr::Call { target: call_target, args },
                } if target == &input
                    && call_target.display_name() == "newmap"
                    && args.is_empty()
            )),
            "effect input definition must survive optimization: {block:?}"
        );
        let effect_index = block
            .stmts
            .iter()
            .position(|stmt| {
                matches!(
                    stmt,
                    SsaStmt::Expr(SsaExpr::Call { target, .. })
                        if target.display_name() == "set_item"
                )
            })
            .expect("set_item effect statement must survive optimization");
        assert!(ssa.definitions.contains_key(&input));
        assert!(!ssa.definitions.contains_key(&key));
        assert!(ssa
            .uses
            .get(&input)
            .is_some_and(|sites| sites.iter().any(|site| site.stmt_index == effect_index)));
        assert!(block.stmts.iter().any(|stmt| matches!(
            stmt,
            SsaStmt::Expr(SsaExpr::Call { target, args })
                if target.display_name() == "set_item"
                    && args.get(1) == Some(&SsaExpr::lit(Literal::Int(1)))
        )));
    }
}
