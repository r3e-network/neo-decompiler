use crate::decompiler::ir::{Block as IrBlock, ControlFlow, Expr, Stmt};
use std::collections::BTreeSet;
pub(super) fn simplify_unreachable_control(block: &mut IrBlock) {
    loop {
        let before = block.clone();
        let mut referenced_labels = BTreeSet::new();
        collect_goto_labels(block, &mut referenced_labels);
        simplify_block(block, &referenced_labels);
        if *block == before {
            break;
        }
    }
}

fn collect_goto_labels(
    block: &IrBlock,
    referenced: &mut BTreeSet<crate::decompiler::ir::BlockLabel>,
) {
    for statement in &block.stmts {
        match statement {
            Stmt::Goto(label) => {
                referenced.insert(*label);
            }
            Stmt::ControlFlow(control) => match control.as_ref() {
                ControlFlow::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    collect_goto_labels(then_branch, referenced);
                    if let Some(else_branch) = else_branch {
                        collect_goto_labels(else_branch, referenced);
                    }
                }
                ControlFlow::While { body, .. } | ControlFlow::DoWhile { body, .. } => {
                    collect_goto_labels(body, referenced);
                }
                ControlFlow::For { init, body, .. } => {
                    if let Some(init) = init {
                        collect_statement_goto_labels(init, referenced);
                    }
                    collect_goto_labels(body, referenced);
                }
                ControlFlow::TryCatch {
                    try_body,
                    catch_body,
                    finally_body,
                    ..
                } => {
                    collect_goto_labels(try_body, referenced);
                    if let Some(catch_body) = catch_body {
                        collect_goto_labels(catch_body, referenced);
                    }
                    if let Some(finally_body) = finally_body {
                        collect_goto_labels(finally_body, referenced);
                    }
                }
                ControlFlow::Switch { cases, default, .. } => {
                    for (_, body) in cases {
                        collect_goto_labels(body, referenced);
                    }
                    if let Some(default) = default {
                        collect_goto_labels(default, referenced);
                    }
                }
            },
            _ => {}
        }
    }
}

fn collect_statement_goto_labels(
    statement: &Stmt,
    referenced: &mut BTreeSet<crate::decompiler::ir::BlockLabel>,
) {
    match statement {
        Stmt::Goto(label) => {
            referenced.insert(*label);
        }
        Stmt::ControlFlow(control) => {
            let mut wrapper = IrBlock::new();
            wrapper.push(Stmt::ControlFlow(control.clone()));
            collect_goto_labels(&wrapper, referenced);
        }
        _ => {}
    }
}

fn simplify_block(
    block: &mut IrBlock,
    referenced_labels: &BTreeSet<crate::decompiler::ir::BlockLabel>,
) {
    for statement in &mut block.stmts {
        let Stmt::ControlFlow(control) = statement else {
            continue;
        };
        if matches!(
            control.as_ref(),
            ControlFlow::While {
                condition: Expr::Literal(crate::decompiler::ir::Literal::Bool(false)),
                body,
            } if body.stmts.as_slice() == [Stmt::Continue]
        ) {
            **control = ControlFlow::do_while(
                IrBlock::with_stmts(vec![Stmt::Continue]),
                Expr::Literal(crate::decompiler::ir::Literal::Bool(false)),
            );
        }
        match control.as_mut() {
            ControlFlow::If {
                then_branch,
                else_branch,
                ..
            } => {
                simplify_block(then_branch, referenced_labels);
                if let Some(else_branch) = else_branch {
                    simplify_block(else_branch, referenced_labels);
                }
            }
            ControlFlow::While { body, .. } | ControlFlow::DoWhile { body, .. } => {
                simplify_block(body, referenced_labels);
            }
            ControlFlow::For { init, body, .. } => {
                if let Some(init) = init {
                    simplify_statement(init, referenced_labels);
                }
                simplify_block(body, referenced_labels);
            }
            ControlFlow::TryCatch {
                try_body,
                catch_body,
                finally_body,
                ..
            } => {
                simplify_block(try_body, referenced_labels);
                if let Some(catch_body) = catch_body {
                    simplify_block(catch_body, referenced_labels);
                }
                if let Some(finally_body) = finally_body {
                    simplify_block(finally_body, referenced_labels);
                }
            }
            ControlFlow::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    simplify_block(body, referenced_labels);
                }
                if let Some(default) = default {
                    simplify_block(default, referenced_labels);
                }
            }
        }
    }

    let mut reachable = true;
    block.stmts.retain(|statement| {
        if let Stmt::Label(label) = statement {
            if referenced_labels.contains(label) {
                reachable = true;
                return true;
            }
            return false;
        }
        if !reachable {
            return false;
        }
        if statement_always_terminates(statement) {
            reachable = false;
        }
        true
    });
}

fn simplify_statement(
    statement: &mut Stmt,
    referenced_labels: &BTreeSet<crate::decompiler::ir::BlockLabel>,
) {
    if let Stmt::ControlFlow(control) = statement {
        let mut wrapper = IrBlock::new();
        wrapper.push(Stmt::ControlFlow(control.clone()));
        simplify_block(&mut wrapper, referenced_labels);
        if let Some(Stmt::ControlFlow(simplified)) = wrapper.stmts.pop() {
            *control = simplified;
        }
    }
}

fn block_always_terminates(block: &IrBlock) -> bool {
    let mut terminates = false;
    for statement in &block.stmts {
        if matches!(statement, Stmt::Label(_)) {
            terminates = false;
        } else if !terminates && statement_always_terminates(statement) {
            terminates = true;
        }
    }
    terminates
}

fn statement_always_terminates(statement: &Stmt) -> bool {
    match statement {
        Stmt::Return(_)
        | Stmt::Throw(_)
        | Stmt::Abort(_)
        | Stmt::Break
        | Stmt::Continue
        | Stmt::Goto(_) => true,
        Stmt::ControlFlow(control) => match control.as_ref() {
            ControlFlow::If {
                then_branch,
                else_branch: Some(else_branch),
                ..
            } => block_always_terminates(then_branch) && block_always_terminates(else_branch),
            ControlFlow::TryCatch {
                try_body,
                catch_body,
                finally_body,
                ..
            } => {
                finally_body.as_ref().is_some_and(block_always_terminates)
                    || (block_always_terminates(try_body)
                        && catch_body.as_ref().is_none_or(block_always_terminates))
            }
            ControlFlow::Switch { cases, default, .. } => {
                default.as_ref().is_some_and(block_always_terminates)
                    && cases.iter().all(|(_, body)| block_always_terminates(body))
            }
            _ => false,
        },
        Stmt::Assign { .. }
        | Stmt::Assert { .. }
        | Stmt::ExprStmt(_)
        | Stmt::Comment(_)
        | Stmt::Label(_) => false,
    }
}
