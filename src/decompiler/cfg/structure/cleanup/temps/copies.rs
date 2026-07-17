//! Single-use copy propagation for structured IR.

use crate::decompiler::ir::{Block as IrBlock, Expr, Stmt};
use std::collections::BTreeSet;

use super::queries::{
    collect_collection_bases, is_pure, is_static_slot, statement_assigns_variable,
    statement_mutates_collections, statement_uses_variable,
};
use super::support::{for_each_child_block_mut, substitute_variable, visit_expr, UseCounts};

// ---------------------------------------------------------------------------
// Single-use copy propagation
// ---------------------------------------------------------------------------

pub(super) fn propagate_single_use_copies(block: &mut IrBlock) {
    // One substitution at a time with freshly recomputed use counts: injecting
    // a copy changes occurrence counts, so stale counts could misjudge later
    // candidates.
    loop {
        let counts = UseCounts::of(block);
        if !propagate_one(block, &counts) {
            break;
        }
    }
}

/// Attempt a single copy-propagation step anywhere in the tree (depth-first).
fn propagate_one(block: &mut IrBlock, counts: &UseCounts) -> bool {
    for statement in &mut block.stmts {
        let mut propagated = false;
        for_each_child_block_mut(statement, &mut |child| {
            if !propagated && propagate_one(child, counts) {
                propagated = true;
            }
        });
        if propagated {
            return true;
        }
    }

    let mut index = 0;
    while index < block.stmts.len() {
        if let Some((target, value)) = copy_candidate_at(block, index, counts) {
            substitute_variable(block, &target, &value);
            block.stmts.remove(index);
            return true;
        }
        index += 1;
    }
    false
}

/// Return `(target, value)` when the assignment at `index` can be folded into
/// its single use site without changing semantics.
fn copy_candidate_at(block: &IrBlock, index: usize, counts: &UseCounts) -> Option<(String, Expr)> {
    let Stmt::Assign { target, value } = &block.stmts[index] else {
        return None;
    };
    if is_static_slot(target) || !is_pure(value) {
        return None;
    }
    let mut value_vars = BTreeSet::new();
    visit_expr(value, &mut |expr| {
        if let Expr::Variable(name) = expr {
            value_vars.insert(name.clone());
        }
    });
    if value_vars.contains(target) {
        return None;
    }

    // The target must have exactly one use in the whole method body, and that
    // use must sit in a later statement of this same list (nested blocks of
    // that statement are fine: the definition dominates them).
    if counts.total(target) != 1 {
        return None;
    }
    let use_index = block.stmts[index + 1..]
        .iter()
        .position(|statement| statement_uses_variable(statement, target))
        .map(|position| index + 1 + position)?;
    debug_assert!(use_index > index);

    // Nothing between the definition and the use may rebind the target or any
    // free variable of the value, nor mutate a collection the value reads
    // (SETITEM-style writes or opaque internal calls taking the collection).
    let mut collection_bases = BTreeSet::new();
    collect_collection_bases(value, &mut collection_bases);
    for statement in &block.stmts[index + 1..use_index] {
        if statement_assigns_variable(statement, target)
            || value_vars
                .iter()
                .any(|variable| statement_assigns_variable(statement, variable))
            || statement_mutates_collections(statement, &collection_bases)
        {
            return None;
        }
    }
    // The use statement itself: leaf statements evaluate their expressions
    // (which contain the use) before performing any write, so `x = f(t)` can
    // fold `t` even when `x` appears in `t`'s value. Nested control flow is
    // checked conservatively because branch ordering is not established here.
    let use_statement = &block.stmts[use_index];
    let is_leaf_use = matches!(
        use_statement,
        Stmt::Assign { .. }
            | Stmt::Return(_)
            | Stmt::Throw(_)
            | Stmt::Abort(_)
            | Stmt::ExprStmt(_)
            | Stmt::Assert { .. }
    );
    if !is_leaf_use
        && (statement_assigns_variable(use_statement, target)
            || value_vars
                .iter()
                .any(|variable| statement_assigns_variable(use_statement, variable)))
    {
        return None;
    }
    if statement_mutates_collections(use_statement, &collection_bases) {
        return None;
    }

    Some((target.clone(), value.clone()))
}
