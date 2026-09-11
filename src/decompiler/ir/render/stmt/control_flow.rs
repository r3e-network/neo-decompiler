use super::super::super::control_flow::ControlFlow;
use super::super::super::statement::Block;
use super::super::expr::render_expr;

use super::{render_block, INDENT};

pub(super) fn render_control_flow(cf: &ControlFlow, indent: usize) -> String {
    let prefix = INDENT.repeat(indent);
    match cf {
        ControlFlow::If {
            condition,
            then_branch,
            else_branch,
        } => {
            let mut result = format!("{}if ({}) {{\n", prefix, render_expr(condition));
            push_body(&mut result, then_branch, indent + 1);
            if let Some(else_branch) = else_branch {
                result.push_str(&format!("{}}} else {{\n", prefix));
                push_body(&mut result, else_branch, indent + 1);
            }
            result.push_str(&format!("{}}}", prefix));
            result
        }
        ControlFlow::While { condition, body } => {
            let mut result = format!("{}while ({}) {{\n", prefix, render_expr(condition));
            push_body(&mut result, body, indent + 1);
            result.push_str(&format!("{}}}", prefix));
            result
        }
        ControlFlow::DoWhile { body, condition } => {
            let mut result = format!("{}do {{\n", prefix);
            push_body(&mut result, body, indent + 1);
            result.push_str(&format!("{}}} while ({});", prefix, render_expr(condition)));
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
                .map(|s| super::render_stmt(s, 0).trim_end_matches(';').to_string())
                .unwrap_or_default();
            let cond_str = condition.as_ref().map(render_expr).unwrap_or_default();
            let update_str = update.as_ref().map(render_expr).unwrap_or_default();

            let mut result = format!(
                "{}for ({}; {}; {}) {{\n",
                prefix, init_str, cond_str, update_str
            );
            push_body(&mut result, body, indent + 1);
            result.push_str(&format!("{}}}", prefix));
            result
        }
        ControlFlow::TryCatch {
            try_body,
            catch_var,
            catch_body,
            finally_body,
        } => {
            let mut result = format!("{}try {{\n", prefix);
            push_body(&mut result, try_body, indent + 1);
            result.push_str(&format!("{}}}", prefix));

            if let Some(catch_body) = catch_body {
                let catch_var_str = catch_var
                    .as_ref()
                    .map(|v| format!("({})", v))
                    .unwrap_or_default();
                result.push_str(&format!(" catch{} {{\n", catch_var_str));
                push_body(&mut result, catch_body, indent + 1);
                result.push_str(&format!("{}}}", prefix));
            }

            if let Some(finally_body) = finally_body {
                result.push_str(" finally {\n");
                push_body(&mut result, finally_body, indent + 1);
                result.push_str(&format!("{}}}", prefix));
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
                if !case_body.stmts.is_empty() {
                    result.push_str(&render_block(case_body, indent + 2));
                    result.push('\n');
                }
            }
            if let Some(default_body) = default {
                result.push_str(&format!("{}default:\n", INDENT.repeat(indent + 1)));
                if !default_body.stmts.is_empty() {
                    result.push_str(&render_block(default_body, indent + 2));
                    result.push('\n');
                }
            }
            result.push_str(&format!("{}}}", prefix));
            result
        }
    }
}

/// Append a control-flow body followed by the line break that precedes the
/// closing brace.
///
/// Empty blocks emit nothing so the caller's closing brace lands on the next
/// line instead of after a stray blank line. The caller owns the closing
/// `prefix` — writing it here too would double every nested brace indent.
fn push_body(result: &mut String, block: &Block, indent: usize) {
    if block.stmts.is_empty() {
        return;
    }
    result.push_str(&render_block(block, indent));
    result.push('\n');
}
