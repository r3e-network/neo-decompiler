//! Individual SSA optimization passes, each with clear preconditions and postconditions.

use std::collections::{BTreeMap, BTreeSet};

use crate::decompiler::ir::{BinOp, Literal, UnaryOp};

use super::super::form::{SsaExpr, SsaForm, SsaStmt, UseSite};
use super::super::variable::SsaVariable;

/// A single substitution table mapping a variable to the simpler expression
/// that replaces every reference to it.
pub(super) type Subst = BTreeMap<SsaVariable, SsaExpr>;

/// Pass 1: Remove dead phi nodes whose targets are never used.
///
/// **Precondition:** None
/// **Postcondition:** All remaining phi targets have at least one use
pub(super) fn dead_phi_elimination_pass(ssa: &mut SsaForm) -> usize {
    let live_phi_targets = super::collect_used(ssa, &BTreeMap::new());
    let mut rewrites = 0usize;
    for block in ssa.blocks.values_mut() {
        let before = block.phi_nodes.len();
        block
            .phi_nodes
            .retain(|phi| live_phi_targets.contains(&phi.target));
        rewrites += before - block.phi_nodes.len();
    }
    rewrites
}

/// Pass 2: Build substitution table from constants, copies, and foldable expressions.
///
/// **Precondition:** None
/// **Postcondition:** Returns mapping of variables to simpler expressions
pub(super) fn build_substitution_map(ssa: &SsaForm) -> Subst {
    let mut subst: Subst = BTreeMap::new();

    // Gather constants/copies from assignments
    for block in ssa.blocks.values() {
        for stmt in &block.stmts {
            if let SsaStmt::Assign { target, value } = stmt {
                match value {
                    // Direct constant: propagate the literal — but NOT out of a
                    // named slot variable (loc/arg/static). A slot is a
                    // user-visible variable; substituting its constant into uses
                    // would erase the variable and fold away branch conditions /
                    // arithmetic the structurer needs (e.g. `loc0 = 0; if loc0<3`
                    // must survive, not become `if true`). Anonymous temps fold.
                    SsaExpr::Literal(_) if !is_slot_var(target) => {
                        subst.insert(target.clone(), value.clone());
                    }
                    // Copy of another var: `t = v` -> use v. For a slot target,
                    // only collapse to ANOTHER slot (a redundant load-alias);
                    // never to a temp/literal, so the slot reference stays
                    // visible at use sites.
                    SsaExpr::Variable(src)
                        if !is_slot_var(target) || is_slot_var(src) || src.is_vm_null() =>
                    {
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
    for block in ssa.blocks.values() {
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

    subst
}

/// Pass 3: Trivial-φ elimination.
///
/// **Precondition:** `subst` contains initial substitutions
/// **Postcondition:** `subst` extended with collapsed φ nodes
pub(super) fn trivial_phi_elimination_pass(ssa: &SsaForm, subst: &mut Subst) {
    for block in ssa.blocks.values() {
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
            resolved_first = resolve_once(subst, &resolved_first);
            let all_same = operands.iter().all(|v| {
                let r = subst
                    .get(*v)
                    .cloned()
                    .unwrap_or_else(|| SsaExpr::var((*v).clone()));
                resolve_once(subst, &r) == resolved_first
            });
            if all_same {
                subst.insert(phi.target.clone(), resolved_first);
            }
        }
    }
}

/// Pass 4: Normalize substitution chains to terminal expressions.
///
/// **Precondition:** `subst` may contain chains
/// **Postcondition:** Returns normalized substitutions with cycles removed
pub(super) fn normalize_substitutions(subst: &Subst) -> Subst {
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

/// Pass 5: Remove phi nodes that can be safely eliminated.
///
/// **Precondition:** `subst` is normalized
/// **Postcondition:** Removable phi nodes deleted, variable-only consumers rewritten
pub(super) fn removable_phi_pass(ssa: &mut SsaForm, subst: &Subst) -> usize {
    let mut rewrites = 0usize;

    // Phi operands and terminator conditions can only name variables. A
    // non-variable phi result therefore has to retain its defining phi while
    // either kind of consumer still references it.
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

    // Chase variable substitutions before deleting their definitions.
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

    // Rewrite variable-only consumers before liveness and DCE.
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

    rewrites
}

/// Pass 6: Apply substitutions to all expression roots.
///
/// **Precondition:** `subst` is normalized
/// **Postcondition:** All expressions rewritten with substituted values
pub(super) fn apply_substitutions_pass(ssa: &mut SsaForm, subst: &Subst) -> usize {
    let mut rewrites = 0usize;

    for block in ssa.blocks.values_mut() {
        for stmt in &mut block.stmts {
            match stmt {
                SsaStmt::Assign { value, .. } => {
                    let new_value = rewrite_expr(value, subst);
                    if new_value != *value {
                        *value = new_value;
                        rewrites += 1;
                    }
                }
                SsaStmt::Return(Some(value)) => {
                    let new_value = rewrite_expr(value, subst);
                    if new_value != *value {
                        *value = new_value;
                        rewrites += 1;
                    }
                }
                SsaStmt::Expr(value) => {
                    let new_value = rewrite_expr(value, subst);
                    if new_value != *value {
                        *value = new_value;
                        rewrites += 1;
                    }
                }
                SsaStmt::Throw(Some(value)) | SsaStmt::Abort(Some(value)) => {
                    let new_value = rewrite_expr(value, subst);
                    if new_value != *value {
                        *value = new_value;
                        rewrites += 1;
                    }
                }
                SsaStmt::Assert { condition, message } => {
                    let new_condition = rewrite_expr(condition, subst);
                    if new_condition != *condition {
                        *condition = new_condition;
                        rewrites += 1;
                    }
                    if let Some(message) = message {
                        let new_message = rewrite_expr(message, subst);
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

    rewrites
}

/// Pass 7: Dead code elimination for substituted assignments.
///
/// **Precondition:** All expressions rewritten with `subst`
/// **Postcondition:** Dead assignments removed
pub(super) fn dead_code_elimination_pass(ssa: &mut SsaForm, subst: &Subst) -> usize {
    let mut rewrites = 0usize;
    let used = super::collect_used(ssa, subst);

    for block in ssa.blocks.values_mut() {
        let before = block.stmts.len();
        block.stmts.retain(|stmt| match stmt {
            SsaStmt::Assign { target, .. } => !subst.contains_key(target) || used.contains(target),
            _ => true,
        });
        rewrites += before - block.stmts.len();
    }

    rewrites
}

// Helper functions

fn is_slot_var(var: &SsaVariable) -> bool {
    let b = var.base.as_str();
    b.starts_with("loc") || b.starts_with("arg") || b.starts_with("static")
}

fn as_literal(expr: &SsaExpr) -> Option<Literal> {
    match expr {
        SsaExpr::Literal(lit) => Some(lit.clone()),
        _ => None,
    }
}

fn fold_binary(op: BinOp, left: &Literal, right: &Literal) -> Option<Literal> {
    super::fold_binary(op, left, right)
}

fn fold_unary(op: UnaryOp, operand: &Literal) -> Option<Literal> {
    super::fold_unary(op, operand)
}

fn resolve_once(subst: &Subst, expr: &SsaExpr) -> SsaExpr {
    if let SsaExpr::Variable(v) = expr {
        if let Some(rep) = subst.get(v) {
            return rep.clone();
        }
    }
    expr.clone()
}

fn rewrite_expr(expr: &SsaExpr, subst: &Subst) -> SsaExpr {
    super::rewrite_expr(expr, subst)
}
