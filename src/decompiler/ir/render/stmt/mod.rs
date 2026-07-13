use super::super::statement::{Block, Stmt};

use super::expr::render_expr;

mod control_flow;

const INDENT: &str = "    ";

#[must_use]
pub fn render_stmt(stmt: &Stmt, indent: usize) -> String {
    let prefix = INDENT.repeat(indent);
    match stmt {
        Stmt::Assign { target, value } => format!("{}{} = {};", prefix, target, render_expr(value)),
        Stmt::Return(Some(expr)) => format!("{}return {};", prefix, render_expr(expr)),
        Stmt::Return(None) => format!("{}return;", prefix),
        Stmt::Throw(Some(expr)) => format!("{}throw({});", prefix, render_expr(expr)),
        Stmt::Throw(None) => format!("{}throw();", prefix),
        Stmt::Abort(Some(message)) => format!("{}abort({});", prefix, render_expr(message)),
        Stmt::Abort(None) => format!("{}abort();", prefix),
        Stmt::Assert {
            condition,
            message: Some(message),
        } => format!(
            "{}assert({}, {});",
            prefix,
            render_expr(condition),
            render_expr(message)
        ),
        Stmt::Assert {
            condition,
            message: None,
        } => format!("{}assert({});", prefix, render_expr(condition)),
        Stmt::ExprStmt(expr) => format!("{}{};", prefix, render_expr(expr)),
        Stmt::Comment(text) => format!("{}// {}", prefix, text),
        Stmt::Break => format!("{}break;", prefix),
        Stmt::Continue => format!("{}continue;", prefix),
        Stmt::Label(label) => format!("{}label_{}:", prefix, label.0),
        Stmt::Goto(label) => format!("{}goto label_{};", prefix, label.0),
        Stmt::ControlFlow(cf) => control_flow::render_control_flow(cf, indent),
    }
}

#[must_use]
pub fn render_block(block: &Block, indent: usize) -> String {
    block
        .stmts
        .iter()
        .map(|stmt| render_stmt(stmt, indent))
        .collect::<Vec<_>>()
        .join("\n")
}
