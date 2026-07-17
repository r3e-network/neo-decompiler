//! Safety and variable queries shared by temporary-reduction passes.

use crate::decompiler::ir::{
    BinOp, Block as IrBlock, ControlFlow, Expr, Intrinsic, SemanticCallTarget, Stmt, UnaryOp,
};
use crate::instruction::OpCode;
use std::collections::BTreeSet;

use super::support::{visit_expr, visit_stmt_exprs};

// ---------------------------------------------------------------------------
// Small queries
// ---------------------------------------------------------------------------

pub(super) fn statement_uses_variable(statement: &Stmt, variable: &str) -> bool {
    let mut found = false;
    visit_stmt_exprs(statement, &mut |expr| {
        if matches!(expr, Expr::Variable(name) if name == variable) {
            found = true;
        }
    });
    found
}

pub(super) fn count_expr_uses(expr: &Expr, variable: &str) -> usize {
    let mut count = 0;
    visit_expr(expr, &mut |node| {
        if matches!(node, Expr::Variable(name) if name == variable) {
            count += 1;
        }
    });
    count
}

pub(super) fn statement_assigns_variable(statement: &Stmt, variable: &str) -> bool {
    match statement {
        Stmt::Assign { target, .. } => target == variable,
        Stmt::ControlFlow(control) => match control.as_ref() {
            ControlFlow::If {
                then_branch,
                else_branch,
                ..
            } => {
                block_assigns_variable(then_branch, variable)
                    || else_branch
                        .as_ref()
                        .is_some_and(|branch| block_assigns_variable(branch, variable))
            }
            ControlFlow::While { body, .. } | ControlFlow::DoWhile { body, .. } => {
                block_assigns_variable(body, variable)
            }
            ControlFlow::For { init, body, .. } => {
                init.as_deref()
                    .is_some_and(|init| statement_assigns_variable(init, variable))
                    || block_assigns_variable(body, variable)
            }
            ControlFlow::TryCatch {
                try_body,
                catch_body,
                finally_body,
                ..
            } => {
                block_assigns_variable(try_body, variable)
                    || catch_body
                        .as_ref()
                        .is_some_and(|body| block_assigns_variable(body, variable))
                    || finally_body
                        .as_ref()
                        .is_some_and(|body| block_assigns_variable(body, variable))
            }
            ControlFlow::Switch { cases, default, .. } => {
                cases
                    .iter()
                    .any(|(_, body)| block_assigns_variable(body, variable))
                    || default
                        .as_ref()
                        .is_some_and(|body| block_assigns_variable(body, variable))
            }
        },
        _ => false,
    }
}

fn block_assigns_variable(block: &IrBlock, variable: &str) -> bool {
    block
        .stmts
        .iter()
        .any(|statement| statement_assigns_variable(statement, variable))
}

pub(super) fn expression_has_effectful_call(expr: &Expr) -> bool {
    let mut found = false;
    visit_expr(expr, &mut |node| {
        if matches!(node, Expr::Call { target, .. } if !is_pure_call_target(target)) {
            found = true;
        }
    });
    found
}

/// Expressions that can throw even without a call. Keep their assignments in
/// place when the destination is otherwise dead so readability reduction does
/// not change observable VM failure behavior.
pub(super) fn expression_may_fault(expr: &Expr) -> bool {
    let mut may_fault = false;
    visit_expr(expr, &mut |node| {
        if may_fault {
            return;
        }
        match node {
            Expr::Index { .. }
            | Expr::Member { .. }
            | Expr::Convert { .. }
            | Expr::IsType { .. }
            | Expr::Unknown
            | Expr::StackTemp(_) => may_fault = true,
            Expr::Binary {
                op: BinOp::Div | BinOp::Mod | BinOp::Pow | BinOp::Shl | BinOp::Shr,
                ..
            }
            | Expr::Unary {
                op: UnaryOp::LogicalNot,
                ..
            } => may_fault = true,
            Expr::Call { .. } => may_fault = true,
            _ => {}
        }
    });
    may_fault
}

/// Pure expressions can be re-evaluated (or dropped) without observable
/// effects: no calls, no unrecovered stack values. A whitelist of read-only
/// VM intrinsics (length reads, slicing, math, allocation) still qualifies —
/// they render as ordinary C# property/operator/helper expressions.
pub(super) fn is_pure(expr: &Expr) -> bool {
    let mut pure = true;
    visit_expr(expr, &mut |node| {
        if matches!(node, Expr::Unknown | Expr::StackTemp(_)) {
            pure = false;
        }
        if let Expr::Call { target, .. } = node {
            pure &= is_pure_call_target(target);
        }
    });
    pure
}

pub(super) fn is_pure_call_target(target: &SemanticCallTarget) -> bool {
    let SemanticCallTarget::Intrinsic(intrinsic) = target else {
        return false;
    };
    let Intrinsic::Opcode(opcode) = intrinsic else {
        return false;
    };
    matches!(
        opcode,
        OpCode::Size
            | OpCode::Cat
            | OpCode::Substr
            | OpCode::Left
            | OpCode::Right
            | OpCode::Within
            | OpCode::Min
            | OpCode::Max
            | OpCode::Sqrt
            | OpCode::Modmul
            | OpCode::Modpow
            | OpCode::Nz
            | OpCode::Isnull
            | OpCode::Istype
            | OpCode::Keys
            | OpCode::Values
            | OpCode::Pickitem
            | OpCode::Haskey
            | OpCode::Newbuffer
            | OpCode::Newarray0
            | OpCode::Newarray
            | OpCode::NewarrayT
            | OpCode::Newstruct0
            | OpCode::Newstruct
            | OpCode::Newmap
    )
}

/// Collect the variables whose collection contents the expression reads:
/// bases of index/member reads and first arguments of collection-reading
/// intrinsics (concat, length, slicing, key lookup).
pub(super) fn collect_collection_bases(expr: &Expr, bases: &mut BTreeSet<String>) {
    visit_expr(expr, &mut |node| match node {
        Expr::Index { base, .. } | Expr::Member { base, .. } => {
            if let Expr::Variable(name) = base.as_ref() {
                bases.insert(name.clone());
            }
        }
        Expr::Call {
            target:
                SemanticCallTarget::Intrinsic(Intrinsic::Opcode(
                    OpCode::Size
                    | OpCode::Cat
                    | OpCode::Substr
                    | OpCode::Left
                    | OpCode::Right
                    | OpCode::Pickitem
                    | OpCode::Haskey
                    | OpCode::Keys
                    | OpCode::Values,
                )),
            args,
        } => {
            if let Some(Expr::Variable(name)) = args.first() {
                bases.insert(name.clone());
            }
        }
        _ => {}
    });
}

/// Whether the statement could mutate one of the given collections: an
/// item-writing intrinsic aimed at it, or an opaque internal/unresolved call
/// that receives it (arrays pass by reference in the VM).
pub(super) fn statement_mutates_collections(statement: &Stmt, bases: &BTreeSet<String>) -> bool {
    if bases.is_empty() {
        return false;
    }
    let mut found = false;
    visit_stmt_exprs(statement, &mut |expr| {
        if found {
            return;
        }
        let Expr::Call { target, args } = expr else {
            return;
        };
        match target {
            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(
                OpCode::Setitem
                | OpCode::Append
                | OpCode::Remove
                | OpCode::Clearitems
                | OpCode::Popitem
                | OpCode::Reverseitems
                | OpCode::Memcpy,
            )) => {
                if args
                    .first()
                    .is_some_and(|receiver| expr_mentions_any(receiver, bases))
                {
                    found = true;
                }
            }
            SemanticCallTarget::Internal { .. } | SemanticCallTarget::Unresolved { .. }
                if args.iter().any(|arg| expr_mentions_any(arg, bases)) =>
            {
                found = true
            }
            _ => {}
        }
    });
    found
}

pub(super) fn expr_mentions_any(expr: &Expr, names: &BTreeSet<String>) -> bool {
    let mut found = false;
    visit_expr(expr, &mut |node| {
        if matches!(node, Expr::Variable(name) if names.contains(name)) {
            found = true;
        }
    });
    found
}

/// Static slots (`static0`, `static1`, ...) are contract-wide memory: other
/// methods may observe them, so their stores are never dead and their values
/// are never propagated away.
pub(super) fn is_static_slot(name: &str) -> bool {
    name.strip_prefix("static").is_some_and(|suffix| {
        !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
    })
}
