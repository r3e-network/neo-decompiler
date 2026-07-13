use std::collections::HashSet;

use crate::decompiler::cfg::BlockId;
use crate::decompiler::ir::{BinOp, Block as IrBlock, ControlFlow, Expr, Stmt};

use super::super::ssa::{ssa_expr_to_ir_with_source_names, SsaExpr, SsaStmt, SsaVariable, UseSite};
use super::StructCtx;

impl<'a> StructCtx<'a> {
    pub(super) fn handle_branch(
        &self,
        out: &mut IrBlock,
        bid: BlockId,
        then_target: BlockId,
        else_target: BlockId,
        outer_boundary: Option<BlockId>,
        visited: &mut HashSet<BlockId>,
    ) -> Option<BlockId> {
        let cond = self
            .comparison_condition_for_block(bid)
            .unwrap_or_else(|| self.condition_for_block(bid));

        if then_target == else_target {
            out.stmts
                .extend(self.phi_lowering.edge_statements(bid, then_target));
            return Some(then_target);
        }

        if let Some(switch) = self.try_switch(bid, then_target, else_target, visited) {
            out.push(Stmt::ControlFlow(Box::new(ControlFlow::Switch {
                expr: switch.scrutinee,
                cases: switch.cases,
                default: switch.default,
            })));
            return match switch.merge {
                Some(m) if Some(m) != outer_boundary => Some(m),
                _ => None,
            };
        }

        let merge = self.find_merge(then_target, else_target);
        let mut then_visited = visited.clone();
        let mut else_visited = visited.clone();
        let then_block = self.structure_edge_region(bid, then_target, merge, &mut then_visited);
        let else_block = self.structure_edge_region(bid, else_target, merge, &mut else_visited);
        visited.extend(then_visited);
        visited.extend(else_visited);

        let cf = if else_block.is_empty() && then_block.is_empty() {
            out.push(Stmt::ExprStmt(cond));
            return merge.or(Some(else_target));
        } else if else_block.is_empty() {
            ControlFlow::if_then(cond, then_block)
        } else {
            ControlFlow::if_else(cond, then_block, else_block)
        };
        out.push(Stmt::ControlFlow(Box::new(cf)));

        match merge {
            Some(m) if Some(m) != outer_boundary => Some(m),
            _ => None,
        }
    }

    pub(super) fn handle_branch_in_set(
        &self,
        out: &mut IrBlock,
        bid: BlockId,
        then_target: BlockId,
        else_target: BlockId,
        stop: &HashSet<BlockId>,
        visited: &mut HashSet<BlockId>,
    ) -> Option<BlockId> {
        let cond = self
            .comparison_condition_for_block(bid)
            .unwrap_or_else(|| self.condition_for_block(bid));
        if then_target == else_target {
            out.stmts
                .extend(self.phi_lowering.edge_statements(bid, then_target));
            return (!stop.contains(&then_target)).then_some(then_target);
        }

        let then_distances = self.shortest_distances(then_target);
        let else_distances = self.shortest_distances(else_target);
        let merge = self.find_merge(then_target, else_target);
        let merge_score = merge.and_then(|block| {
            Some((
                *then_distances.get(&block)?.max(else_distances.get(&block)?),
                block,
            ))
        });
        let stop_score = stop
            .iter()
            .filter_map(|block| {
                Some((
                    *then_distances.get(block)?.max(else_distances.get(block)?),
                    *block,
                ))
            })
            .min();
        let merge = match (merge_score, stop_score) {
            (Some((merge_distance, merge)), Some((stop_distance, _)))
                if merge_distance < stop_distance =>
            {
                Some(merge)
            }
            (Some((_, merge)), None) => Some(merge),
            _ => None,
        };

        let mut arm_stop = stop.clone();
        if let Some(merge) = merge {
            arm_stop.insert(merge);
        }
        let mut then_visited = visited.clone();
        let mut else_visited = visited.clone();
        let then_block =
            self.structure_set_edge_region(bid, then_target, &arm_stop, &mut then_visited);
        let else_block =
            self.structure_set_edge_region(bid, else_target, &arm_stop, &mut else_visited);
        visited.extend(then_visited);
        visited.extend(else_visited);
        let control = if else_block.is_empty() && then_block.is_empty() {
            out.push(Stmt::ExprStmt(cond));
            return merge;
        } else if else_block.is_empty() {
            ControlFlow::if_then(cond, then_block)
        } else {
            ControlFlow::if_else(cond, then_block, else_block)
        };
        out.push(Stmt::ControlFlow(Box::new(control)));
        merge
    }

    pub(super) fn comparison_condition_for_block(&self, bid: BlockId) -> Option<Expr> {
        let value = self.condition_expression(bid)?;
        let SsaExpr::Binary { op, .. } = value else {
            return None;
        };
        let is_comparison = matches!(
            op,
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
        );
        is_comparison.then(|| ssa_expr_to_ir_with_source_names(&value, self.source_names))
    }

    pub(super) fn condition_for_block(&self, bid: BlockId) -> Expr {
        self.condition_expression(bid).map_or_else(
            || Expr::Variable(format!("cond_{}", bid.0)),
            |condition| ssa_expr_to_ir_with_source_names(&condition, self.source_names),
        )
    }

    pub(super) fn condition_variable_for_block(&self, bid: BlockId) -> Option<&SsaVariable> {
        let terminator_site = UseSite::terminator(bid);
        self.ssa
            .uses
            .iter()
            .find_map(|(variable, sites)| sites.contains(&terminator_site).then_some(variable))
            .or_else(|| {
                self.ssa.block(bid)?.stmts.iter().rev().find_map(|stmt| {
                    if let SsaStmt::Assign { target, .. } = stmt {
                        Some(target)
                    } else {
                        None
                    }
                })
            })
    }

    pub(super) fn condition_expression(&self, bid: BlockId) -> Option<SsaExpr> {
        let condition = self.condition_variable_for_block(bid)?;
        if !self.can_inline_condition(bid, condition) {
            return Some(SsaExpr::var(condition.clone()));
        }
        let block = self.ssa.block(bid)?;
        for stmt in &block.stmts {
            if let SsaStmt::Assign { target, value } = stmt {
                if target == condition {
                    return Some(value.clone());
                }
            }
        }
        Some(SsaExpr::var(condition.clone()))
    }

    pub(super) fn can_inline_condition(&self, bid: BlockId, condition: &SsaVariable) -> bool {
        let terminator_site = UseSite::terminator(bid);
        self.ssa.uses_of(condition).is_none_or(|sites| {
            sites.is_empty() || (sites.len() == 1 && sites.contains(&terminator_site))
        })
    }
}
