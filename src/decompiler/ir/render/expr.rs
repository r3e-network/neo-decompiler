use super::super::expression::{Expr, Literal, UnaryOp};

pub fn render_expr(expr: &Expr) -> String {
    match expr {
        Expr::Literal(lit) => render_literal(lit),
        Expr::Variable(name) => name.clone(),
        Expr::Binary { op, left, right } => {
            format!("({} {} {})", render_expr(left), op, render_expr(right))
        }
        Expr::Unary { op, operand } => match op {
            UnaryOp::Abs | UnaryOp::Sign => format!("{}({})", op, render_expr(operand)),
            UnaryOp::Inc | UnaryOp::Dec => format!("{}{}", render_expr(operand), op),
            _ => format!("{}{}", op, render_expr(operand)),
        },
        Expr::Call { name, args } => {
            let args_str = args.iter().map(render_expr).collect::<Vec<_>>().join(", ");
            format!("{}({})", name, args_str)
        }
        Expr::Index { base, index } => format!("{}[{}]", render_expr(base), render_expr(index)),
        Expr::Member { base, name } => format!("{}.{}", render_expr(base), name),
        Expr::Cast { expr, target_type } => format!("({})({})", target_type, render_expr(expr)),
        Expr::Array(elements) => {
            let elems = elements
                .iter()
                .map(render_expr)
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{}]", elems)
        }
        Expr::Map(pairs) => {
            let items = pairs
                .iter()
                .map(|(k, v)| format!("{}: {}", render_expr(k), render_expr(v)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{{}}}", items)
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => format!(
            "({} ? {} : {})",
            render_expr(condition),
            render_expr(then_expr),
            render_expr(else_expr)
        ),
        Expr::StackTemp(idx) => format!("_tmp{}", idx),
    }
}

fn render_literal(lit: &Literal) -> String {
    lit.to_string()
}
