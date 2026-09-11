//! High-level dialect rendering of the typed IR.
//!
//! Mirrors the stack-emitter high-level view syntax (`let` declarations,
//! compact conditions, `+=` increments) while driving control flow from the
//! structured IR instead of string-pattern postprocess.

use std::collections::BTreeSet;
use std::fmt::Write;

use super::super::control_flow::ControlFlow;
use super::super::expression::{BinOp, Expr, UnaryOp};
use super::super::statement::{Block, Stmt};
use super::expr::render_expr;

const INDENT: &str = "    ";

/// Render a structured IR block in the high-level dialect.
///
/// `base_indent` is the indent level *inside* the method body (typically 2,
/// matching `fn name() {` + one level of body statements at column 8).
#[must_use]
pub fn render_high_level_block(block: &Block, base_indent: usize) -> String {
    let mut declared = BTreeSet::new();
    let mut out = String::new();
    render_block_into(block, base_indent, &mut declared, &mut out);
    out
}

fn render_block_into(
    block: &Block,
    indent: usize,
    declared: &mut BTreeSet<String>,
    out: &mut String,
) {
    for (index, statement) in block.stmts.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        render_stmt_into(statement, indent, declared, out);
    }
}

fn render_stmt_into(stmt: &Stmt, indent: usize, declared: &mut BTreeSet<String>, out: &mut String) {
    let prefix = INDENT.repeat(indent);
    match stmt {
        Stmt::Assign { target, value } => {
            // `x = x++` / `x = x--` is a post-increment store: emit the
            // statement form, never `let x = x++`.
            if is_self_inc_dec(target, value) {
                let op = match value {
                    Expr::Unary {
                        op: UnaryOp::Inc, ..
                    } => "++",
                    _ => "--",
                };
                let _ = write!(out, "{prefix}{target}{op};");
                declared.insert(target.clone());
                return;
            }
            let already = declared.contains(target.as_str());
            let keyword = if already { "" } else { "let " };
            if !already {
                declared.insert(target.clone());
            }
            // `x = x + 1` / `x = x - 1` → compound assign in the high-level view.
            if already {
                if let Some((op, rhs)) = compound_delta(target, value) {
                    let _ = write!(out, "{prefix}{target} {op} {};", hl_expr(rhs));
                } else {
                    let _ = write!(out, "{prefix}{target} = {};", hl_expr(value));
                }
            } else {
                let _ = write!(out, "{prefix}{keyword}{target} = {};", hl_expr(value));
            }
        }
        Stmt::Return(Some(expr)) => {
            let _ = write!(out, "{prefix}return {};", hl_expr(expr));
        }
        Stmt::Return(None) => {
            let _ = write!(out, "{prefix}return;");
        }
        Stmt::Throw(Some(expr)) => {
            let _ = write!(out, "{prefix}throw({});", hl_expr(expr));
        }
        Stmt::Throw(None) => {
            let _ = write!(out, "{prefix}throw();");
        }
        Stmt::Abort(Some(message)) => {
            let _ = write!(out, "{prefix}abort({});", hl_expr(message));
        }
        Stmt::Abort(None) => {
            let _ = write!(out, "{prefix}abort();");
        }
        Stmt::Assert {
            condition,
            message: Some(message),
        } => {
            let _ = write!(
                out,
                "{prefix}assert({}, {});",
                hl_expr(condition),
                hl_expr(message)
            );
        }
        Stmt::Assert {
            condition,
            message: None,
        } => {
            let _ = write!(out, "{prefix}assert({});", hl_expr(condition));
        }
        Stmt::ExprStmt(expr) => {
            let _ = write!(out, "{prefix}{};", hl_expr(expr));
        }
        Stmt::Comment(text) => {
            let _ = write!(out, "{prefix}// {text}");
        }
        Stmt::Break => {
            let _ = write!(out, "{prefix}break;");
        }
        Stmt::Continue => {
            let _ = write!(out, "{prefix}continue;");
        }
        Stmt::Label(label) => {
            let _ = write!(out, "{prefix}label_{}:", label.0);
        }
        Stmt::Goto(label) => {
            let _ = write!(out, "{prefix}goto label_{};", label.0);
        }
        Stmt::ControlFlow(cf) => {
            render_control_flow_into(cf, indent, declared, out);
        }
    }
}

/// `target = target++` / `target = target--`.
fn is_self_inc_dec(target: &str, value: &Expr) -> bool {
    matches!(
        value,
        Expr::Unary {
            op: UnaryOp::Inc | UnaryOp::Dec,
            operand,
        } if matches!(operand.as_ref(), Expr::Variable(name) if name == target)
    )
}

/// `target = target + 1` → `("+=", 1)`; `target = target - 1` → `("-=", 1)`.
///
/// Returns the compound operator together with the right-hand side so callers
/// never re-match the expression (and never hit an unreachable branch).
fn compound_delta<'a>(target: &str, value: &'a Expr) -> Option<(&'static str, &'a Expr)> {
    let Expr::Binary { op, left, right } = value else {
        return None;
    };
    let Expr::Variable(name) = left.as_ref() else {
        return None;
    };
    if name != target {
        return None;
    }
    if !matches!(right.as_ref(), Expr::Literal(_)) {
        return None;
    }
    match op {
        BinOp::Add => Some(("+=", right.as_ref())),
        BinOp::Sub => Some(("-=", right.as_ref())),
        _ => None,
    }
}

fn render_control_flow_into(
    cf: &ControlFlow,
    indent: usize,
    declared: &mut BTreeSet<String>,
    out: &mut String,
) {
    let prefix = INDENT.repeat(indent);
    match cf {
        ControlFlow::If {
            condition,
            then_branch,
            else_branch,
        } => {
            let _ = writeln!(out, "{prefix}if {} {{", hl_condition(condition));
            push_hl_body(out, then_branch, indent + 1, declared, &prefix);
            if let Some(else_branch) = else_branch {
                let _ = write!(out, " else {{");
                push_hl_body(out, else_branch, indent + 1, declared, &prefix);
            }
        }
        ControlFlow::While { condition, body } => {
            let _ = writeln!(out, "{prefix}while {} {{", hl_condition(condition));
            push_hl_body(out, body, indent + 1, declared, &prefix);
        }
        ControlFlow::DoWhile { body, condition } => {
            let _ = writeln!(out, "{prefix}do {{");
            push_hl_body(out, body, indent + 1, declared, &prefix);
            let _ = write!(out, " while {};", hl_condition(condition));
        }
        ControlFlow::For {
            init,
            condition,
            update,
            body,
        } => {
            let init_str = init.as_ref().map_or_else(String::new, |statement| {
                // High-level for-init keeps the `let`/assignment form without a
                // trailing semicolon inside the for header.
                let mut tmp_declared = declared.clone();
                let mut rendered = String::new();
                render_for_init_into(statement, &mut tmp_declared, &mut rendered);
                // Declarations established in the for-init are visible in the body.
                *declared = tmp_declared;
                rendered
            });
            let cond_str = condition.as_ref().map_or_else(String::new, hl_condition);
            let update_str = update.as_ref().map_or_else(String::new, hl_for_update);
            let _ = writeln!(out, "{prefix}for ({init_str}; {cond_str}; {update_str}) {{");
            push_hl_body(out, body, indent + 1, declared, &prefix);
        }
        ControlFlow::TryCatch {
            try_body,
            catch_var,
            catch_body,
            finally_body,
        } => {
            let _ = writeln!(out, "{prefix}try {{");
            push_hl_body(out, try_body, indent + 1, declared, &prefix);
            if let Some(catch_body) = catch_body {
                match catch_var {
                    Some(name) => {
                        declared.insert(name.clone());
                        let _ = write!(out, " catch {{");
                        out.push('\n');
                        let _ = writeln!(out, "{prefix}    let {name} = exception;");
                        render_block_into(catch_body, indent + 1, declared, out);
                        out.push('\n');
                        out.push_str(&prefix);
                        out.push('}');
                    }
                    None => {
                        let _ = write!(out, " catch {{");
                        push_hl_body(out, catch_body, indent + 1, declared, &prefix);
                    }
                }
            }
            if let Some(finally_body) = finally_body {
                let _ = write!(out, " finally {{");
                push_hl_body(out, finally_body, indent + 1, declared, &prefix);
            }
        }
        ControlFlow::Switch {
            expr,
            cases,
            default,
        } => {
            let _ = writeln!(out, "{prefix}switch {} {{", hl_parens(hl_expr(expr)));
            for (case_index, (case_expr, case_body)) in cases.iter().enumerate() {
                if case_index > 0 {
                    out.push('\n');
                }
                let _ = writeln!(
                    out,
                    "{}case {} {{",
                    INDENT.repeat(indent + 1),
                    hl_expr(case_expr)
                );
                if !case_body.stmts.is_empty() {
                    render_block_into(case_body, indent + 2, declared, out);
                    out.push('\n');
                }
                let _ = write!(out, "{}}}", INDENT.repeat(indent + 1));
            }
            if let Some(default_body) = default {
                out.push('\n');
                let _ = writeln!(out, "{}default {{", INDENT.repeat(indent + 1));
                if !default_body.stmts.is_empty() {
                    render_block_into(default_body, indent + 2, declared, out);
                    out.push('\n');
                }
                let _ = write!(out, "{}}}", INDENT.repeat(indent + 1));
            }
            let _ = write!(out, "\n{prefix}}}");
        }
    }
}

fn render_for_init_into(stmt: &Stmt, declared: &mut BTreeSet<String>, out: &mut String) {
    match stmt {
        Stmt::Assign { target, value } => {
            let already = declared.contains(target.as_str());
            if !already {
                declared.insert(target.clone());
                let _ = write!(out, "let {target} = {}", hl_expr(value));
            } else {
                let _ = write!(out, "{target} = {}", hl_expr(value));
            }
        }
        Stmt::ExprStmt(expression) => {
            out.push_str(&hl_expr(expression));
        }
        _ => {}
    }
}

/// High-level expression rendering. Binary nodes omit the analysis dialect's
/// always-on outer parentheses; operands still parenthesize when nested.
fn hl_expr(expr: &Expr) -> String {
    match expr {
        Expr::Binary { op, left, right } => {
            format!("{} {} {}", hl_operand(left), op, hl_operand(right))
        }
        other => render_expr(other),
    }
}

fn hl_operand(expr: &Expr) -> String {
    match expr {
        Expr::Binary { .. } | Expr::Ternary { .. } => format!("({})", hl_expr(expr)),
        other => hl_expr(other),
    }
}

/// High-level conditions omit the extra outer parentheses the analysis IR
/// dialect always adds around binary expressions.
fn hl_condition(expr: &Expr) -> String {
    match expr {
        Expr::Binary { .. } | Expr::Ternary { .. } | Expr::Unary { .. } => hl_expr(expr),
        _ => hl_parens(hl_expr(expr)),
    }
}

fn hl_parens(source: String) -> String {
    if source.starts_with('(') && source.ends_with(')') && balanced_outer_parens(&source) {
        source
    } else {
        format!("({source})")
    }
}

fn balanced_outer_parens(source: &str) -> bool {
    let bytes = source.as_bytes();
    if bytes.len() < 2 || bytes[0] != b'(' || *bytes.last().unwrap_or(&0) != b')' {
        return false;
    }
    let mut depth = 0usize;
    for (index, byte) in bytes.iter().enumerate() {
        match byte {
            b'(' => depth += 1,
            b')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 && index + 1 != bytes.len() {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

/// Append a high-level body and close the brace.
///
/// `prefix` is the indentation of the *opening* line, so the closing brace
/// lines up with it. Empty blocks emit no body lines (and no blank line)
/// but must still be indented — otherwise the brace lands in column 0
/// whenever the block is rendered below the top level.
fn push_hl_body(
    out: &mut String,
    block: &Block,
    indent: usize,
    declared: &mut BTreeSet<String>,
    prefix: &str,
) {
    if !block.stmts.is_empty() {
        if !out.ends_with('\n') {
            out.push('\n');
        }
        render_block_into(block, indent, declared, out);
        out.push('\n');
    }
    out.push_str(prefix);
    out.push('}');
}

/// `x++` in a for-update becomes high-level `x += 1`.
fn hl_for_update(expr: &Expr) -> String {
    match expr {
        Expr::Unary {
            op: UnaryOp::Inc,
            operand,
        } => format!("{} += 1", hl_expr(operand)),
        Expr::Unary {
            op: UnaryOp::Dec,
            operand,
        } => format!("{} -= 1", hl_expr(operand)),
        other => hl_update(other),
    }
}

/// `x++` stays `x++`; other updates render as plain expressions.
fn hl_update(expr: &Expr) -> String {
    match expr {
        Expr::Unary {
            op: UnaryOp::Inc,
            operand,
        } => format!("{}++", hl_expr(operand)),
        Expr::Unary {
            op: UnaryOp::Dec,
            operand,
        } => format!("{}--", hl_expr(operand)),
        other => hl_expr(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decompiler::ir::{
        Block as IrBlock, ControlFlow as IrCf, Expr as IrExpr, Stmt as IrStmt,
    };

    #[test]
    fn self_increment_assignment_becomes_statement() {
        let block = IrBlock::with_stmts(vec![IrStmt::assign(
            "static0",
            IrExpr::unary(UnaryOp::Inc, IrExpr::var("static0")),
        )]);
        let rendered = render_high_level_block(&block, 0);
        assert_eq!(rendered, "static0++;");
    }

    #[test]
    fn first_assignment_uses_let_and_subsequent_use_compound() {
        let block = IrBlock::with_stmts(vec![
            IrStmt::assign("loc0", IrExpr::int(0)),
            IrStmt::assign(
                "loc0",
                IrExpr::binary(BinOp::Add, IrExpr::var("loc0"), IrExpr::int(1)),
            ),
        ]);
        let rendered = render_high_level_block(&block, 0);
        assert_eq!(rendered, "let loc0 = 0;\nloc0 += 1;");
    }

    #[test]
    fn if_conditions_drop_redundant_outer_parens() {
        let block = IrBlock::with_stmts(vec![IrStmt::ControlFlow(Box::new(IrCf::if_then(
            IrExpr::binary(BinOp::Lt, IrExpr::var("x"), IrExpr::int(3)),
            IrBlock::with_stmts(vec![IrStmt::ret_void()]),
        )))]);
        let rendered = render_high_level_block(&block, 0);
        assert!(rendered.starts_with("if x < 3 {"), "{rendered}");
    }

    #[test]
    fn for_loops_use_let_init_and_increment() {
        let block = IrBlock::with_stmts(vec![IrStmt::ControlFlow(Box::new(IrCf::for_loop(
            Some(IrStmt::assign("i", IrExpr::int(0))),
            Some(IrExpr::binary(BinOp::Lt, IrExpr::var("i"), IrExpr::int(3))),
            Some(IrExpr::unary(UnaryOp::Inc, IrExpr::var("i"))),
            IrBlock::new(),
        )))]);
        let rendered = render_high_level_block(&block, 0);
        assert!(
            rendered.contains("for (let i = 0; i < 3; i += 1) {"),
            "{rendered}"
        );
    }

    #[test]
    fn control_flow_closes_each_high_level_block_once() {
        let block = IrBlock::with_stmts(vec![IrStmt::ControlFlow(Box::new(IrCf::If {
            condition: IrExpr::var("flag"),
            then_branch: IrBlock::with_stmts(vec![IrStmt::ret_void()]),
            else_branch: Some(IrBlock::with_stmts(vec![IrStmt::Break])),
        }))]);
        let rendered = render_high_level_block(&block, 0);
        assert_eq!(rendered.matches('{').count(), rendered.matches('}').count());
        assert!(rendered.contains("} else {"), "{rendered}");
    }

    #[test]
    fn named_catch_variable_closes_its_brace() {
        use crate::decompiler::ir::ControlFlow as IrCf;
        let block = IrBlock::with_stmts(vec![IrStmt::ControlFlow(Box::new(IrCf::TryCatch {
            try_body: IrBlock::with_stmts(vec![IrStmt::ret_void()]),
            catch_var: Some("e".to_string()),
            catch_body: Some(IrBlock::with_stmts(vec![IrStmt::ret_void()])),
            finally_body: None,
        }))]);
        let rendered = render_high_level_block(&block, 0);
        assert_eq!(
            rendered.matches('{').count(),
            rendered.matches('}').count(),
            "named catch block is not brace-balanced:\n{rendered}"
        );
        assert!(rendered.contains("let e = exception;"), "{rendered}");
        assert!(rendered.contains("} catch {"), "{rendered}");
    }

    // ---- property tests: all ControlFlow arms ----

    fn brace_balanced(s: &str) -> bool {
        s.matches('{').count() == s.matches('}').count()
    }

    #[test]
    fn switch_empty_no_cases_brace_balance() {
        let block = IrBlock::with_stmts(vec![IrStmt::ControlFlow(Box::new(IrCf::Switch {
            expr: IrExpr::var("x"),
            cases: vec![],
            default: None,
        }))]);
        let rendered = render_high_level_block(&block, 0);
        assert!(brace_balanced(&rendered), "switch(empty): {rendered}");
    }

    #[test]
    fn switch_single_case_brace_balance() {
        let block = IrBlock::with_stmts(vec![IrStmt::ControlFlow(Box::new(IrCf::Switch {
            expr: IrExpr::var("x"),
            cases: vec![(IrExpr::int(1), IrBlock::with_stmts(vec![IrStmt::Break]))],
            default: None,
        }))]);
        let rendered = render_high_level_block(&block, 0);
        assert!(brace_balanced(&rendered), "switch(1 case): {rendered}");
    }

    #[test]
    fn switch_multi_case_with_default_brace_balance() {
        let block = IrBlock::with_stmts(vec![IrStmt::ControlFlow(Box::new(IrCf::Switch {
            expr: IrExpr::var("v"),
            cases: vec![
                (IrExpr::int(0), IrBlock::with_stmts(vec![IrStmt::ret_void()])),
                (IrExpr::int(1), IrBlock::with_stmts(vec![IrStmt::Break])),
                (
                    IrExpr::int(2),
                    IrBlock::with_stmts(vec![IrStmt::assign("v", IrExpr::int(99))]),
                ),
            ],
            default: Some(IrBlock::with_stmts(vec![IrStmt::ret_void()])),
        }))]);
        let rendered = render_high_level_block(&block, 0);
        assert!(
            brace_balanced(&rendered),
            "switch(3 cases + default): {rendered}"
        );
    }

    #[test]
    fn switch_empty_case_bodies_brace_balance() {
        let block = IrBlock::with_stmts(vec![IrStmt::ControlFlow(Box::new(IrCf::Switch {
            expr: IrExpr::var("x"),
            cases: vec![
                (IrExpr::int(0), IrBlock::new()),
                (IrExpr::int(1), IrBlock::new()),
            ],
            default: Some(IrBlock::new()),
        }))]);
        let rendered = render_high_level_block(&block, 0);
        assert!(
            brace_balanced(&rendered),
            "switch(empty bodies): {rendered}"
        );
    }

    // All 8 combinations of: catch_body present × catch_var present × finally_body present.
    fn try_combo(
        catch_body: Option<IrBlock>,
        catch_var: Option<&str>,
        finally_body: Option<IrBlock>,
    ) -> String {
        let block = IrBlock::with_stmts(vec![IrStmt::ControlFlow(Box::new(IrCf::TryCatch {
            try_body: IrBlock::with_stmts(vec![IrStmt::ret_void()]),
            catch_var: catch_var.map(str::to_string),
            catch_body,
            finally_body,
        }))]);
        render_high_level_block(&block, 0)
    }

    #[test]
    fn try_catch_finally_all_8_combinations_brace_balanced() {
        let body = || IrBlock::with_stmts(vec![IrStmt::ret_void()]);
        for (has_catch, has_var, has_finally) in
            (0u8..8).map(|n| (n & 4 != 0, n & 2 != 0, n & 1 != 0))
        {
            let catch_body = has_catch.then(body);
            let catch_var = (has_catch && has_var).then_some("e");
            let finally_body = has_finally.then(body);
            let rendered = try_combo(catch_body, catch_var, finally_body);
            assert!(
                brace_balanced(&rendered),
                "TryCatch combo catch={has_catch} var={has_var} finally={has_finally}:\n{rendered}"
            );
        }
    }

    #[test]
    fn deeply_nested_if_brace_balance() {
        let inner = IrBlock::with_stmts(vec![IrStmt::ret_void()]);
        let depth4 = IrBlock::with_stmts(vec![IrStmt::ControlFlow(Box::new(IrCf::if_then(
            IrExpr::var("d"),
            inner,
        )))]);
        let depth3 = IrBlock::with_stmts(vec![IrStmt::ControlFlow(Box::new(IrCf::if_then(
            IrExpr::var("c"),
            depth4,
        )))]);
        let depth2 = IrBlock::with_stmts(vec![IrStmt::ControlFlow(Box::new(IrCf::if_then(
            IrExpr::var("b"),
            depth3,
        )))]);
        let block = IrBlock::with_stmts(vec![IrStmt::ControlFlow(Box::new(IrCf::if_then(
            IrExpr::var("a"),
            depth2,
        )))]);
        let rendered = render_high_level_block(&block, 0);
        assert!(
            brace_balanced(&rendered),
            "deeply nested if: {rendered}"
        );
    }

    #[test]
    fn do_while_no_trailing_newline() {
        let block =
            IrBlock::with_stmts(vec![IrStmt::ControlFlow(Box::new(IrCf::DoWhile {
                body: IrBlock::with_stmts(vec![IrStmt::ret_void()]),
                condition: IrExpr::var("cond"),
            }))]);
        let rendered = render_high_level_block(&block, 0);
        assert!(!rendered.ends_with('\n'), "do-while trailing newline: {rendered:?}");
        assert!(brace_balanced(&rendered), "do-while brace balance: {rendered}");
    }

    #[test]
    fn for_loop_declaration_scoping_no_double_let() {
        // Variable declared in for-init must not get a second `let` in the body.
        let block = IrBlock::with_stmts(vec![IrStmt::ControlFlow(Box::new(IrCf::for_loop(
            Some(IrStmt::assign("i", IrExpr::int(0))),
            Some(IrExpr::binary(BinOp::Lt, IrExpr::var("i"), IrExpr::int(10))),
            Some(IrExpr::unary(UnaryOp::Inc, IrExpr::var("i"))),
            IrBlock::with_stmts(vec![IrStmt::assign(
                "i",
                IrExpr::binary(BinOp::Add, IrExpr::var("i"), IrExpr::int(1)),
            )]),
        )))]);
        let rendered = render_high_level_block(&block, 0);
        // Body re-assignment to `i` should be compound `i += 1`, not `let i = ...`.
        assert!(
            !rendered.contains("let i = i"),
            "for-body should not re-declare i: {rendered}"
        );
        assert!(brace_balanced(&rendered), "{rendered}");
    }

    #[test]
    fn while_loop_brace_balance() {
        let block =
            IrBlock::with_stmts(vec![IrStmt::ControlFlow(Box::new(IrCf::While {
                condition: IrExpr::binary(BinOp::Lt, IrExpr::var("n"), IrExpr::int(10)),
                body: IrBlock::with_stmts(vec![IrStmt::assign(
                    "n",
                    IrExpr::binary(BinOp::Add, IrExpr::var("n"), IrExpr::int(1)),
                )]),
            }))]);
        let rendered = render_high_level_block(&block, 0);
        assert!(brace_balanced(&rendered), "{rendered}");
    }

    #[test]
    fn empty_high_level_body_keeps_closing_indent() {
        let block = IrBlock::with_stmts(vec![IrStmt::ControlFlow(Box::new(IrCf::if_then(
            IrExpr::var("flag"),
            IrBlock::new(),
        )))]);
        let rendered = render_high_level_block(&block, 1);
        assert_eq!(rendered, "    if (flag) {\n    }");
    }
}
