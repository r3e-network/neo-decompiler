//! Collapse compiler-generated unchecked `int32` normalization wrappers.
//!
//! Neo C# compiler output widens integer arithmetic to VM integers and then
//! emits a signed-32-bit normalization sequence around the result.  The CFG
//! structurer faithfully represents that sequence as nested branches, which
//! is useful for analysis but noisy in generated C#.  This pass recognizes the
//! complete, side-effect-free shape and replaces it with the equivalent mask
//! and sign-extension expression.

use crate::decompiler::ir::{BinOp, Block as IrBlock, ControlFlow, Expr, Literal, Stmt};

const I32_MIN: i64 = -2_147_483_648;
const I32_MAX: i64 = 2_147_483_647;
const U32_MAX: i64 = 4_294_967_295;
const U32_MODULUS: i64 = 4_294_967_296;

pub(super) fn collapse_int32_wrappers(block: &mut IrBlock) {
    collapse_children(block);

    let mut index = 0;
    while index + 2 < block.stmts.len() {
        let replacement = match_int32_wrapper(
            &block.stmts[index],
            &block.stmts[index + 1],
            &block.stmts[index + 2],
        );
        let Some(replacement) = replacement else {
            index += 1;
            continue;
        };

        // Keep the original arithmetic assignment. Only the branch wrapper
        // and its phi-like copy are replaced by the normalized sequence.
        let replacement_len = replacement.len();
        block.stmts.splice(index + 1..index + 3, replacement);
        index += replacement_len + 1;
    }
}

fn collapse_children(block: &mut IrBlock) {
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
                collapse_int32_wrappers(then_branch);
                if let Some(else_branch) = else_branch {
                    collapse_int32_wrappers(else_branch);
                }
            }
            ControlFlow::While { body, .. } | ControlFlow::DoWhile { body, .. } => {
                collapse_int32_wrappers(body);
            }
            ControlFlow::For { init, body, .. } => {
                if let Some(Stmt::ControlFlow(control)) = init.as_deref_mut() {
                    let mut wrapper = IrBlock::with_stmts(vec![Stmt::ControlFlow(control.clone())]);
                    collapse_int32_wrappers(&mut wrapper);
                    if let Some(Stmt::ControlFlow(rewritten)) = wrapper.stmts.pop() {
                        *control = rewritten;
                    }
                }
                collapse_int32_wrappers(body);
            }
            ControlFlow::TryCatch {
                try_body,
                catch_body,
                finally_body,
                ..
            } => {
                collapse_int32_wrappers(try_body);
                if let Some(catch_body) = catch_body {
                    collapse_int32_wrappers(catch_body);
                }
                if let Some(finally_body) = finally_body {
                    collapse_int32_wrappers(finally_body);
                }
            }
            ControlFlow::Switch { cases, default, .. } => {
                for (_, case_body) in cases {
                    collapse_int32_wrappers(case_body);
                }
                if let Some(default) = default {
                    collapse_int32_wrappers(default);
                }
            }
        }
    }
}

fn match_int32_wrapper(operation: &Stmt, wrapper: &Stmt, copy: &Stmt) -> Option<Vec<Stmt>> {
    let Stmt::Assign {
        target: operation_var,
        ..
    } = operation
    else {
        return None;
    };
    let Stmt::ControlFlow(control) = wrapper else {
        return None;
    };
    let ControlFlow::If {
        condition: outer_condition,
        then_branch: in_range,
        else_branch: Some(out_of_range),
    } = control.as_ref()
    else {
        return None;
    };
    if !matches_bound_check(outer_condition, operation_var, BinOp::Ge, I32_MIN) {
        return None;
    }

    let [Stmt::ControlFlow(inner_control)] = in_range.stmts.as_slice() else {
        return None;
    };
    let ControlFlow::If {
        condition: upper_condition,
        then_branch: direct_branch,
        else_branch: Some(inner_out_of_range),
    } = inner_control.as_ref()
    else {
        return None;
    };
    if !matches_bound_check(upper_condition, operation_var, BinOp::Le, I32_MAX) {
        return None;
    }

    let normalized_var = direct_copy_target(direct_branch, operation_var)?;
    let inner_path = mask_path_target(inner_out_of_range, operation_var)?;
    let outer_path = mask_path_target(out_of_range, operation_var)?;
    if inner_path.normalized_var != normalized_var || outer_path.normalized_var != normalized_var {
        return None;
    }

    let final_statement = match copy {
        Stmt::Assign {
            target: destination,
            value: Expr::Variable(copied_var),
        } if copied_var == &normalized_var => FinalStatement::Assign(destination.clone()),
        Stmt::Return(Some(Expr::Variable(copied_var))) if copied_var == &normalized_var => {
            FinalStatement::Return
        }
        _ => return None,
    };

    let mask_var = outer_path.mask_var;
    Some(vec![
        Stmt::assign(
            mask_var.clone(),
            Expr::binary(BinOp::And, Expr::var(operation_var), Expr::int(U32_MAX)),
        ),
        Stmt::ControlFlow(Box::new(ControlFlow::if_then(
            Expr::binary(BinOp::Gt, Expr::var(mask_var.clone()), Expr::int(I32_MAX)),
            IrBlock::with_stmts(vec![Stmt::assign(
                mask_var.clone(),
                Expr::binary(
                    BinOp::Sub,
                    Expr::var(mask_var.clone()),
                    Expr::int(U32_MODULUS),
                ),
            )]),
        ))),
        final_statement.render(mask_var),
    ])
}

enum FinalStatement {
    Assign(String),
    Return,
}

impl FinalStatement {
    fn render(self, value: String) -> Stmt {
        match self {
            Self::Assign(target) => Stmt::assign(target, Expr::var(value)),
            Self::Return => Stmt::Return(Some(Expr::var(value))),
        }
    }
}

fn direct_copy_target(block: &IrBlock, source: &str) -> Option<String> {
    let [Stmt::Assign {
        target,
        value: Expr::Variable(value),
    }] = block.stmts.as_slice()
    else {
        return None;
    };
    (value == source).then(|| target.clone())
}

struct MaskPath {
    normalized_var: String,
    mask_var: String,
}

fn mask_path_target(block: &IrBlock, operation_var: &str) -> Option<MaskPath> {
    let [Stmt::Assign {
        target: mask_var,
        value: mask,
    }, Stmt::ControlFlow(inner_control)] = block.stmts.as_slice()
    else {
        return None;
    };
    if !matches!(
        mask,
        Expr::Binary {
            op: BinOp::And,
            left,
            right,
        } if is_variable(left, operation_var) && is_integer_literal(right, U32_MAX)
    ) {
        return None;
    }

    let ControlFlow::If {
        condition,
        then_branch,
        else_branch: Some(else_branch),
    } = inner_control.as_ref()
    else {
        return None;
    };
    if !matches_bound_check(condition, mask_var, BinOp::Le, I32_MAX) {
        return None;
    }
    direct_copy_target(then_branch, mask_var)?;

    let [Stmt::Assign {
        target: signed_var,
        value: signed_value,
    }, Stmt::Assign {
        target: normalized_var,
        value: Expr::Variable(copied_signed_var),
    }] = else_branch.stmts.as_slice()
    else {
        return None;
    };
    if copied_signed_var != signed_var
        || !matches!(
            signed_value,
            Expr::Binary {
                op: BinOp::Sub,
                left,
                right,
            } if is_variable(left, mask_var) && is_integer_literal(right, U32_MODULUS)
        )
    {
        return None;
    }

    let direct_target = direct_copy_target(then_branch, mask_var)?;
    (direct_target == *normalized_var).then(|| MaskPath {
        normalized_var: normalized_var.clone(),
        mask_var: mask_var.clone(),
    })
}

fn matches_bound_check(expression: &Expr, variable: &str, operator: BinOp, bound: i64) -> bool {
    let Expr::Binary { op, left, right } = expression else {
        return false;
    };
    if *op == operator && is_variable(left, variable) && is_integer_literal(right, bound) {
        return true;
    }

    let reversed = match operator {
        BinOp::Ge => BinOp::Le,
        BinOp::Le => BinOp::Ge,
        _ => return false,
    };
    *op == reversed && is_integer_literal(left, bound) && is_variable(right, variable)
}

fn is_variable(expression: &Expr, expected: &str) -> bool {
    matches!(expression, Expr::Variable(name) if name == expected)
}

fn is_integer_literal(expression: &Expr, expected: i64) -> bool {
    match expression {
        Expr::Literal(Literal::Int(value)) => *value == expected,
        Expr::Literal(Literal::BigInt(value)) => value.parse::<i64>().ok() == Some(expected),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(name: &str) -> Expr {
        Expr::var(name)
    }

    fn assign(name: &str, value: Expr) -> Stmt {
        Stmt::assign(name, value)
    }

    fn mask_path(operation: &str, normalized: &str, mask: &str, signed: &str) -> IrBlock {
        IrBlock::with_stmts(vec![
            assign(
                mask,
                Expr::binary(BinOp::And, v(operation), Expr::int(U32_MAX)),
            ),
            Stmt::ControlFlow(Box::new(ControlFlow::if_else(
                Expr::binary(BinOp::Le, v(mask), Expr::int(I32_MAX)),
                IrBlock::with_stmts(vec![assign(normalized, v(mask))]),
                IrBlock::with_stmts(vec![
                    assign(
                        signed,
                        Expr::binary(BinOp::Sub, v(mask), Expr::int(U32_MODULUS)),
                    ),
                    assign(normalized, v(signed)),
                ]),
            ))),
        ])
    }

    fn wrapper() -> IrBlock {
        let operation = "t_11";
        let normalized = "p6_0";
        IrBlock::with_stmts(vec![
            assign(operation, Expr::binary(BinOp::Add, v("loc1"), v("loc5"))),
            Stmt::ControlFlow(Box::new(ControlFlow::if_else(
                Expr::binary(BinOp::Ge, v(operation), Expr::int(I32_MIN)),
                IrBlock::with_stmts(vec![Stmt::ControlFlow(Box::new(ControlFlow::if_else(
                    Expr::binary(BinOp::Le, v(operation), Expr::int(I32_MAX)),
                    IrBlock::with_stmts(vec![assign(normalized, v(operation))]),
                    mask_path(operation, normalized, "t_19", "t_24"),
                )))]),
                mask_path(operation, normalized, "t_19", "t_24"),
            ))),
            assign("loc1", v(normalized)),
        ])
    }

    #[test]
    fn collapses_exact_wrapper_to_signed_mask_expression() {
        let mut block = wrapper();
        collapse_int32_wrappers(&mut block);

        assert_eq!(block.stmts.len(), 4);
        assert!(matches!(
            &block.stmts[1],
            Stmt::Assign {
                target,
                value: Expr::Binary { op: BinOp::And, .. },
            } if target == "t_19"
        ));
        let rendered = crate::decompiler::ir::render_block(&block, 0);
        assert!(rendered.contains("t_19 > 2147483647"));
        assert!(!rendered.contains("p6_0"));
    }

    #[test]
    fn leaves_partial_wrapper_untouched() {
        let mut block = wrapper();
        let Stmt::ControlFlow(control) = &mut block.stmts[1] else {
            panic!("expected wrapper");
        };
        let ControlFlow::If { else_branch, .. } = control.as_mut() else {
            panic!("expected outer branch");
        };
        else_branch.as_mut().expect("else branch").stmts.pop();

        let before = block.clone();
        collapse_int32_wrappers(&mut block);
        assert_eq!(block, before);
    }

    #[test]
    fn collapses_wrappers_inside_loop_bodies() {
        let body = wrapper();
        let mut block = IrBlock::with_stmts(vec![Stmt::ControlFlow(Box::new(
            ControlFlow::while_loop(Expr::Literal(Literal::Bool(true)), body),
        ))]);
        collapse_int32_wrappers(&mut block);

        let rendered = crate::decompiler::ir::render_block(&block, 0);
        assert!(rendered.contains("t_19 = (t_11 & 4294967295);"));
        assert!(!rendered.contains("p6_0"));
    }

    #[test]
    fn collapses_wrapper_when_the_normalized_value_is_returned() {
        let mut block = wrapper();
        block.stmts[2] = Stmt::Return(Some(v("p6_0")));
        collapse_int32_wrappers(&mut block);

        assert_eq!(block.stmts.len(), 4);
        assert!(
            matches!(block.stmts[3], Stmt::Return(Some(Expr::Variable(ref name))) if name == "t_19")
        );
    }
}
