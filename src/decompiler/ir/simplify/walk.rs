use super::super::expression::Expr;

pub(super) fn simplify_children(expr: Expr) -> Expr {
    match expr {
        Expr::Binary { op, left, right } => Expr::Binary {
            op,
            left: Box::new(super::simplify(*left)),
            right: Box::new(super::simplify(*right)),
        },
        Expr::Unary { op, operand } => Expr::Unary {
            op,
            operand: Box::new(super::simplify(*operand)),
        },
        Expr::Call { name, args } => Expr::Call {
            name,
            args: args.into_iter().map(super::simplify).collect(),
        },
        Expr::Index { base, index } => Expr::Index {
            base: Box::new(super::simplify(*base)),
            index: Box::new(super::simplify(*index)),
        },
        Expr::Member { base, name } => Expr::Member {
            base: Box::new(super::simplify(*base)),
            name,
        },
        Expr::Cast { expr, target_type } => Expr::Cast {
            expr: Box::new(super::simplify(*expr)),
            target_type,
        },
        Expr::Array(elems) => Expr::Array(elems.into_iter().map(super::simplify).collect()),
        Expr::Map(pairs) => Expr::Map(
            pairs
                .into_iter()
                .map(|(k, v)| (super::simplify(k), super::simplify(v)))
                .collect(),
        ),
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => Expr::Ternary {
            condition: Box::new(super::simplify(*condition)),
            then_expr: Box::new(super::simplify(*then_expr)),
            else_expr: Box::new(super::simplify(*else_expr)),
        },
        other => other,
    }
}
