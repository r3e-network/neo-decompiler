use crate::decompiler::ir::{Block as IrBlock, ControlFlow, Expr, Stmt, UnaryOp};

use super::{arithmetic_update_shape, symbol_base, LoopUpdateShape};

/// Recover a compiler-shaped scan loop whose induction update is the
/// non-terminal arm of the final body `if`.
pub(super) fn terminal_update_shape(body: &IrBlock, condition: &Expr) -> Option<LoopUpdateShape> {
    let index = body.stmts.len().checked_sub(1)?;
    let Stmt::ControlFlow(control) = &body.stmts[index] else {
        return None;
    };
    let ControlFlow::If {
        then_branch,
        else_branch: Some(else_branch),
        ..
    } = control.as_ref()
    else {
        return None;
    };
    if body.stmts[..index].iter().any(contains_continue)
        || then_branch.stmts.iter().any(contains_continue)
        || else_branch.stmts.iter().any(contains_continue)
    {
        return None;
    }

    let then_terminates = strictly_terminates(then_branch);
    let else_terminates = strictly_terminates(else_branch);
    if then_terminates == else_terminates {
        return None;
    }
    let (terminal_in_then, update_branch) = if then_terminates {
        (true, else_branch)
    } else {
        (false, then_branch)
    };
    let (update, variable) = block_update_shape(update_branch)?;
    super::contains_variable(condition, &variable).then_some(LoopUpdateShape::TerminalIf {
        update,
        variable,
        index,
        terminal_in_then,
    })
}

pub(super) fn rewrite_terminal_update(body: &mut IrBlock, index: usize, terminal_in_then: bool) {
    let statement = body.stmts.remove(index);
    let Stmt::ControlFlow(control) = statement else {
        unreachable!("terminal update shape must point to an if statement");
    };
    let ControlFlow::If {
        condition,
        then_branch,
        else_branch: Some(else_branch),
    } = *control
    else {
        unreachable!("terminal update shape must point to a two-arm if statement");
    };
    let (terminal_condition, terminal_branch) = if terminal_in_then {
        (condition, then_branch)
    } else {
        (Expr::unary(UnaryOp::LogicalNot, condition), else_branch)
    };
    body.stmts.insert(
        index,
        Stmt::ControlFlow(Box::new(ControlFlow::if_then(
            terminal_condition,
            terminal_branch,
        ))),
    );
}

fn strictly_terminates(block: &IrBlock) -> bool {
    matches!(
        block.stmts.last(),
        Some(Stmt::Return(_) | Stmt::Throw(_) | Stmt::Abort(_))
    )
}

fn contains_continue(statement: &Stmt) -> bool {
    match statement {
        Stmt::Continue => true,
        Stmt::ControlFlow(control) => match control.as_ref() {
            ControlFlow::If {
                then_branch,
                else_branch,
                ..
            } => {
                then_branch.stmts.iter().any(contains_continue)
                    || else_branch
                        .as_ref()
                        .is_some_and(|branch| branch.stmts.iter().any(contains_continue))
            }
            ControlFlow::TryCatch {
                try_body,
                catch_body,
                finally_body,
                ..
            } => {
                try_body.stmts.iter().any(contains_continue)
                    || catch_body
                        .as_ref()
                        .is_some_and(|branch| branch.stmts.iter().any(contains_continue))
                    || finally_body
                        .as_ref()
                        .is_some_and(|branch| branch.stmts.iter().any(contains_continue))
            }
            ControlFlow::Switch { cases, default, .. } => {
                cases
                    .iter()
                    .any(|(_, branch)| branch.stmts.iter().any(contains_continue))
                    || default
                        .as_ref()
                        .is_some_and(|branch| branch.stmts.iter().any(contains_continue))
            }
            ControlFlow::While { body, .. }
            | ControlFlow::DoWhile { body, .. }
            | ControlFlow::For { body, .. } => body.stmts.iter().any(contains_continue),
        },
        _ => false,
    }
}

fn block_update_shape(block: &IrBlock) -> Option<(Expr, String)> {
    if let [statement] = block.stmts.as_slice() {
        if let Some(update) = arithmetic_update_shape(statement) {
            return Some(update);
        }
        if let Stmt::ExprStmt(
            update @ Expr::Unary {
                op: UnaryOp::Inc | UnaryOp::Dec,
                operand,
            },
        ) = statement
        {
            if let Expr::Variable(variable) = operand.as_ref() {
                return Some((update.clone(), variable.clone()));
            }
        }
        if let Stmt::Assign {
            target,
            value:
                Expr::Unary {
                    op: update_op,
                    operand,
                },
        } = statement
        {
            if matches!(update_op, UnaryOp::Inc | UnaryOp::Dec) {
                if let Expr::Variable(variable) = operand.as_ref() {
                    if symbol_base(target) == symbol_base(variable) {
                        return Some((
                            Expr::unary(*update_op, Expr::var(variable.clone())),
                            variable.clone(),
                        ));
                    }
                }
            }
        }
        return None;
    }

    let [Stmt::Assign {
        target: temporary,
        value: Expr::Unary {
            op: update_op,
            operand,
        },
    }, Stmt::Assign {
        target: copied_target,
        value: Expr::Variable(copied_value),
    }] = block.stmts.as_slice()
    else {
        return None;
    };
    if !matches!(update_op, UnaryOp::Inc | UnaryOp::Dec) || copied_value != temporary {
        return None;
    }
    let Expr::Variable(variable) = operand.as_ref() else {
        return None;
    };
    (symbol_base(copied_target) == symbol_base(variable)).then_some((
        Expr::unary(*update_op, Expr::var(variable.clone())),
        variable.clone(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decompiler::ir::BinOp;

    #[test]
    fn terminal_update_requires_a_single_induction_assignment() {
        let induction = Expr::var("index_0");
        let condition = Expr::binary(BinOp::Lt, induction.clone(), Expr::int(3));
        let terminal = IrBlock::with_stmts(vec![Stmt::ret(Expr::int(1))]);
        let update = IrBlock::with_stmts(vec![Stmt::assign(
            "index_1",
            Expr::binary(BinOp::Add, induction.clone(), Expr::int(1)),
        )]);
        let body = IrBlock::with_stmts(vec![Stmt::ControlFlow(Box::new(ControlFlow::if_else(
            Expr::var("predicate"),
            terminal,
            update,
        )))]);

        let shape = terminal_update_shape(&body, &condition).expect("terminal scan update");
        assert!(matches!(
            shape,
            LoopUpdateShape::TerminalIf {
                update: Expr::Unary {
                    op: UnaryOp::Inc,
                    ..
                },
                terminal_in_then: true,
                ..
            }
        ));

        let mut rewritten = body;
        rewrite_terminal_update(&mut rewritten, 0, true);
        assert!(matches!(
            rewritten.stmts.as_slice(),
            [Stmt::ControlFlow(control)]
                if matches!(control.as_ref(), ControlFlow::If {
                    else_branch: None,
                    ..
                })
        ));
    }

    #[test]
    fn terminal_update_rejects_extra_effects_and_continue() {
        let induction = Expr::var("index_0");
        let condition = Expr::binary(BinOp::Lt, induction.clone(), Expr::int(3));
        let terminal = IrBlock::with_stmts(vec![Stmt::ret(Expr::int(1))]);
        let update_with_effect = IrBlock::with_stmts(vec![
            Stmt::assign(
                "index_1",
                Expr::binary(BinOp::Add, induction.clone(), Expr::int(1)),
            ),
            Stmt::expr(Expr::unresolved_call("observe", vec![])),
        ]);
        let body = IrBlock::with_stmts(vec![Stmt::ControlFlow(Box::new(ControlFlow::if_else(
            Expr::var("predicate"),
            terminal,
            update_with_effect,
        )))]);
        assert!(terminal_update_shape(&body, &condition).is_none());

        let continue_branch = IrBlock::with_stmts(vec![Stmt::Continue]);
        let update = IrBlock::with_stmts(vec![Stmt::assign(
            "index_1",
            Expr::binary(BinOp::Add, induction, Expr::int(1)),
        )]);
        let body = IrBlock::with_stmts(vec![Stmt::ControlFlow(Box::new(ControlFlow::if_else(
            Expr::var("predicate"),
            continue_branch,
            update,
        )))]);
        assert!(terminal_update_shape(&body, &condition).is_none());

        let terminal = IrBlock::with_stmts(vec![Stmt::ret(Expr::int(1))]);
        let update = IrBlock::with_stmts(vec![Stmt::assign(
            "index_1",
            Expr::binary(BinOp::Add, Expr::var("index_0"), Expr::int(1)),
        )]);
        let body = IrBlock::with_stmts(vec![
            Stmt::Continue,
            Stmt::ControlFlow(Box::new(ControlFlow::if_else(
                Expr::var("predicate"),
                terminal,
                update,
            ))),
        ]);
        assert!(terminal_update_shape(&body, &condition).is_none());
    }
}
