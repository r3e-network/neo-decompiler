//! Collapse compiler-generated `SIZE`-guarded signed integer normalization.
//!
//! The Neo C# compiler leaves small signed integers unchanged and normalizes
//! wider values through a width-specific mask plus sign extension. The shape is
//! semantically equivalent to the unconditional mask/sign-extension sequence
//! used by `int_normalization`, but it is represented as a separate `SIZE`
//! branch in the structured IR.

use crate::decompiler::ir::{
    BinOp, Block as IrBlock, ControlFlow, Expr, Intrinsic, Literal, SemanticCallTarget, Stmt,
};
use crate::instruction::OpCode;

use super::int_normalization_uses::statement_uses_variable;

const I32_WIDTH: i64 = 4;
const I64_WIDTH: i64 = 8;
const I32_MAX: i64 = 2_147_483_647;
const I64_MAX: i64 = 9_223_372_036_854_775_807;
const U32_MAX: i64 = 4_294_967_295;
const U32_MODULUS: i64 = 4_294_967_296;
const U64_MAX: &str = "18446744073709551615";
const U64_MODULUS: &str = "18446744073709551616";

#[derive(Debug, Clone, PartialEq)]
struct Normalization {
    width: i64,
    mask: Literal,
    max: Literal,
    modulus: Literal,
}

fn i32_normalization() -> Normalization {
    Normalization {
        width: I32_WIDTH,
        mask: Literal::Int(U32_MAX),
        max: Literal::Int(I32_MAX),
        modulus: Literal::Int(U32_MODULUS),
    }
}

fn i64_normalization() -> Normalization {
    Normalization {
        width: I64_WIDTH,
        mask: Literal::BigInt(U64_MAX.to_string()),
        max: Literal::Int(I64_MAX),
        modulus: Literal::BigInt(U64_MODULUS.to_string()),
    }
}

pub(super) fn collapse_size_wrappers(block: &mut IrBlock) {
    collapse_children(block);

    let mut index = 0;
    while index + 2 < block.stmts.len() {
        let full_match = (index + 3 < block.stmts.len()).then(|| {
            match_size_wrapper(
                &block.stmts[index + 1],
                &block.stmts[index + 2],
                Some(&block.stmts[index + 3]),
            )
            .map(|replacement| (3, replacement))
        });
        let Some((consumed, replacement)) = full_match.flatten().or_else(|| {
            match_size_wrapper(&block.stmts[index + 1], &block.stmts[index + 2], None)
                .map(|replacement| (2, replacement))
        }) else {
            index += 1;
            continue;
        };

        let preserve_normalized = block.stmts[index + 1 + consumed..]
            .iter()
            .any(|statement| statement_uses_variable(statement, &replacement.normalized_var));
        let replacement = replacement.render(preserve_normalized);
        let replacement_len = replacement.len();
        block
            .stmts
            .splice(index + 1..index + 1 + consumed, replacement);
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
                collapse_size_wrappers(then_branch);
                if let Some(else_branch) = else_branch {
                    collapse_size_wrappers(else_branch);
                }
            }
            ControlFlow::While { body, .. } | ControlFlow::DoWhile { body, .. } => {
                collapse_size_wrappers(body);
            }
            ControlFlow::For { init, body, .. } => {
                if let Some(Stmt::ControlFlow(control)) = init.as_deref_mut() {
                    let mut wrapper = IrBlock::with_stmts(vec![Stmt::ControlFlow(control.clone())]);
                    collapse_size_wrappers(&mut wrapper);
                    if let Some(Stmt::ControlFlow(rewritten)) = wrapper.stmts.pop() {
                        *control = rewritten;
                    }
                }
                collapse_size_wrappers(body);
            }
            ControlFlow::TryCatch {
                try_body,
                catch_body,
                finally_body,
                ..
            } => {
                collapse_size_wrappers(try_body);
                if let Some(catch_body) = catch_body {
                    collapse_size_wrappers(catch_body);
                }
                if let Some(finally_body) = finally_body {
                    collapse_size_wrappers(finally_body);
                }
            }
            ControlFlow::Switch { cases, default, .. } => {
                for (_, case_body) in cases {
                    collapse_size_wrappers(case_body);
                }
                if let Some(default) = default {
                    collapse_size_wrappers(default);
                }
            }
        }
    }
}

struct WrapperMatch {
    operation_var: String,
    normalized_var: String,
    mask_var: String,
    normalization: Normalization,
    final_statement: FinalStatement,
}

impl WrapperMatch {
    fn render(self, preserve_normalized: bool) -> Vec<Stmt> {
        let Self {
            operation_var,
            normalized_var,
            mask_var,
            normalization,
            final_statement,
        } = self;
        let mut replacement = vec![
            Stmt::assign(
                mask_var.clone(),
                Expr::binary(
                    BinOp::And,
                    Expr::var(operation_var),
                    Expr::Literal(normalization.mask.clone()),
                ),
            ),
            Stmt::ControlFlow(Box::new(ControlFlow::if_then(
                Expr::binary(
                    BinOp::Gt,
                    Expr::var(mask_var.clone()),
                    Expr::Literal(normalization.max.clone()),
                ),
                IrBlock::with_stmts(vec![Stmt::assign(
                    mask_var.clone(),
                    Expr::binary(
                        BinOp::Sub,
                        Expr::var(mask_var.clone()),
                        Expr::Literal(normalization.modulus.clone()),
                    ),
                )]),
            ))),
        ];
        match final_statement {
            FinalStatement::Assign(destination) => {
                replacement.push(Stmt::assign(
                    destination.clone(),
                    Expr::var(mask_var.clone()),
                ));
                if preserve_normalized && destination != normalized_var {
                    replacement.push(Stmt::assign(normalized_var, Expr::var(mask_var)));
                }
            }
            FinalStatement::Return => {
                replacement.push(Stmt::Return(Some(Expr::var(mask_var))));
            }
            FinalStatement::SetItem { target, mut args } => {
                *args.last_mut().expect("setitem has three arguments") =
                    Expr::var(mask_var.clone());
                replacement.push(Stmt::ExprStmt(Expr::Call { target, args }));
                if preserve_normalized {
                    replacement.push(Stmt::assign(normalized_var, Expr::var(mask_var)));
                }
            }
        }
        replacement
    }
}

enum FinalStatement {
    Assign(String),
    Return,
    SetItem {
        target: SemanticCallTarget,
        args: Vec<Expr>,
    },
}

fn match_size_wrapper(
    size_statement: &Stmt,
    wrapper: &Stmt,
    copy: Option<&Stmt>,
) -> Option<WrapperMatch> {
    let Stmt::Assign {
        target: size_var,
        value:
            Expr::Call {
                target: SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Size)),
                args,
            },
    } = size_statement
    else {
        return None;
    };
    let [Expr::Variable(operation_var)] = args.as_slice() else {
        return None;
    };
    let Stmt::ControlFlow(control) = wrapper else {
        return None;
    };
    let ControlFlow::If {
        condition,
        then_branch,
        else_branch: Some(else_branch),
    } = control.as_ref()
    else {
        return None;
    };

    let (normal_branch, mask_branch, expected_normalization) =
        if let Some(normalization) = matches_size_check(condition, size_var) {
            (then_branch, else_branch, normalization)
        } else if let Some(normalization) = matches_size_overflow_check(condition, size_var) {
            (else_branch, then_branch, normalization)
        } else {
            return None;
        };
    let normalized_var = direct_copy_target(normal_branch, operation_var)?;
    let (mask_var, normalized_from_mask, actual_normalization) =
        mask_path_target(mask_branch, operation_var)?;
    if normalized_from_mask != normalized_var || actual_normalization != expected_normalization {
        return None;
    }

    let final_statement = match copy {
        Some(Stmt::Assign {
            target: destination,
            value: Expr::Variable(copied_var),
        }) if copied_var == &normalized_var => FinalStatement::Assign(destination.clone()),
        Some(Stmt::Return(Some(Expr::Variable(copied_var)))) if copied_var == &normalized_var => {
            FinalStatement::Return
        }
        Some(Stmt::ExprStmt(Expr::Call {
            target: SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Setitem)),
            args,
        })) if args.len() == 3
            && matches!(args.last(), Some(Expr::Variable(copied_var)) if copied_var == &normalized_var) =>
        {
            FinalStatement::SetItem {
                target: SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Setitem)),
                args: args.clone(),
            }
        }
        None => FinalStatement::Assign(normalized_var.clone()),
        Some(_) => return None,
    };

    Some(WrapperMatch {
        operation_var: operation_var.clone(),
        normalized_var,
        mask_var,
        normalization: expected_normalization,
        final_statement,
    })
}

fn matches_size_check(expression: &Expr, variable: &str) -> Option<Normalization> {
    size_normalization_for_bound(expression, variable, BinOp::Le)
}

fn matches_size_overflow_check(expression: &Expr, variable: &str) -> Option<Normalization> {
    size_normalization_for_bound(expression, variable, BinOp::Gt)
}

fn size_normalization_for_bound(
    expression: &Expr,
    variable: &str,
    operator: BinOp,
) -> Option<Normalization> {
    let bound = bound_value(expression, variable, operator)?;
    match bound {
        I32_WIDTH => Some(i32_normalization()),
        I64_WIDTH => Some(i64_normalization()),
        _ => None,
    }
}

fn matches_bound_check(expression: &Expr, variable: &str, operator: BinOp, bound: i64) -> bool {
    bound_value(expression, variable, operator) == Some(bound)
}

fn bound_value(expression: &Expr, variable: &str, operator: BinOp) -> Option<i64> {
    let Expr::Binary { op, left, right } = expression else {
        return None;
    };
    if *op == operator && is_variable(left, variable) {
        return integer_literal_value(right);
    }
    let reversed = match operator {
        BinOp::Le => BinOp::Ge,
        BinOp::Gt => BinOp::Lt,
        _ => return None,
    };
    (*op == reversed && is_variable(right, variable))
        .then(|| integer_literal_value(left))
        .flatten()
}

fn mask_path_target(
    block: &IrBlock,
    operation_var: &str,
) -> Option<(String, String, Normalization)> {
    let [Stmt::Assign {
        target: mask_var,
        value:
            Expr::Binary {
                op: BinOp::And,
                left,
                right,
            },
    }, Stmt::ControlFlow(inner_control)] = block.stmts.as_slice()
    else {
        return None;
    };
    if !is_variable(left, operation_var) {
        return None;
    }
    let normalization = match right.as_ref() {
        Expr::Literal(Literal::Int(value)) if *value == U32_MAX => i32_normalization(),
        Expr::Literal(Literal::BigInt(value)) if value == U64_MAX => i64_normalization(),
        _ => return None,
    };
    let ControlFlow::If {
        condition,
        then_branch,
        else_branch: Some(else_branch),
    } = inner_control.as_ref()
    else {
        return None;
    };
    if !matches_bound_check(
        condition,
        mask_var,
        BinOp::Le,
        match &normalization.max {
            Literal::Int(value) => *value,
            Literal::BigInt(value) => value.parse::<i64>().ok()?,
            _ => return None,
        },
    ) {
        return None;
    }
    direct_copy_target(then_branch, mask_var)?;
    let [Stmt::Assign {
        target: signed_var,
        value:
            Expr::Binary {
                op: BinOp::Sub,
                left,
                right,
            },
    }, Stmt::Assign {
        target: normalized_var,
        value: Expr::Variable(copied_signed_var),
    }] = else_branch.stmts.as_slice()
    else {
        return None;
    };
    if copied_signed_var != signed_var
        || !is_variable(left, mask_var)
        || !literal_matches(right, &normalization.modulus)
    {
        return None;
    }
    Some((mask_var.clone(), normalized_var.clone(), normalization))
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

fn is_variable(expression: &Expr, expected: &str) -> bool {
    matches!(expression, Expr::Variable(name) if name == expected)
}

fn integer_literal_value(expression: &Expr) -> Option<i64> {
    match expression {
        Expr::Literal(Literal::Int(value)) => Some(*value),
        Expr::Literal(Literal::BigInt(value)) => value.parse::<i64>().ok(),
        _ => None,
    }
}

fn literal_matches(expression: &Expr, expected: &Literal) -> bool {
    match (expression, expected) {
        (Expr::Literal(Literal::Int(actual)), Literal::Int(expected)) => actual == expected,
        (Expr::Literal(Literal::BigInt(actual)), Literal::BigInt(expected)) => actual == expected,
        (Expr::Literal(Literal::Int(actual)), Literal::BigInt(expected)) => {
            expected.parse::<i64>().ok() == Some(*actual)
        }
        (Expr::Literal(Literal::BigInt(actual)), Literal::Int(expected)) => {
            actual.parse::<i64>().ok() == Some(*expected)
        }
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

    fn mask_path_with(
        operation: &str,
        normalized: &str,
        mask: &str,
        signed: &str,
        normalization: &Normalization,
    ) -> IrBlock {
        IrBlock::with_stmts(vec![
            assign(
                mask,
                Expr::binary(
                    BinOp::And,
                    v(operation),
                    Expr::Literal(normalization.mask.clone()),
                ),
            ),
            Stmt::ControlFlow(Box::new(ControlFlow::if_else(
                Expr::binary(BinOp::Le, v(mask), Expr::Literal(normalization.max.clone())),
                IrBlock::with_stmts(vec![assign(normalized, v(mask))]),
                IrBlock::with_stmts(vec![
                    assign(
                        signed,
                        Expr::binary(
                            BinOp::Sub,
                            v(mask),
                            Expr::Literal(normalization.modulus.clone()),
                        ),
                    ),
                    assign(normalized, v(signed)),
                ]),
            ))),
        ])
    }

    fn wrapper(bound: i64) -> IrBlock {
        wrapper_with(Normalization {
            width: bound,
            ..i32_normalization()
        })
    }

    fn wrapper_with(normalization: Normalization) -> IrBlock {
        let operation = "t_0";
        let size = "t_2";
        let normalized = "p3_0";
        IrBlock::with_stmts(vec![
            assign(operation, Expr::binary(BinOp::Add, v("a"), v("b"))),
            assign(
                size,
                Expr::call(
                    SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Size)),
                    vec![v(operation)],
                ),
            ),
            Stmt::ControlFlow(Box::new(ControlFlow::if_else(
                Expr::binary(BinOp::Le, v(size), Expr::int(normalization.width)),
                IrBlock::with_stmts(vec![assign(normalized, v(operation))]),
                mask_path_with(operation, normalized, "t_6", "t_11", &normalization),
            ))),
            Stmt::Return(Some(v(normalized))),
        ])
    }

    #[test]
    fn collapses_exact_size_guarded_wrapper() {
        let mut block = wrapper(I32_WIDTH);
        collapse_size_wrappers(&mut block);

        assert_eq!(block.stmts.len(), 4);
        assert!(matches!(
            &block.stmts[1],
            Stmt::Assign {
                target,
                value: Expr::Binary { op: BinOp::And, .. },
            } if target == "t_6"
        ));
        let rendered = crate::decompiler::ir::render_block(&block, 0);
        assert!(!rendered.contains("size("));
        assert!(!rendered.contains("p3_0"));
        assert!(rendered.contains("t_6 > 2147483647"));
    }

    #[test]
    fn collapses_i64_size_guarded_wrapper() {
        let mut block = wrapper_with(i64_normalization());
        collapse_size_wrappers(&mut block);

        let rendered = crate::decompiler::ir::render_block(&block, 0);
        assert!(!rendered.contains("size("));
        assert!(!rendered.contains("p3_0"));
        assert!(rendered.contains("BigInteger(\"18446744073709551615\")"));
        assert!(rendered.contains("9223372036854775807"));
    }

    #[test]
    fn leaves_unrecognized_size_bound_untouched() {
        let mut block = wrapper(5);
        let before = block.clone();
        collapse_size_wrappers(&mut block);
        assert_eq!(block, before);
    }
}
