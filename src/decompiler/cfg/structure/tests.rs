use super::*;
use crate::decompiler::cfg::ssa::DominanceInfo;
use crate::decompiler::cfg::ssa::SsaForm;
use crate::decompiler::cfg::ssa::{PhiNode, SsaBlock, SsaExpr, SsaStmt, SsaVariable};
use crate::decompiler::cfg::{BasicBlock, BlockId, Cfg, EdgeKind, Terminator};
use crate::decompiler::ir::{BinOp, Literal, Stmt};

fn v(base: &str, n: usize) -> SsaVariable {
    SsaVariable::new(base.to_string(), n)
}

/// Build a diamond: BB0 branches to BB1/BB2, both jump to BB3 (merge/ret).
fn diamond_cfg() -> Cfg {
    let mut cfg = Cfg::new();
    cfg.add_block(BasicBlock::new(
        BlockId(0),
        0,
        1,
        0..1,
        Terminator::Branch {
            then_target: BlockId(1),
            else_target: BlockId(2),
        },
    ));
    cfg.add_block(BasicBlock::new(
        BlockId(1),
        1,
        2,
        1..2,
        Terminator::Jump { target: BlockId(3) },
    ));
    cfg.add_block(BasicBlock::new(
        BlockId(2),
        2,
        3,
        2..3,
        Terminator::Jump { target: BlockId(3) },
    ));
    cfg.add_block(BasicBlock::new(BlockId(3), 3, 4, 3..4, Terminator::Return));
    cfg.add_edge(BlockId(0), BlockId(1), EdgeKind::ConditionalTrue);
    cfg.add_edge(BlockId(0), BlockId(2), EdgeKind::ConditionalFalse);
    cfg.add_edge(BlockId(1), BlockId(3), EdgeKind::Unconditional);
    cfg.add_edge(BlockId(2), BlockId(3), EdgeKind::Unconditional);
    cfg
}

fn block_with(stmts: Vec<SsaStmt>) -> SsaBlock {
    let mut b = SsaBlock::new();
    for s in stmts {
        b.add_stmt(s);
    }
    b
}

fn phi(target: SsaVariable, operands: &[(BlockId, SsaVariable)]) -> PhiNode {
    let mut phi = PhiNode::new(target);
    for (predecessor, operand) in operands {
        phi.add_operand(*predecessor, operand.clone());
    }
    phi
}

fn block_contains_call(block: &IrBlock, expected: &str) -> bool {
    block
        .stmts
        .iter()
        .any(|stmt| stmt_contains_call(stmt, expected))
}

fn stmt_contains_call(stmt: &Stmt, expected: &str) -> bool {
    match stmt {
        Stmt::Assign { value, .. } | Stmt::ExprStmt(value) => expr_contains_call(value, expected),
        Stmt::Return(value) => value
            .as_ref()
            .is_some_and(|value| expr_contains_call(value, expected)),
        Stmt::Throw(value) | Stmt::Abort(value) => value
            .as_ref()
            .is_some_and(|value| expr_contains_call(value, expected)),
        Stmt::Assert { condition, message } => {
            expr_contains_call(condition, expected)
                || message
                    .as_ref()
                    .is_some_and(|message| expr_contains_call(message, expected))
        }
        Stmt::ControlFlow(control_flow) => control_flow_contains_call(control_flow, expected),
        Stmt::Comment(_) | Stmt::Break | Stmt::Continue | Stmt::Label(_) | Stmt::Goto(_) => false,
    }
}

fn control_flow_contains_call(control_flow: &ControlFlow, expected: &str) -> bool {
    match control_flow {
        ControlFlow::If {
            condition,
            then_branch,
            else_branch,
        } => {
            expr_contains_call(condition, expected)
                || block_contains_call(then_branch, expected)
                || else_branch
                    .as_ref()
                    .is_some_and(|branch| block_contains_call(branch, expected))
        }
        ControlFlow::While { condition, body } | ControlFlow::DoWhile { body, condition } => {
            expr_contains_call(condition, expected) || block_contains_call(body, expected)
        }
        ControlFlow::For {
            init,
            condition,
            update,
            body,
        } => {
            init.as_ref()
                .is_some_and(|stmt| stmt_contains_call(stmt, expected))
                || condition
                    .as_ref()
                    .is_some_and(|expr| expr_contains_call(expr, expected))
                || update
                    .as_ref()
                    .is_some_and(|expr| expr_contains_call(expr, expected))
                || block_contains_call(body, expected)
        }
        ControlFlow::TryCatch {
            try_body,
            catch_body,
            finally_body,
            ..
        } => {
            block_contains_call(try_body, expected)
                || catch_body
                    .as_ref()
                    .is_some_and(|body| block_contains_call(body, expected))
                || finally_body
                    .as_ref()
                    .is_some_and(|body| block_contains_call(body, expected))
        }
        ControlFlow::Switch {
            expr,
            cases,
            default,
        } => {
            expr_contains_call(expr, expected)
                || cases.iter().any(|(value, body)| {
                    expr_contains_call(value, expected) || block_contains_call(body, expected)
                })
                || default
                    .as_ref()
                    .is_some_and(|body| block_contains_call(body, expected))
        }
    }
}

fn expr_contains_call(expr: &Expr, expected: &str) -> bool {
    match expr {
        Expr::Call { target, args } => {
            target.display_name() == expected
                || args
                    .iter()
                    .any(|argument| expr_contains_call(argument, expected))
        }
        Expr::Binary { left, right, .. } => {
            expr_contains_call(left, expected) || expr_contains_call(right, expected)
        }
        Expr::Unary { operand, .. } => expr_contains_call(operand, expected),
        Expr::Index { base, index } => {
            expr_contains_call(base, expected) || expr_contains_call(index, expected)
        }
        Expr::Member { base, .. } => expr_contains_call(base, expected),
        Expr::Cast { expr, .. } => expr_contains_call(expr, expected),
        Expr::Convert { value, .. } | Expr::IsType { value, .. } => {
            expr_contains_call(value, expected)
        }
        Expr::NewArray { length, .. } => expr_contains_call(length, expected),
        Expr::Array(elements) | Expr::Struct(elements) => elements
            .iter()
            .any(|element| expr_contains_call(element, expected)),
        Expr::Map(pairs) => pairs.iter().any(|(key, value)| {
            expr_contains_call(key, expected) || expr_contains_call(value, expected)
        }),
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_contains_call(condition, expected)
                || expr_contains_call(then_expr, expected)
                || expr_contains_call(else_expr, expected)
        }
        Expr::Unknown | Expr::Literal(_) | Expr::Variable(_) | Expr::StackTemp(_) => false,
    }
}

fn collect_transfers<'a>(block: &'a IrBlock, transfers: &mut Vec<&'a Stmt>) {
    for statement in &block.stmts {
        match statement {
            Stmt::Break | Stmt::Continue | Stmt::Label(_) | Stmt::Goto(_) => {
                transfers.push(statement);
            }
            Stmt::ControlFlow(control) => match control.as_ref() {
                ControlFlow::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    collect_transfers(then_branch, transfers);
                    if let Some(else_branch) = else_branch {
                        collect_transfers(else_branch, transfers);
                    }
                }
                ControlFlow::While { body, .. } | ControlFlow::DoWhile { body, .. } => {
                    collect_transfers(body, transfers);
                }
                ControlFlow::For { init, body, .. } => {
                    if matches!(
                        init.as_deref(),
                        Some(Stmt::Break | Stmt::Continue | Stmt::Label(_) | Stmt::Goto(_))
                    ) {
                        transfers.push(init.as_deref().expect("matched initializer"));
                    }
                    collect_transfers(body, transfers);
                }
                ControlFlow::TryCatch {
                    try_body,
                    catch_body,
                    finally_body,
                    ..
                } => {
                    collect_transfers(try_body, transfers);
                    if let Some(catch_body) = catch_body {
                        collect_transfers(catch_body, transfers);
                    }
                    if let Some(finally_body) = finally_body {
                        collect_transfers(finally_body, transfers);
                    }
                }
                ControlFlow::Switch { cases, default, .. } => {
                    for (_, body) in cases {
                        collect_transfers(body, transfers);
                    }
                    if let Some(default) = default {
                        collect_transfers(default, transfers);
                    }
                }
            },
            _ => {}
        }
    }
}

fn entry_self_loop_structure() -> IrBlock {
    const VIRTUAL_ENTRY: BlockId = BlockId(usize::MAX);

    let entry = BlockId(0);
    let mut cfg = Cfg::new();
    cfg.add_block(BasicBlock::new(
        entry,
        0,
        1,
        0..1,
        Terminator::Jump { target: entry },
    ));
    cfg.add_edge(entry, entry, EdgeKind::Unconditional);

    let state = v("state", 1);
    let initial = v("initial", 0);
    let next = v("next", 0);
    let mut block = block_with(vec![
        SsaStmt::assign(next.clone(), SsaExpr::lit(Literal::Int(2))),
        SsaStmt::expr(SsaExpr::unresolved_call(
            "check".to_string(),
            vec![SsaExpr::var(state.clone())],
        )),
    ]);
    block.add_phi(phi(
        state.clone(),
        &[(VIRTUAL_ENTRY, initial.clone()), (entry, next.clone())],
    ));
    let ssa = SsaForm {
        dominance: crate::decompiler::cfg::ssa::compute(&cfg),
        cfg,
        blocks: BTreeMap::from([(entry, block)]),
        definitions: BTreeMap::new(),
        uses: BTreeMap::from([(state, BTreeSet::from([UseSite::new(entry, 1)]))]),
    };

    structure(&ssa)
}

#[path = "tests_branches_loops.rs"]
mod branches_loops;
#[path = "tests_do_while_switch.rs"]
mod do_while_switch;
#[path = "tests_entry_phi.rs"]
mod entry_phi;
#[path = "tests_irreducible_phi.rs"]
mod irreducible_phi;
#[path = "tests_try_regions.rs"]
mod try_regions;
