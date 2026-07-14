use crate::decompiler::ir::{Block as IrBlock, ControlFlow, Expr, Literal, Stmt, UnaryOp};

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
