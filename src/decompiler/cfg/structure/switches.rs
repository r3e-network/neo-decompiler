//! Switch-cascade recovery for the CFG structurer.

use std::collections::HashSet;

use crate::decompiler::cfg::{BlockId, Terminator};
use crate::decompiler::ir::{Block as IrBlock, Expr};

use super::super::ssa::{ssa_expr_to_ir_with_source_names, SsaExpr, SsaStmt, SsaVariable};
use super::StructCtx;

pub(super) struct SwitchResult {
    pub(super) scrutinee: Expr,
    pub(super) cases: Vec<(Expr, IrBlock)>,
    pub(super) default: Option<IrBlock>,
    pub(super) merge: Option<BlockId>,
}

impl<'a> StructCtx<'a> {
    /// Recognize a switch represented as an equality cascade on one scrutinee.
    pub(super) fn try_switch(
        &self,
        bid: BlockId,
        then_target: BlockId,
        else_target: BlockId,
        visited: &mut HashSet<BlockId>,
    ) -> Option<SwitchResult> {
        let first = self.condition_expression(bid)?;
        let (scrutinee, first_val) = extract_eq_cond(&first)?;
        let scrut_base = scrutinee.base.clone();
        let mut cases = vec![(
            ssa_expr_to_ir_with_source_names(&first_val, self.source_names),
            bid,
            then_target,
        )];
        let mut current = else_target;
        let mut default_from = bid;
        let default_entry;
        loop {
            if self.cfg.predecessors(current).len() >= 2 {
                default_entry = current;
                break;
            }
            match self.terminator(current) {
                Terminator::Branch {
                    then_target,
                    else_target,
                } => {
                    if !self.can_promote_switch_comparison(current, &scrut_base) {
                        default_entry = current;
                        break;
                    }
                    match self
                        .condition_expression(current)
                        .and_then(|condition| extract_eq_cond(&condition))
                    {
                        Some((variable, value)) if variable.base == scrut_base => {
                            cases.push((
                                ssa_expr_to_ir_with_source_names(&value, self.source_names),
                                current,
                                then_target,
                            ));
                            default_from = current;
                            current = else_target;
                        }
                        _ => {
                            default_entry = current;
                            break;
                        }
                    }
                }
                _ => {
                    default_entry = current;
                    break;
                }
            }
        }
        if cases.len() < 2 {
            return None;
        }
        let merge = self.find_merge(cases[0].2, default_entry);
        let case_blocks = cases
            .iter()
            .map(|(value, comparison_block, body_entry)| {
                (
                    value.clone(),
                    self.structure_edge_region(*comparison_block, *body_entry, merge, visited),
                )
            })
            .collect();
        let default_body = self.structure_edge_region(default_from, default_entry, merge, visited);
        Some(SwitchResult {
            scrutinee: ssa_expr_to_ir_with_source_names(
                &SsaExpr::var(scrutinee),
                self.source_names,
            ),
            cases: case_blocks,
            default: (!default_body.is_empty()).then_some(default_body),
            merge,
        })
    }

    fn can_promote_switch_comparison(&self, bid: BlockId, scrutinee: &str) -> bool {
        let Some(condition) = self.condition_variable_for_block(bid) else {
            return false;
        };
        if !self.can_inline_condition(bid, condition) {
            return false;
        }
        self.ssa.block(bid).is_some_and(|block| {
            block.phi_nodes.is_empty()
                && block.stmts.iter().all(|statement| match statement {
                    SsaStmt::Assign { target, .. } if target == condition => true,
                    SsaStmt::Assign { target, value } => {
                        target.base == scrutinee && is_slot_load(value)
                    }
                    _ => false,
                })
        })
    }
}

fn extract_eq_cond(expression: &SsaExpr) -> Option<(SsaVariable, SsaExpr)> {
    let SsaExpr::Binary {
        op: crate::decompiler::ir::BinOp::Eq,
        left,
        right,
    } = expression
    else {
        return None;
    };
    match (left.as_ref(), right.as_ref()) {
        (SsaExpr::Variable(variable), value) if is_literal(value) => {
            Some((variable.clone(), value.clone()))
        }
        (value, SsaExpr::Variable(variable)) if is_literal(value) => {
            Some((variable.clone(), value.clone()))
        }
        _ => None,
    }
}

fn is_literal(expression: &SsaExpr) -> bool {
    matches!(expression, SsaExpr::Literal(_))
}

fn is_slot_load(expression: &SsaExpr) -> bool {
    matches!(
        expression,
        SsaExpr::Call {
            target: crate::decompiler::ir::SemanticCallTarget::Intrinsic(
                crate::decompiler::ir::Intrinsic::Opcode(
                    crate::instruction::OpCode::Ldloc0
                        | crate::instruction::OpCode::Ldloc1
                        | crate::instruction::OpCode::Ldloc2
                        | crate::instruction::OpCode::Ldloc3
                        | crate::instruction::OpCode::Ldloc4
                        | crate::instruction::OpCode::Ldloc5
                        | crate::instruction::OpCode::Ldloc6
                        | crate::instruction::OpCode::Ldloc
                        | crate::instruction::OpCode::Ldarg0
                        | crate::instruction::OpCode::Ldarg1
                        | crate::instruction::OpCode::Ldarg2
                        | crate::instruction::OpCode::Ldarg3
                        | crate::instruction::OpCode::Ldarg4
                        | crate::instruction::OpCode::Ldarg5
                        | crate::instruction::OpCode::Ldarg6
                        | crate::instruction::OpCode::Ldarg
                        | crate::instruction::OpCode::Ldsfld0
                        | crate::instruction::OpCode::Ldsfld1
                        | crate::instruction::OpCode::Ldsfld2
                        | crate::instruction::OpCode::Ldsfld3
                        | crate::instruction::OpCode::Ldsfld4
                        | crate::instruction::OpCode::Ldsfld5
                        | crate::instruction::OpCode::Ldsfld6
                        | crate::instruction::OpCode::Ldsfld
                ),
            ),
            args,
        } if args.is_empty()
    )
}
