use std::collections::{BTreeSet, HashSet};

use crate::decompiler::cfg::{BlockId, Terminator};
use crate::decompiler::ir::{Block as IrBlock, ControlFlow, Expr, Literal, Stmt, UnaryOp};

use super::StructCtx;

impl<'a> StructCtx<'a> {
    /// Emit a loop for a loop-header branch, promoting only an unambiguous
    /// source-level induction shape and retaining `while` otherwise.
    pub(super) fn build_loop(
        &self,
        out: &mut IrBlock,
        bid: BlockId,
        cond: Expr,
        body: &mut IrBlock,
    ) {
        // Backedge phi copies are already at the body tail. Replay the header
        // after them so its effects run before the next condition check.
        self.emit_body_except_condition(body, bid);
        if self.try_promote_for(out, cond.clone(), body) {
            return;
        }
        out.push(Stmt::ControlFlow(Box::new(ControlFlow::while_loop(
            cond,
            std::mem::take(body),
        ))));
    }

    /// Promote `i = init; while (cond(i)) { ...; i++; }` only when the update
    /// is an explicit unary increment/decrement. Versioned assignments from
    /// SSA are accepted when their target and operand share the same base;
    /// unrelated phi-copy updates remain `while` loops.
    fn try_promote_for(&self, out: &mut IrBlock, condition: Expr, body: &mut IrBlock) -> bool {
        let Some((update, variable, update_len)) = update_shape(body) else {
            return false;
        };
        if !contains_variable(&condition, &variable) {
            return false;
        }
        let Some((init_index, init)) =
            out.stmts
                .iter()
                .enumerate()
                .rev()
                .find_map(|(index, statement)| match statement {
                    Stmt::Assign { target, value }
                        if symbol_base(target) == symbol_base(&variable)
                            && is_zero_initializer(value) =>
                    {
                        Some((index, statement.clone()))
                    }
                    _ => None,
                })
        else {
            return false;
        };
        if out.stmts[init_index + 1..]
            .iter()
            .any(|statement| statement_mentions_variable(statement, &variable))
        {
            return false;
        }
        out.stmts.remove(init_index);
        body.stmts.truncate(body.stmts.len() - update_len);
        out.push(Stmt::ControlFlow(Box::new(ControlFlow::for_loop(
            Some(init),
            Some(condition),
            Some(update),
            std::mem::take(body),
        ))));
        true
    }

    /// For a do-while loop header `header`, find its latch: the back-edge
    /// predecessor that is a `Branch` re-entering `header`. Returns `(latch,
    /// exit)` where `exit` is the latch's other (non-back-edge) target.
    pub(super) fn find_dowhile_latch(&self, header: BlockId) -> Option<(BlockId, BlockId)> {
        for pred in self.cfg.predecessors(header) {
            // The latch is inside the loop (header dominates it).
            if !self.ssa.dominance.strictly_dominates(header, *pred) {
                continue;
            }
            if let Terminator::Branch {
                then_target,
                else_target,
            } = self.terminator(*pred)
            {
                // One target must be the back-edge to the header; the other is
                // the loop exit.
                if then_target == header {
                    return Some((*pred, else_target));
                }
                if else_target == header {
                    return Some((*pred, then_target));
                }
            }
        }
        None
    }

    pub(super) fn find_unconditional_latch(&self, header: BlockId) -> Option<BlockId> {
        self.cfg
            .predecessors(header)
            .iter()
            .copied()
            .find(|predecessor| {
                self.ssa.dominance.strictly_dominates(header, *predecessor)
                    && matches!(
                        self.terminator(*predecessor),
                        Terminator::Jump { target } | Terminator::Fallthrough { target }
                            if target == header
                    )
            })
    }

    /// Render a branch-headed natural loop as unconditional when both header
    /// successors stay inside it. Terminal return/throw edges may leave the
    /// loop without making the header comparison its loop condition.
    pub(super) fn try_emit_infinite_branch_loop(
        &self,
        out: &mut IrBlock,
        header: BlockId,
        then_target: BlockId,
        else_target: BlockId,
        visited: &mut HashSet<BlockId>,
    ) -> bool {
        if !self.loop_headers.contains(&header) {
            return false;
        }

        let members = self.natural_loop_blocks(header);
        let outside_targets: BTreeSet<_> = members
            .iter()
            .flat_map(|member| self.cfg.successors(*member).iter().copied())
            .filter(|successor| !members.contains(successor))
            .collect();
        if !members.contains(&then_target)
            || !members.contains(&else_target)
            || outside_targets.iter().any(|target| {
                !matches!(
                    self.terminator(*target),
                    Terminator::Return
                        | Terminator::Throw
                        | Terminator::Abort
                        | Terminator::NoReturnCall
                )
            })
        {
            return false;
        }

        let mut body = IrBlock::new();
        self.emit_body_except_condition(&mut body, header);

        if then_target == else_target {
            let shared = self.structure_edge_region(header, then_target, Some(header), visited);
            body.stmts.extend(shared.stmts);
        } else {
            let merge = self
                .closest_loop_merge(then_target, else_target, header, &members)
                .unwrap_or(header);
            let mut then_visited = visited.clone();
            let mut else_visited = visited.clone();
            let then_block =
                self.structure_edge_region(header, then_target, Some(merge), &mut then_visited);
            let else_block =
                self.structure_edge_region(header, else_target, Some(merge), &mut else_visited);
            visited.extend(then_visited);
            visited.extend(else_visited);
            let condition = self
                .comparison_condition_for_block(header)
                .unwrap_or_else(|| self.condition_for_block(header));

            if then_block.is_empty() {
                if !else_block.is_empty() {
                    body.push(Stmt::ControlFlow(Box::new(ControlFlow::if_else(
                        condition, then_block, else_block,
                    ))));
                }
            } else if else_block.is_empty() {
                body.push(Stmt::ControlFlow(Box::new(ControlFlow::if_then(
                    condition, then_block,
                ))));
            } else {
                body.push(Stmt::ControlFlow(Box::new(ControlFlow::if_else(
                    condition, then_block, else_block,
                ))));
            }

            if merge != header {
                let tail = self.structure_region(merge, Some(header), visited, true);
                body.stmts.extend(tail.stmts);
            }
        }

        out.push(Stmt::ControlFlow(Box::new(ControlFlow::while_loop(
            Expr::Literal(crate::decompiler::ir::Literal::Bool(true)),
            body,
        ))));
        true
    }
}

fn update_shape(body: &IrBlock) -> Option<(Expr, String, usize)> {
    let last = body.stmts.last()?;
    if let Stmt::ExprStmt(
        update @ Expr::Unary {
            op: UnaryOp::Inc | UnaryOp::Dec,
            operand,
        },
    ) = last
    {
        let Expr::Variable(variable) = operand.as_ref() else {
            return None;
        };
        return Some((update.clone(), variable.clone(), 1));
    }
    if let Stmt::Assign {
        target,
        value: Expr::Unary {
            op: update_op,
            operand,
        },
    } = last
    {
        if !matches!(update_op, UnaryOp::Inc | UnaryOp::Dec) {
            return None;
        }
        let Expr::Variable(variable) = operand.as_ref() else {
            return None;
        };
        if symbol_base(target) == symbol_base(variable) {
            return Some((
                Expr::unary(*update_op, Expr::var(variable.clone())),
                variable.clone(),
                1,
            ));
        }
    }

    let [prefix @ .., Stmt::Assign {
        target: copied_target,
        value: Expr::Variable(copied_value),
    }] = body.stmts.as_slice()
    else {
        return None;
    };
    let Stmt::Assign {
        target: temporary,
        value: Expr::Unary {
            op: update_op,
            operand,
        },
    } = prefix.last()?
    else {
        return None;
    };
    if !matches!(update_op, UnaryOp::Inc | UnaryOp::Dec) {
        return None;
    }
    let Expr::Variable(variable) = operand.as_ref() else {
        return None;
    };
    if copied_value != temporary || symbol_base(copied_target) != symbol_base(variable) {
        return None;
    }
    Some((
        Expr::unary(*update_op, Expr::var(variable.clone())),
        variable.clone(),
        2,
    ))
}

fn is_zero_initializer(expression: &Expr) -> bool {
    match expression {
        Expr::Literal(Literal::Int(value)) => *value == 0,
        Expr::Literal(Literal::BigInt(value)) => value == "0",
        _ => false,
    }
}

fn statement_mentions_variable(statement: &Stmt, variable: &str) -> bool {
    match statement {
        Stmt::Assign { target, value } => {
            symbol_base(target) == symbol_base(variable) || contains_variable(value, variable)
        }
        Stmt::Return(value) | Stmt::Throw(value) | Stmt::Abort(value) => value
            .as_ref()
            .is_some_and(|value| contains_variable(value, variable)),
        Stmt::Assert { condition, message } => {
            contains_variable(condition, variable)
                || message
                    .as_ref()
                    .is_some_and(|message| contains_variable(message, variable))
        }
        Stmt::ExprStmt(value) => contains_variable(value, variable),
        Stmt::ControlFlow(_) => true,
        Stmt::Comment(_) | Stmt::Break | Stmt::Continue | Stmt::Label(_) | Stmt::Goto(_) => false,
    }
}

fn symbol_base(name: &str) -> &str {
    name.rsplit_once('_')
        .filter(|(_, suffix)| {
            !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
        })
        .map_or(name, |(base, _)| base)
}

fn contains_variable(expression: &Expr, name: &str) -> bool {
    match expression {
        Expr::Variable(variable) => variable == name,
        Expr::Binary { left, right, .. } => {
            contains_variable(left, name) || contains_variable(right, name)
        }
        Expr::Unary { operand, .. }
        | Expr::Convert { value: operand, .. }
        | Expr::IsType { value: operand, .. }
        | Expr::Cast { expr: operand, .. } => contains_variable(operand, name),
        Expr::Call { args, .. } | Expr::Array(args) | Expr::Struct(args) => {
            args.iter().any(|arg| contains_variable(arg, name))
        }
        Expr::Index { base, index } => {
            contains_variable(base, name) || contains_variable(index, name)
        }
        Expr::Member { base, .. } => contains_variable(base, name),
        Expr::NewArray { length, .. } => contains_variable(length, name),
        Expr::Map(entries) => entries
            .iter()
            .any(|(key, value)| contains_variable(key, name) || contains_variable(value, name)),
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            contains_variable(condition, name)
                || contains_variable(then_expr, name)
                || contains_variable(else_expr, name)
        }
        Expr::Unknown | Expr::Literal(_) | Expr::StackTemp(_) => false,
    }
}
