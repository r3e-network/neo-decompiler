use crate::decompiler::ir::{
    Block as IrBlock, ControlFlow, Expr, Intrinsic, Literal, SemanticCallTarget, Stmt, UnaryOp,
};
use crate::instruction::OpCode;

use super::StructCtx;

impl<'a> StructCtx<'a> {
    /// Promote `i = init; while (cond(i)) { ...; i++; }` only when the update
    /// is an explicit unary increment/decrement. Versioned assignments from
    /// SSA are accepted when their target and operand share the same base;
    /// unrelated phi-copy updates remain `while` loops.
    pub(super) fn try_promote_for(
        &self,
        out: &mut IrBlock,
        condition: Expr,
        body: &mut IrBlock,
    ) -> bool {
        let Some((update, variable, update_range)) = update_shape(body, &condition) else {
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
        body.stmts.drain(update_range);
        out.push(Stmt::ControlFlow(Box::new(ControlFlow::for_loop(
            Some(init),
            Some(condition),
            Some(update),
            std::mem::take(body),
        ))));
        true
    }
}

fn update_shape(
    body: &IrBlock,
    condition: &Expr,
) -> Option<(Expr, String, std::ops::Range<usize>)> {
    let last = body.stmts.last()?;
    if let Stmt::ExprStmt(
        update @ Expr::Unary {
            op: UnaryOp::Inc | UnaryOp::Dec,
            operand,
        },
    ) = last
    {
        if let Expr::Variable(variable) = operand.as_ref() {
            return Some((
                update.clone(),
                variable.clone(),
                body.stmts.len() - 1..body.stmts.len(),
            ));
        }
    }
    if let Stmt::Assign {
        target,
        value: Expr::Unary {
            op: update_op,
            operand,
        },
    } = last
    {
        if matches!(update_op, UnaryOp::Inc | UnaryOp::Dec) {
            if let Expr::Variable(variable) = operand.as_ref() {
                if symbol_base(target) == symbol_base(variable) {
                    return Some((
                        Expr::unary(*update_op, Expr::var(variable.clone())),
                        variable.clone(),
                        body.stmts.len() - 1..body.stmts.len(),
                    ));
                }
            }
        }
    }

    if let [prefix @ .., Stmt::Assign {
        target: copied_target,
        value: Expr::Variable(copied_value),
    }] = body.stmts.as_slice()
    {
        if let Some(Stmt::Assign {
            target: temporary,
            value:
                Expr::Unary {
                    op: update_op,
                    operand,
                },
        }) = prefix.last()
        {
            if matches!(update_op, UnaryOp::Inc | UnaryOp::Dec) {
                if let Expr::Variable(variable) = operand.as_ref() {
                    if copied_value == temporary
                        && symbol_base(copied_target) == symbol_base(variable)
                    {
                        return Some((
                            Expr::unary(*update_op, Expr::var(variable.clone())),
                            variable.clone(),
                            body.stmts.len() - 2..body.stmts.len(),
                        ));
                    }
                }
            }
        }
    }
    normalized_update_shape(body, condition)
}

fn normalized_update_shape(
    body: &IrBlock,
    condition: &Expr,
) -> Option<(Expr, String, std::ops::Range<usize>)> {
    for index in (0..body.stmts.len()).rev() {
        let Stmt::Assign {
            target,
            value:
                Expr::Unary {
                    op: update_op,
                    operand,
                },
        } = &body.stmts[index]
        else {
            continue;
        };
        if !matches!(update_op, UnaryOp::Inc | UnaryOp::Dec) || !is_generated_name(target) {
            continue;
        }
        let Expr::Variable(variable) = operand.as_ref() else {
            continue;
        };
        if !contains_variable(condition, variable) {
            continue;
        }
        let suffix = &body.stmts[index + 1..];
        if suffix.is_empty()
            || !suffix
                .iter()
                .all(|statement| is_scalar_normalization(statement, variable))
        {
            continue;
        }

        // A compiler-generated loop may refresh `size(collection)` after
        // normalizing the induction value. Keep that refresh in the body so
        // the original condition still observes the same value each round.
        let remove_end = suffix
            .last()
            .and_then(size_refresh_target)
            .filter(|target| contains_variable(condition, target))
            .map_or(body.stmts.len(), |_| body.stmts.len() - 1);
        if remove_end <= index {
            continue;
        }
        return Some((
            Expr::unary(*update_op, Expr::var(variable.clone())),
            variable.clone(),
            index..remove_end,
        ));
    }
    None
}

fn is_scalar_normalization(statement: &Stmt, induction: &str) -> bool {
    match statement {
        Stmt::Assign { target, value } => {
            is_normalization_target(target, induction) && is_scalar_expression(value, induction)
        }
        Stmt::ControlFlow(control) => match control.as_ref() {
            ControlFlow::If {
                condition,
                then_branch,
                else_branch,
            } => {
                is_scalar_expression(condition, induction)
                    && then_branch
                        .stmts
                        .iter()
                        .all(|statement| is_scalar_normalization(statement, induction))
                    && else_branch.as_ref().is_none_or(|branch| {
                        branch
                            .stmts
                            .iter()
                            .all(|statement| is_scalar_normalization(statement, induction))
                    })
            }
            _ => false,
        },
        _ => false,
    }
}

fn is_normalization_target(name: &str, induction: &str) -> bool {
    symbol_base(name) == symbol_base(induction) || is_generated_name(name)
}

fn is_scalar_expression(expression: &Expr, induction: &str) -> bool {
    match expression {
        Expr::Variable(name) => name == induction || is_generated_name(name),
        Expr::Literal(_) => true,
        Expr::Binary { left, right, .. } => {
            is_scalar_expression(left, induction) && is_scalar_expression(right, induction)
        }
        Expr::Unary { operand, .. }
        | Expr::Convert { value: operand, .. }
        | Expr::IsType { value: operand, .. }
        | Expr::Cast { expr: operand, .. } => is_scalar_expression(operand, induction),
        Expr::Call {
            target: SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Size)),
            args,
        } => args.iter().all(is_side_effect_free_expression),
        _ => false,
    }
}

fn is_side_effect_free_expression(expression: &Expr) -> bool {
    match expression {
        Expr::Variable(_) | Expr::Literal(_) => true,
        Expr::Binary { left, right, .. } => {
            is_side_effect_free_expression(left) && is_side_effect_free_expression(right)
        }
        Expr::Unary { operand, .. }
        | Expr::Convert { value: operand, .. }
        | Expr::IsType { value: operand, .. }
        | Expr::Cast { expr: operand, .. } => is_side_effect_free_expression(operand),
        _ => false,
    }
}

fn size_refresh_target(statement: &Stmt) -> Option<&str> {
    let Stmt::Assign {
        target,
        value:
            Expr::Call {
                target: SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Size)),
                ..
            },
    } = statement
    else {
        return None;
    };
    Some(target)
}

fn is_generated_name(name: &str) -> bool {
    let base = symbol_base(name);
    base == "t"
        || base.strip_prefix('p').is_some_and(|suffix| {
            !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
        })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decompiler::ir::BinOp;

    #[test]
    fn normalized_update_rejects_source_state_after_increment() {
        let induction = Expr::var("loc2_0");
        let condition = Expr::binary(BinOp::Lt, induction.clone(), Expr::var("t_38"));
        let body = IrBlock::with_stmts(vec![
            Stmt::assign("t_24", Expr::unary(UnaryOp::Inc, induction.clone())),
            Stmt::assign("loc1_0", Expr::var("t_24")),
            Stmt::assign(
                "t_38",
                Expr::call(
                    SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Size)),
                    vec![Expr::var("loc0_0")],
                ),
            ),
        ]);

        assert!(normalized_update_shape(&body, &condition).is_none());
    }
}
