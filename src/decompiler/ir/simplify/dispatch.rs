use super::super::expression::{BinOp, Expr, Literal, UnaryOp};

pub(super) fn simplify_binary(op: BinOp, left: Expr, right: Expr) -> Expr {
    match op {
        // Arithmetic identities
        BinOp::Add => super::arithmetic::simplify_add(left, right),
        BinOp::Sub => super::arithmetic::simplify_sub(left, right),
        BinOp::Mul => super::arithmetic::simplify_mul(left, right),
        BinOp::Div => super::arithmetic::simplify_div(left, right),
        BinOp::Pow => super::arithmetic::simplify_pow(left, right),

        // Boolean/comparison simplifications
        BinOp::Eq => super::boolean::simplify_eq(left, right),
        BinOp::Ne => super::boolean::simplify_ne(left, right),
        BinOp::LogicalAnd => super::boolean::simplify_and(left, right),
        BinOp::LogicalOr => super::boolean::simplify_or(left, right),

        // Bitwise identities
        BinOp::And => super::bitwise::simplify_bitwise_and(left, right),
        BinOp::Or => super::bitwise::simplify_bitwise_or(left, right),
        BinOp::Xor => super::bitwise::simplify_xor(left, right),
        BinOp::Shl | BinOp::Shr => super::bitwise::simplify_shift(op, left, right),

        // No simplification for other ops
        _ => Expr::binary(op, left, right),
    }
}

pub(super) fn simplify_unary(op: UnaryOp, operand: Expr) -> Expr {
    match op {
        // Double negation: !!x → x
        UnaryOp::LogicalNot => {
            if let Expr::Unary {
                op: UnaryOp::LogicalNot,
                operand: inner,
            } = operand
            {
                return *inner;
            }
            // !true → false, !false → true
            if let Expr::Literal(Literal::Bool(b)) = operand {
                return Expr::Literal(Literal::Bool(!b));
            }
            Expr::unary(op, operand)
        }

        // Double arithmetic negation: --x → x
        UnaryOp::Neg => {
            if let Expr::Unary {
                op: UnaryOp::Neg,
                operand: inner,
            } = operand
            {
                return *inner;
            }
            // -0 → 0
            if let Expr::Literal(Literal::Int(0)) = operand {
                return Expr::Literal(Literal::Int(0));
            }
            Expr::unary(op, operand)
        }

        // Double bitwise NOT: ~~x → x
        UnaryOp::Not => {
            if let Expr::Unary {
                op: UnaryOp::Not,
                operand: inner,
            } = operand
            {
                return *inner;
            }
            Expr::unary(op, operand)
        }

        _ => Expr::unary(op, operand),
    }
}
