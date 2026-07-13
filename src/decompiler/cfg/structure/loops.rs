use std::collections::{BTreeSet, HashSet};

use crate::decompiler::cfg::{BlockId, Terminator};
use crate::decompiler::ir::{Block as IrBlock, ControlFlow, Expr, Stmt};

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
        let Some(last) = body.stmts.last() else {
            return false;
        };
        let (update, variable) = match last {
            Stmt::ExprStmt(
                update @ Expr::Unary {
                    op: crate::decompiler::ir::UnaryOp::Inc | crate::decompiler::ir::UnaryOp::Dec,
                    operand,
                },
            ) => {
                let Expr::Variable(variable) = operand.as_ref() else {
                    return false;
                };
                (update.clone(), variable.clone())
            }
            Stmt::Assign {
                target,
                value:
                    Expr::Unary {
                        op:
                            op @ (crate::decompiler::ir::UnaryOp::Inc
                            | crate::decompiler::ir::UnaryOp::Dec),
                        operand,
                    },
            } => {
                let Expr::Variable(variable) = operand.as_ref() else {
                    return false;
                };
                if symbol_base(target) != symbol_base(variable) {
                    return false;
                }
                (
                    Expr::Unary {
                        op: *op,
                        operand: Box::new(Expr::Variable(variable.clone())),
                    },
                    variable.clone(),
                )
            }
            _ => return false,
        };
        if !contains_variable(&condition, &variable) {
            return false;
        }
        let Some(Stmt::Assign { target, .. }) = out.stmts.last() else {
            return false;
        };
        if symbol_base(target) != symbol_base(&variable) {
            return false;
        }
        let init = out.stmts.pop();
        body.stmts.pop();
        out.push(Stmt::ControlFlow(Box::new(ControlFlow::for_loop(
            init,
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
