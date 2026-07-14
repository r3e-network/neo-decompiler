use std::collections::BTreeSet;

use crate::decompiler::ir::{Block, ControlFlow, Expr, Literal, SemanticCallTarget, Stmt};

/// Find compiler-generated state temporaries used by the `Runtime.Debug`
/// lowering (`Notify("Debug", PACK(message))`).
pub(super) fn collect_debug_state_names(block: &Block) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    collect_block(block, &mut names);
    names
}

fn collect_block(block: &Block, names: &mut BTreeSet<String>) {
    for statement in &block.stmts {
        collect_statement(statement, names);
    }
}

fn collect_statement(statement: &Stmt, names: &mut BTreeSet<String>) {
    match statement {
        Stmt::Assign { value, .. } | Stmt::ExprStmt(value) => collect_expr(value, names),
        Stmt::Return(value) | Stmt::Throw(value) | Stmt::Abort(value) => {
            if let Some(value) = value {
                collect_expr(value, names);
            }
        }
        Stmt::Assert { condition, message } => {
            collect_expr(condition, names);
            if let Some(message) = message {
                collect_expr(message, names);
            }
        }
        Stmt::ControlFlow(control) => collect_control(control, names),
        Stmt::Comment(_) | Stmt::Break | Stmt::Continue | Stmt::Label(_) | Stmt::Goto(_) => {}
    }
}

fn collect_control(control: &ControlFlow, names: &mut BTreeSet<String>) {
    match control {
        ControlFlow::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_expr(condition, names);
            collect_block(then_branch, names);
            if let Some(else_branch) = else_branch {
                collect_block(else_branch, names);
            }
        }
        ControlFlow::While { condition, body } | ControlFlow::DoWhile { condition, body } => {
            collect_expr(condition, names);
            collect_block(body, names);
        }
        ControlFlow::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                collect_statement(init, names);
            }
            if let Some(condition) = condition {
                collect_expr(condition, names);
            }
            collect_block(body, names);
            if let Some(update) = update {
                collect_expr(update, names);
            }
        }
        ControlFlow::TryCatch {
            try_body,
            catch_body,
            finally_body,
            ..
        } => {
            collect_block(try_body, names);
            if let Some(catch_body) = catch_body {
                collect_block(catch_body, names);
            }
            if let Some(finally_body) = finally_body {
                collect_block(finally_body, names);
            }
        }
        ControlFlow::Switch {
            expr,
            cases,
            default,
        } => {
            collect_expr(expr, names);
            for (value, body) in cases {
                collect_expr(value, names);
                collect_block(body, names);
            }
            if let Some(default) = default {
                collect_block(default, names);
            }
        }
    }
}

fn collect_expr(expression: &Expr, names: &mut BTreeSet<String>) {
    if let Expr::Call {
        target: SemanticCallTarget::Syscall { hash, .. },
        args,
    } = expression
    {
        if *hash == 0x616F_0195 {
            let state = match args.as_slice() {
                [Expr::Literal(Literal::String(label)), Expr::Variable(name)]
                    if label == "Debug" =>
                {
                    Some(name)
                }
                [Expr::Literal(Literal::String(_)), Expr::Literal(Literal::String(label)), Expr::Variable(name)]
                    if label == "Debug" =>
                {
                    Some(name)
                }
                _ => None,
            };
            if let Some(name) = state {
                names.insert(name.clone());
            }
        }
    }
    match expression {
        Expr::Binary { left, right, .. } => {
            collect_expr(left, names);
            collect_expr(right, names);
        }
        Expr::Unary { operand, .. }
        | Expr::Cast { expr: operand, .. }
        | Expr::Convert { value: operand, .. }
        | Expr::IsType { value: operand, .. }
        | Expr::Member { base: operand, .. } => collect_expr(operand, names),
        Expr::Call { args, .. } | Expr::Array(args) | Expr::Struct(args) => {
            for argument in args {
                collect_expr(argument, names);
            }
        }
        Expr::Index { base, index } => {
            collect_expr(base, names);
            collect_expr(index, names);
        }
        Expr::NewArray { length, .. } => collect_expr(length, names),
        Expr::Map(pairs) => {
            for (key, value) in pairs {
                collect_expr(key, names);
                collect_expr(value, names);
            }
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_expr(condition, names);
            collect_expr(then_expr, names);
            collect_expr(else_expr, names);
        }
        Expr::Unknown | Expr::Variable(_) | Expr::StackTemp(_) | Expr::Literal(_) => {}
    }
}
