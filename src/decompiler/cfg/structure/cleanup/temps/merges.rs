//! Phi-style branch value-merge folding for structured IR.

use crate::decompiler::ir::{Block as IrBlock, ControlFlow, Expr, Stmt};
use std::collections::BTreeSet;

use super::queries::{expr_mentions_any, statement_uses_variable};
use super::support::{for_each_child_block_mut, substitute_variable, UseCounts};

// Branch value-merge folding
// ---------------------------------------------------------------------------

/// Fold phi-style branch merges into conditional expressions:
/// `if (c) { p = a; } else { p = b; } use(p)` becomes `use(c ? a : b)`, with
/// the boolean short-circuit spellings (`c && a`, `c || b`, ...) when one arm
/// is a literal. This collapses the compiler's argument-validation chains
/// (`if (t is T) { p = t.Length == 20; } else { p = false; } if (p) ...`)
/// back into single readable conditions.
pub(super) fn fold_branch_value_merges(block: &mut IrBlock) {
    for statement in &mut block.stmts {
        for_each_child_block_mut(statement, &mut |child| fold_branch_value_merges(child));
    }

    let counts = UseCounts::of(block);
    let mut index = 0;
    while index + 1 < block.stmts.len() {
        let Some(merge) = match_branch_merge(&block.stmts[index], &counts) else {
            index += 1;
            continue;
        };
        // The single use of the merged variable must be in the immediately
        // following statement, so nothing intervenes between the merge and
        // the use.
        if !statement_uses_variable(&block.stmts[index + 1], &merge.variable) {
            index += 1;
            continue;
        }
        let folded = merge.fold();
        substitute_variable(block, &merge.variable.clone(), &folded);
        block.stmts.remove(index);
        // Re-examine the new statement at this position with fresh counts.
        return fold_branch_value_merges(block);
    }
}

struct BranchMerge {
    variable: String,
    condition: Expr,
    then_value: Expr,
    else_value: Expr,
}

impl BranchMerge {
    /// Fold to a ternary, which is short-circuit by construction — exactly
    /// matching the original if/else's conditional evaluation. (A LogicalAnd
    /// node would render as the VM's *eager* `&`, evaluating the guarded
    /// expression unconditionally; the C# renderer prettifies eligible
    /// ternaries back into `&&`/`||`.)
    fn fold(&self) -> Expr {
        Expr::Ternary {
            condition: Box::new(self.condition.clone()),
            then_expr: Box::new(self.then_value.clone()),
            else_expr: Box::new(self.else_value.clone()),
        }
    }
}

/// Match `if (c) { p = a; } else { p = b; }` where `p` is a compiler temporary
/// used exactly once in the whole body.
fn match_branch_merge(statement: &Stmt, counts: &UseCounts) -> Option<BranchMerge> {
    let Stmt::ControlFlow(control) = statement else {
        return None;
    };
    let ControlFlow::If {
        condition,
        then_branch,
        else_branch: Some(else_branch),
    } = control.as_ref()
    else {
        return None;
    };
    let [Stmt::Assign {
        target: then_target,
        value: then_value,
    }] = then_branch.stmts.as_slice()
    else {
        return None;
    };
    let [Stmt::Assign {
        target: else_target,
        value: else_value,
    }] = else_branch.stmts.as_slice()
    else {
        return None;
    };
    if then_target != else_target || !is_merge_temporary(then_target) {
        return None;
    }
    // Exactly one use anywhere, and neither arm reads the merged variable.
    if counts.total(then_target) != 1 {
        return None;
    }
    if expr_mentions_any(then_value, &BTreeSet::from([then_target.clone()]))
        || expr_mentions_any(else_value, &BTreeSet::from([then_target.clone()]))
    {
        return None;
    }
    Some(BranchMerge {
        variable: then_target.clone(),
        condition: condition.clone(),
        then_value: then_value.clone(),
        else_value: else_value.clone(),
    })
}

/// Phi/SSA temporaries are merge artifacts; source locals are kept as named
/// variables for readability.
fn is_merge_temporary(name: &str) -> bool {
    if let Some(suffix) = name.strip_prefix("t_") {
        return !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit());
    }
    if let Some(rest) = name.strip_prefix('p') {
        if let Some((digits, version)) = rest.split_once('_') {
            return !digits.is_empty()
                && digits.bytes().all(|byte| byte.is_ascii_digit())
                && !version.is_empty()
                && version.bytes().all(|byte| byte.is_ascii_digit());
        }
    }
    false
}
