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
        Stmt::ExprStmt(expr) => format!("{}{};", prefix, render_expr(expr)),
        Stmt::Comment(text) => format!("{}// {}", prefix, text),
        Stmt::ControlFlow(cf) => control_flow::render_control_flow(cf, indent),
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
        Stmt::Throw(Some(expr)) => format!("{}throw {};", prefix, render_expr(expr)),
        Stmt::Throw(None) => format!("{}throw;", prefix),
        Stmt::Break => format!("{}break;", prefix),
        Stmt::Continue => format!("{}continue;", prefix),
        Stmt::Unlifted {
            offset,
            opcode,
            comment,
        } => format!("{}// {:#06X}: {} ({})", prefix, offset, opcode, comment),
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
