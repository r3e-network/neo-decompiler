//! IR to text rendering utilities.

use super::control_flow::ControlFlow;
use super::expression::{Expr, Literal, UnaryOp};
use super::statement::{Block, Stmt};

const INDENT: &str = "    ";

/// Render an expression to a string.
pub fn render_expr(expr: &Expr) -> String {
    match expr {
        Expr::Literal(lit) => render_literal(lit),
        Expr::Variable(name) => name.clone(),
        Expr::Binary { op, left, right } => {
            format!("({} {} {})", render_expr(left), op, render_expr(right))
        }
        Expr::Unary { op, operand } => match op {
            UnaryOp::Abs | UnaryOp::Sign => {
                format!("{}({})", op, render_expr(operand))
            }
            UnaryOp::Inc | UnaryOp::Dec => {
                format!("{}{}", render_expr(operand), op)
            }
            _ => format!("{}{}", op, render_expr(operand)),
        },
        Expr::Call { name, args } => {
            let args_str = args.iter().map(render_expr).collect::<Vec<_>>().join(", ");
            format!("{}({})", name, args_str)
        }
        Expr::Index { base, index } => {
            format!("{}[{}]", render_expr(base), render_expr(index))
        }
        Expr::Member { base, name } => {
            format!("{}.{}", render_expr(base), name)
        }
        Expr::Cast { expr, target_type } => {
            format!("({})({}))", target_type, render_expr(expr))
        }
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
        } => {
            format!(
                "({} ? {} : {})",
                render_expr(condition),
                render_expr(then_expr),
                render_expr(else_expr)
            )
        }
        Expr::StackTemp(idx) => format!("_tmp{}", idx),
    }
}

fn render_literal(lit: &Literal) -> String {
    lit.to_string()
}

/// Render a statement to a string with the given indentation level.
pub fn render_stmt(stmt: &Stmt, indent: usize) -> String {
    let prefix = INDENT.repeat(indent);
    match stmt {
        Stmt::Assign { target, value } => {
            format!("{}{} = {};", prefix, target, render_expr(value))
        }
        Stmt::Return(Some(expr)) => {
            format!("{}return {};", prefix, render_expr(expr))
        }
        Stmt::Return(None) => {
            format!("{}return;", prefix)
        }
        Stmt::ExprStmt(expr) => {
            format!("{}{};", prefix, render_expr(expr))
        }
        Stmt::Comment(text) => {
            format!("{}// {}", prefix, text)
        }
        Stmt::ControlFlow(cf) => render_control_flow(cf, indent),
        Stmt::VarDecl {
            name,
            var_type,
            init,
        } => {
            let type_str = var_type
                .as_ref()
                .map(|t| format!("{} ", t))
                .unwrap_or_default();
            match init {
                Some(expr) => format!(
                    "{}let {}{} = {};",
                    prefix,
                    type_str,
                    name,
                    render_expr(expr)
                ),
                None => format!("{}let {}{};", prefix, type_str, name),
            }
        }
        Stmt::Throw(Some(expr)) => {
            format!("{}throw {};", prefix, render_expr(expr))
        }
        Stmt::Throw(None) => {
            format!("{}throw;", prefix)
        }
        Stmt::Break => format!("{}break;", prefix),
        Stmt::Continue => format!("{}continue;", prefix),
        Stmt::Unlifted {
            offset,
            opcode,
            comment,
        } => {
            format!("{}// {:#06X}: {} ({})", prefix, offset, opcode, comment)
        }
    }
}

/// Render a block of statements.
pub fn render_block(block: &Block, indent: usize) -> String {
    block
        .stmts
        .iter()
        .map(|stmt| render_stmt(stmt, indent))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_control_flow(cf: &ControlFlow, indent: usize) -> String {
    let prefix = INDENT.repeat(indent);
    match cf {
        ControlFlow::If {
            condition,
            then_branch,
            else_branch,
        } => {
            let mut result = format!("{}if ({}) {{\n", prefix, render_expr(condition));
            result.push_str(&render_block(then_branch, indent + 1));
            if let Some(else_branch) = else_branch {
                result.push_str(&format!("\n{}}} else {{\n", prefix));
                result.push_str(&render_block(else_branch, indent + 1));
            }
            result.push_str(&format!("\n{}}}", prefix));
            result
        }
        ControlFlow::While { condition, body } => {
            let mut result = format!("{}while ({}) {{\n", prefix, render_expr(condition));
            result.push_str(&render_block(body, indent + 1));
            result.push_str(&format!("\n{}}}", prefix));
            result
        }
        ControlFlow::DoWhile { body, condition } => {
            let mut result = format!("{}do {{\n", prefix);
            result.push_str(&render_block(body, indent + 1));
            result.push_str(&format!(
                "\n{}}} while ({});",
                prefix,
                render_expr(condition)
            ));
            result
        }
        ControlFlow::For {
            init,
            condition,
            update,
            body,
        } => {
            let init_str = init
                .as_ref()
                .map(|s| render_stmt(s, 0).trim_end_matches(';').to_string())
                .unwrap_or_default();
            let cond_str = condition.as_ref().map(render_expr).unwrap_or_default();
            let update_str = update.as_ref().map(render_expr).unwrap_or_default();

            let mut result = format!(
                "{}for ({}; {}; {}) {{\n",
                prefix, init_str, cond_str, update_str
            );
            result.push_str(&render_block(body, indent + 1));
            result.push_str(&format!("\n{}}}", prefix));
            result
        }
        ControlFlow::TryCatch {
            try_body,
            catch_var,
            catch_body,
            finally_body,
        } => {
            let mut result = format!("{}try {{\n", prefix);
            result.push_str(&render_block(try_body, indent + 1));
            result.push_str(&format!("\n{}}}", prefix));

            if let Some(catch_body) = catch_body {
                let catch_var_str = catch_var
                    .as_ref()
                    .map(|v| format!("({})", v))
                    .unwrap_or_default();
                result.push_str(&format!(" catch{} {{\n", catch_var_str));
                result.push_str(&render_block(catch_body, indent + 1));
                result.push_str(&format!("\n{}}}", prefix));
            }

            if let Some(finally_body) = finally_body {
                result.push_str(" finally {\n");
                result.push_str(&render_block(finally_body, indent + 1));
                result.push_str(&format!("\n{}}}", prefix));
            }

            result
        }
        ControlFlow::Switch {
            expr,
            cases,
            default,
        } => {
            let mut result = format!("{}switch ({}) {{\n", prefix, render_expr(expr));
            for (case_expr, case_body) in cases {
                result.push_str(&format!(
                    "{}case {}:\n",
                    INDENT.repeat(indent + 1),
                    render_expr(case_expr)
                ));
                result.push_str(&render_block(case_body, indent + 2));
                result.push('\n');
            }
            if let Some(default_body) = default {
                result.push_str(&format!("{}default:\n", INDENT.repeat(indent + 1)));
                result.push_str(&render_block(default_body, indent + 2));
                result.push('\n');
            }
            result.push_str(&format!("{}}}", prefix));
            result
        }
    }
}
