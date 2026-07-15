//! Recovery of compiler-generated counting loops.

use crate::decompiler::ir::{BinOp, Block as IrBlock, ControlFlow, Expr, Literal, Stmt, UnaryOp};
use std::collections::BTreeSet;

/// Recover `x = init; while (cond(x)) { … update x … }` from the degenerate
/// structured shape produced when a back-edge re-enters the initializer:
/// `while (true) { x = init; if (cond(x)) { … } }`.
pub(super) fn recover_header_init_loops(block: &mut IrBlock) {
    rewrite_control_flow_children(block, recover_header_init_loops);

    let mut rewritten = Vec::with_capacity(block.stmts.len());
    for statement in std::mem::take(&mut block.stmts) {
        if let Some((inits, loop_stmt)) = try_lift_header_init_while(&statement) {
            rewritten.extend(inits);
            rewritten.push(loop_stmt);
        } else {
            rewritten.push(statement);
        }
    }
    block.stmts = rewritten;
}

fn try_lift_header_init_while(statement: &Stmt) -> Option<(Vec<Stmt>, Stmt)> {
    let Stmt::ControlFlow(control) = statement else {
        return None;
    };
    let ControlFlow::While { condition, body } = control.as_ref() else {
        return None;
    };
    if !matches!(condition, Expr::Literal(Literal::Bool(true))) {
        return None;
    }

    let mut init_count = 0usize;
    let mut init_bases = BTreeSet::new();
    for stmt in &body.stmts {
        match stmt {
            Stmt::Assign { target, value } if is_constant_initializer(value) => {
                init_bases.insert(symbol_base(target).to_string());
                init_count += 1;
            }
            _ => break,
        }
    }
    if init_count == 0 || init_count >= body.stmts.len() {
        return None;
    }

    let rest = &body.stmts[init_count..];
    let [Stmt::ControlFlow(inner)] = rest else {
        return None;
    };
    let ControlFlow::If {
        condition: if_cond,
        then_branch,
        else_branch,
    } = inner.as_ref()
    else {
        return None;
    };
    if else_branch
        .as_ref()
        .is_some_and(|branch| !branch.stmts.is_empty())
    {
        return None;
    }
    if !init_bases
        .iter()
        .any(|base| expr_mentions_base(if_cond, base))
    {
        return None;
    }
    // Require a loop-carried write so we only lift true counting/update loops,
    // not one-shot side effects inside an intentional infinite loop.
    if !init_bases
        .iter()
        .any(|base| block_assigns_base(then_branch, base))
    {
        return None;
    }

    let inits = body.stmts[..init_count].to_vec();
    let loop_stmt = Stmt::ControlFlow(Box::new(ControlFlow::while_loop(
        if_cond.clone(),
        then_branch.clone(),
    )));
    Some((inits, loop_stmt))
}

/// Promote `i = 0; while (cond(i)) { …; i = i ± 1; }` into a `for` when the
/// update is an unambiguous unit step on the induction variable.
pub(super) fn promote_adjacent_for_loops(block: &mut IrBlock) {
    rewrite_control_flow_children(block, promote_adjacent_for_loops);

    let mut index = 0;
    while index < block.stmts.len() {
        if index > 0 {
            if let Some(promoted) = try_promote_while_at(&block.stmts, index) {
                block.stmts[index - 1] = promoted;
                block.stmts.remove(index);
                continue;
            }
        }
        index += 1;
    }
}

fn try_promote_while_at(stmts: &[Stmt], while_index: usize) -> Option<Stmt> {
    if while_index == 0 {
        return None;
    }
    let Stmt::Assign {
        target: init_target,
        value: init_value,
    } = &stmts[while_index - 1]
    else {
        return None;
    };
    if !is_constant_initializer(init_value) {
        return None;
    }
    let init_base = symbol_base(init_target);

    let Stmt::ControlFlow(control) = &stmts[while_index] else {
        return None;
    };
    let ControlFlow::While { condition, body } = control.as_ref() else {
        return None;
    };
    if !expr_mentions_base(condition, init_base) {
        return None;
    }
    let (update, body_without_update) = peel_unit_update(body, init_base)?;
    let init = stmts[while_index - 1].clone();
    Some(Stmt::ControlFlow(Box::new(ControlFlow::for_loop(
        Some(init),
        Some(condition.clone()),
        Some(update),
        body_without_update,
    ))))
}

fn peel_unit_update(body: &IrBlock, induction_base: &str) -> Option<(Expr, IrBlock)> {
    if body.stmts.is_empty() {
        return None;
    }

    // `i = i ± 1` or `i++` / `i--`
    if let Some((update, variable)) = unit_update_shape(body.stmts.last()?) {
        if symbol_base(&variable) == induction_base {
            let mut trimmed = body.clone();
            trimmed.stmts.pop();
            return Some((update, trimmed));
        }
    }

    // Compiler copy-chain: `t = i ± 1; i = t`
    if body.stmts.len() < 2 {
        return None;
    }
    let temp_assign = &body.stmts[body.stmts.len() - 2];
    let copy_assign = body.stmts.last()?;
    let (
        Stmt::Assign {
            target: temporary,
            value: temp_value,
        },
        Stmt::Assign {
            target: copied_target,
            value: Expr::Variable(copied_from),
        },
    ) = (temp_assign, copy_assign)
    else {
        return None;
    };
    if copied_from != temporary || symbol_base(copied_target) != induction_base {
        return None;
    }

    let update = match temp_value {
        Expr::Binary {
            op: BinOp::Add,
            left,
            right,
        } => {
            let mentions_induction = matches!(
                (left.as_ref(), right.as_ref()),
                (Expr::Variable(v), s) | (s, Expr::Variable(v))
                    if symbol_base(v) == induction_base && is_one_literal(s)
            );
            if !mentions_induction {
                return None;
            }
            Expr::unary(UnaryOp::Inc, Expr::var(copied_target.clone()))
        }
        Expr::Binary {
            op: BinOp::Sub,
            left,
            right,
        } => {
            if !matches!(
                (left.as_ref(), right.as_ref()),
                (Expr::Variable(v), s)
                    if symbol_base(v) == induction_base && is_one_literal(s)
            ) {
                return None;
            }
            Expr::unary(UnaryOp::Dec, Expr::var(copied_target.clone()))
        }
        _ => return None,
    };

    let mut trimmed = body.clone();
    trimmed.stmts.truncate(body.stmts.len() - 2);
    Some((update, trimmed))
}

fn unit_update_shape(statement: &Stmt) -> Option<(Expr, String)> {
    match statement {
        Stmt::ExprStmt(
            update @ Expr::Unary {
                op: UnaryOp::Inc | UnaryOp::Dec,
                operand,
            },
        ) => {
            if let Expr::Variable(variable) = operand.as_ref() {
                return Some((update.clone(), variable.clone()));
            }
            None
        }
        Stmt::Assign {
            target,
            value:
                Expr::Unary {
                    op: update_op @ (UnaryOp::Inc | UnaryOp::Dec),
                    operand,
                },
        } => {
            if let Expr::Variable(variable) = operand.as_ref() {
                if symbol_base(target) == symbol_base(variable) {
                    return Some((
                        Expr::unary(*update_op, Expr::var(variable.clone())),
                        variable.clone(),
                    ));
                }
            }
            None
        }
        Stmt::Assign {
            target,
            value:
                Expr::Binary {
                    op: BinOp::Add,
                    left,
                    right,
                },
        } => match (left.as_ref(), right.as_ref()) {
            (Expr::Variable(variable), step) | (step, Expr::Variable(variable))
                if is_one_literal(step) && symbol_base(target) == symbol_base(variable) =>
            {
                Some((
                    Expr::unary(UnaryOp::Inc, Expr::var(variable.clone())),
                    variable.clone(),
                ))
            }
            _ => None,
        },
        Stmt::Assign {
            target,
            value:
                Expr::Binary {
                    op: BinOp::Sub,
                    left,
                    right,
                },
        } => match (left.as_ref(), right.as_ref()) {
            (Expr::Variable(variable), step)
                if is_one_literal(step) && symbol_base(target) == symbol_base(variable) =>
            {
                Some((
                    Expr::unary(UnaryOp::Dec, Expr::var(variable.clone())),
                    variable.clone(),
                ))
            }
            _ => None,
        },
        _ => None,
    }
}

fn rewrite_control_flow_children(block: &mut IrBlock, rewrite: fn(&mut IrBlock)) {
    for statement in &mut block.stmts {
        let Stmt::ControlFlow(control) = statement else {
            continue;
        };
        match control.as_mut() {
            ControlFlow::If {
                then_branch,
                else_branch,
                ..
            } => {
                rewrite(then_branch);
                if let Some(else_branch) = else_branch {
                    rewrite(else_branch);
                }
            }
            ControlFlow::While { body, .. } | ControlFlow::DoWhile { body, .. } => {
                rewrite(body);
            }
            ControlFlow::For { body, .. } => {
                rewrite(body);
            }
            ControlFlow::TryCatch {
                try_body,
                catch_body,
                finally_body,
                ..
            } => {
                rewrite(try_body);
                if let Some(catch_body) = catch_body {
                    rewrite(catch_body);
                }
                if let Some(finally_body) = finally_body {
                    rewrite(finally_body);
                }
            }
            ControlFlow::Switch { cases, default, .. } => {
                for (_, case_body) in cases {
                    rewrite(case_body);
                }
                if let Some(default) = default {
                    rewrite(default);
                }
            }
        }
    }
}

fn is_constant_initializer(expression: &Expr) -> bool {
    matches!(
        expression,
        Expr::Literal(
            Literal::Int(_)
                | Literal::BigInt(_)
                | Literal::Bool(_)
                | Literal::Null
                | Literal::String(_)
                | Literal::Bytes(_)
        )
    )
}

fn is_one_literal(expression: &Expr) -> bool {
    match expression {
        Expr::Literal(Literal::Int(value)) => *value == 1,
        Expr::Literal(Literal::BigInt(value)) => value == "1",
        _ => false,
    }
}

fn symbol_base(name: &str) -> &str {
    name.rsplit_once('_')
        .filter(|(_, suffix)| {
            !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
        })
        .map_or(name, |(base, _)| base)
}

fn expr_mentions_base(expression: &Expr, base: &str) -> bool {
    match expression {
        Expr::Variable(name) => symbol_base(name) == base,
        Expr::Binary { left, right, .. } => {
            expr_mentions_base(left, base) || expr_mentions_base(right, base)
        }
        Expr::Unary { operand, .. }
        | Expr::Convert { value: operand, .. }
        | Expr::IsType { value: operand, .. }
        | Expr::Cast { expr: operand, .. } => expr_mentions_base(operand, base),
        Expr::Call { args, .. } | Expr::Array(args) | Expr::Struct(args) => {
            args.iter().any(|arg| expr_mentions_base(arg, base))
        }
        Expr::Index {
            base: container,
            index,
        } => expr_mentions_base(container, base) || expr_mentions_base(index, base),
        Expr::Member {
            base: container, ..
        } => expr_mentions_base(container, base),
        Expr::NewArray { length, .. } => expr_mentions_base(length, base),
        Expr::Map(entries) => entries
            .iter()
            .any(|(key, value)| expr_mentions_base(key, base) || expr_mentions_base(value, base)),
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_mentions_base(condition, base)
                || expr_mentions_base(then_expr, base)
                || expr_mentions_base(else_expr, base)
        }
        Expr::Unknown | Expr::Literal(_) | Expr::StackTemp(_) => false,
    }
}

fn block_assigns_base(block: &IrBlock, base: &str) -> bool {
    block.stmts.iter().any(|statement| match statement {
        Stmt::Assign { target, .. } => symbol_base(target) == base,
        Stmt::ControlFlow(control) => match control.as_ref() {
            ControlFlow::If {
                then_branch,
                else_branch,
                ..
            } => {
                block_assigns_base(then_branch, base)
                    || else_branch
                        .as_ref()
                        .is_some_and(|branch| block_assigns_base(branch, base))
            }
            ControlFlow::While { body, .. } | ControlFlow::DoWhile { body, .. } => {
                block_assigns_base(body, base)
            }
            ControlFlow::For { init, body, .. } => {
                init.as_ref().is_some_and(|init| match init.as_ref() {
                    Stmt::Assign { target, .. } => symbol_base(target) == base,
                    _ => false,
                }) || block_assigns_base(body, base)
            }
            ControlFlow::TryCatch {
                try_body,
                catch_body,
                finally_body,
                ..
            } => {
                block_assigns_base(try_body, base)
                    || catch_body
                        .as_ref()
                        .is_some_and(|body| block_assigns_base(body, base))
                    || finally_body
                        .as_ref()
                        .is_some_and(|body| block_assigns_base(body, base))
            }
            ControlFlow::Switch { cases, default, .. } => {
                cases.iter().any(|(_, body)| block_assigns_base(body, base))
                    || default
                        .as_ref()
                        .is_some_and(|body| block_assigns_base(body, base))
            }
        },
        _ => false,
    })
}
