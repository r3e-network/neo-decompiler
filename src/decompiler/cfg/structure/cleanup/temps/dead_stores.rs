//! Dead-store elimination for structured IR.

use crate::decompiler::ir::{Block as IrBlock, Stmt};

use super::queries::{
    count_expr_uses, expression_has_effectful_call, expression_may_fault, is_static_slot,
};
use super::support::{for_each_child_block_mut, UseCounts};

// ---------------------------------------------------------------------------
// Dead store elimination
// ---------------------------------------------------------------------------

pub(super) fn eliminate_dead_stores(block: &mut IrBlock, counts: &UseCounts) {
    block.stmts.retain_mut(|statement| {
        let Stmt::Assign { target, value } = statement else {
            return true;
        };
        if is_static_slot(target) {
            return true;
        }
        let own_rhs_uses = count_expr_uses(value, target);
        if counts.total(target) > own_rhs_uses {
            return true;
        }
        if expression_has_effectful_call(value) || expression_may_fault(value) {
            // Keep calls and potentially faulting expressions even when their
            // result is unused: removing a generated assignment must not
            // erase a VM exception that the original path could observe.
            if expression_has_effectful_call(value) {
                *statement = Stmt::ExprStmt(value.clone());
            }
            return true;
        }
        // Pure dead stores disappear entirely.
        false
    });

    for statement in &mut block.stmts {
        for_each_child_block_mut(statement, &mut |child| {
            eliminate_dead_stores(child, counts);
        });
    }
}
