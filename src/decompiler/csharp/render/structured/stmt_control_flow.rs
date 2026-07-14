//! Structured control-flow statement rendering.

use crate::decompiler::analysis::types::ValueType;
use crate::decompiler::csharp::helpers::sanitize_csharp_identifier;
use crate::decompiler::ir::{BinOp, Block, ControlFlow, Expr, Stmt};

use super::super::expr::{render_expr, render_vm_condition};
use super::super::plan::ScopeId;
use super::{line, terminates, StatementRenderer};

impl StatementRenderer<'_> {
    pub(super) fn render_control_flow(
        &mut self,
        control: &ControlFlow,
        scope: ScopeId,
        indent: usize,
        lines: &mut Vec<String>,
    ) {
        match control {
            ControlFlow::If {
                condition,
                then_branch,
                else_branch,
            } => {
                lines.push(line(
                    indent,
                    format!(
                        "if ({}) {{",
                        render_vm_condition(condition, &self.expressions)
                    ),
                ));
                let then_scope = self.scopes.next_child(scope);
                lines.extend(self.render_block_at(then_branch, then_scope, indent + 1, false));
                if let Some(else_branch) = else_branch {
                    lines.push(line(indent, "} else {"));
                    let else_scope = self.scopes.next_child(scope);
                    lines.extend(self.render_block_at(else_branch, else_scope, indent + 1, false));
                }
                lines.push(line(indent, "}"));
            }
            ControlFlow::While { condition, body } => {
                lines.push(line(
                    indent,
                    format!(
                        "while ({}) {{",
                        render_vm_condition(condition, &self.expressions)
                    ),
                ));
                let body_scope = self.scopes.next_child(scope);
                lines.extend(self.render_block_at(body, body_scope, indent + 1, false));
                lines.push(line(indent, "}"));
            }
            ControlFlow::DoWhile { body, condition } => {
                lines.push(line(indent, "do {"));
                let body_scope = self.scopes.next_child(scope);
                lines.extend(self.render_block_at(body, body_scope, indent + 1, false));
                lines.push(line(
                    indent,
                    format!(
                        "}} while ({});",
                        render_vm_condition(condition, &self.expressions)
                    ),
                ));
            }
            ControlFlow::For {
                init,
                condition,
                update,
                body,
            } => self.render_for(
                init.as_deref(),
                condition.as_ref(),
                update.as_ref(),
                body,
                scope,
                indent,
                lines,
            ),
            ControlFlow::TryCatch {
                try_body,
                catch_var,
                catch_body,
                finally_body,
            } => {
                lines.push(line(indent, "try {"));
                let try_scope = self.scopes.next_child(scope);
                lines.extend(self.render_block_at(try_body, try_scope, indent + 1, false));
                lines.push(line(indent, "}"));

                if let Some(catch_body) = catch_body {
                    let transport = self.fresh_reserved_name("__caughtException");
                    let payload_pattern = self.fresh_reserved_name("__vmException");
                    let header = format!("catch (Exception {transport}) {{");
                    replace_last_line(lines, indent, format!("}} {header}"));
                    if let Some(name) = catch_var {
                        let payload = sanitize_csharp_identifier(name);
                        lines.push(line(
                            indent + 1,
                            format!(
                                "dynamic {payload} = {transport} is {} {payload_pattern} ? {payload_pattern}.Payload : {transport}.Message;",
                                self.vm_exception_type
                            ),
                        ));
                    }
                    let catch_scope = self.scopes.next_child(scope);
                    lines.extend(self.render_block_at(catch_body, catch_scope, indent + 1, false));
                    lines.push(line(indent, "}"));
                }

                if let Some(finally_body) = finally_body {
                    replace_last_line(lines, indent, "} finally {");
                    let finally_scope = self.scopes.next_child(scope);
                    lines.extend(self.render_block_at(
                        finally_body,
                        finally_scope,
                        indent + 1,
                        false,
                    ));
                    lines.push(line(indent, "}"));
                }
            }
            ControlFlow::Switch {
                expr,
                cases,
                default,
            } => {
                lines.push(line(
                    indent,
                    format!("switch ({}) {{", render_expr(expr, &self.expressions)),
                ));
                let guarded_cases = matches!(
                    self.expressions.value_type(expr),
                    ValueType::Integer | ValueType::ByteString
                );
                for (case, body) in cases {
                    let case_label = match case {
                        Expr::Literal(_) if !guarded_cases => {
                            format!("case {}: {{", render_expr(case, &self.expressions))
                        }
                        _ => {
                            let candidate = self.fresh_switch_value();
                            let predicate =
                                Expr::binary(BinOp::Eq, Expr::var(candidate.clone()), case.clone());
                            format!(
                                "case var {candidate} when {}: {{",
                                render_expr(&predicate, &self.expressions)
                            )
                        }
                    };
                    lines.push(line(indent + 1, case_label));
                    let case_scope = self.scopes.next_child(scope);
                    lines.extend(self.render_block_at(body, case_scope, indent + 2, false));
                    if !terminates(body) {
                        lines.push(line(indent + 2, "break;"));
                    }
                    lines.push(line(indent + 1, "}"));
                }
                if let Some(body) = default {
                    lines.push(line(indent + 1, "default: {"));
                    let default_scope = self.scopes.next_child(scope);
                    lines.extend(self.render_block_at(body, default_scope, indent + 2, false));
                    if !terminates(body) {
                        lines.push(line(indent + 2, "break;"));
                    }
                    lines.push(line(indent + 1, "}"));
                }
                lines.push(line(indent, "}"));
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_for(
        &mut self,
        init: Option<&Stmt>,
        condition: Option<&Expr>,
        update: Option<&Expr>,
        body: &Block,
        parent_scope: ScopeId,
        indent: usize,
        lines: &mut Vec<String>,
    ) {
        let loop_scope = self.scopes.next_child(parent_scope);
        let declarations = self.hoisted_declarations(loop_scope, indent + 1);
        let wrapped = !declarations.is_empty();
        let loop_indent = if wrapped { indent + 1 } else { indent };
        if wrapped {
            lines.push(line(indent, "{"));
            lines.extend(declarations);
        }

        let init = init.map_or_else(String::new, |statement| {
            self.render_for_initializer(statement)
        });
        let condition = condition
            .map(|expression| render_vm_condition(expression, &self.expressions))
            .unwrap_or_default();
        let update = update
            .map(|expression| self.render_for_update(expression))
            .unwrap_or_default();
        lines.push(line(
            loop_indent,
            format!("for ({init}; {condition}; {update}) {{"),
        ));
        let body_scope = self.scopes.next_child(loop_scope);
        lines.extend(self.render_block_at(body, body_scope, loop_indent + 1, false));
        lines.push(line(loop_indent, "}"));

        if wrapped {
            lines.push(line(indent, "}"));
        }
    }

    fn fresh_switch_value(&mut self) -> String {
        loop {
            let candidate = format!("__switchValue{}", self.next_switch_value);
            self.next_switch_value += 1;
            if self.reserved_names.insert(candidate.clone()) {
                return candidate;
            }
        }
    }

    fn fresh_reserved_name(&mut self, prefix: &str) -> String {
        loop {
            let candidate = format!("{prefix}{}", self.next_exception);
            self.next_exception += 1;
            if self.reserved_names.insert(candidate.clone()) {
                return candidate;
            }
        }
    }
}

fn replace_last_line(lines: &mut [String], indent: usize, replacement: impl AsRef<str>) {
    let last = lines
        .last_mut()
        .expect("a control-flow continuation must follow a closing brace");
    *last = line(indent, replacement.as_ref());
}
