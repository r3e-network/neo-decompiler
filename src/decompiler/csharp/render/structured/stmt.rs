use std::collections::{BTreeMap, BTreeSet};

use crate::decompiler::analysis::method_contracts::ReturnBehavior;
use crate::decompiler::analysis::types::ValueType;
use crate::decompiler::cfg::method_body::{SourceMap, StatementId, SymbolInfo};
use crate::decompiler::csharp::helpers::{
    format_vm_assertion, sanitize_csharp_identifier, VM_ASSERT_MESSAGE_HELPER, VM_EXCEPTION_TYPE,
};
use crate::decompiler::ir::{BinOp, Block, ControlFlow, Expr, Stmt};

use super::expr::{render_expr, render_vm_condition, ExprContext};
use super::plan::{DeclarationPlan, ScopeId, ScopeTree};

const INDENT: &str = "    ";

#[path = "stmt_values.rs"]
mod stmt_values;

#[cfg_attr(not(test), allow(dead_code))]
pub(in crate::decompiler::csharp::render) fn render_block(
    block: &Block,
    plan: &DeclarationPlan,
    symbols: &BTreeMap<String, SymbolInfo>,
    return_behavior: ReturnBehavior,
    inline_single_use_temps: bool,
) -> String {
    render_block_with_trace(
        block,
        plan,
        symbols,
        return_behavior,
        inline_single_use_temps,
        VM_ASSERT_MESSAGE_HELPER,
        VM_EXCEPTION_TYPE,
        None,
        None,
        &BTreeMap::new(),
        &BTreeMap::new(),
        None,
        None,
        &[],
    )
}

#[allow(clippy::too_many_arguments)]
pub(in crate::decompiler::csharp::render) fn render_block_with_trace(
    block: &Block,
    plan: &DeclarationPlan,
    symbols: &BTreeMap<String, SymbolInfo>,
    return_behavior: ReturnBehavior,
    inline_single_use_temps: bool,
    assert_message_helper: &str,
    vm_exception_type: &str,
    bare_throw_helper_call: Option<&str>,
    unpack_packstruct_helper_call: Option<&str>,
    tagged_opcode_helper_calls: &BTreeMap<(u8, u8), String>,
    internal_call_return_types: &BTreeMap<usize, String>,
    return_type: Option<&str>,
    source_map: Option<&SourceMap>,
    instructions: &[crate::instruction::Instruction],
) -> String {
    let mut reserved_names = symbols.keys().cloned().collect::<BTreeSet<_>>();
    reserved_names.extend(
        plan.declarations
            .values()
            .map(|declaration| declaration.emitted_name.clone()),
    );
    let mut renderer = StatementRenderer {
        plan,
        expressions: ExprContext::for_block(block, symbols, inline_single_use_temps)
            .with_emitted_names(
                plan.declarations
                    .iter()
                    .map(|(name, declaration)| (name.clone(), declaration.emitted_name.clone()))
                    .collect(),
            )
            .with_unpack_packstruct_helper_call(unpack_packstruct_helper_call)
            .with_tagged_opcode_helper_calls(tagged_opcode_helper_calls)
            .with_internal_call_return_types(internal_call_return_types),
        return_behavior,
        return_type,
        scopes: ScopeCursor::new(&plan.scopes),
        reserved_names,
        next_switch_value: 0,
        next_exception: 0,
        assert_message_helper,
        vm_exception_type,
        bare_throw_helper_call,
        source_map,
        instructions,
        next_statement_id: 0,
    };
    let root = plan.scopes.root();
    renderer.render_block_at(block, root, 0, true).join("\n")
}

#[cfg_attr(not(test), allow(dead_code))]
pub(in crate::decompiler::csharp::render) fn terminates(block: &Block) -> bool {
    block.stmts.iter().any(terminates_statement)
}

fn terminates_statement(statement: &Stmt) -> bool {
    match statement {
        Stmt::Return(_)
        | Stmt::Throw(_)
        | Stmt::Abort(_)
        | Stmt::Break
        | Stmt::Continue
        | Stmt::Goto(_) => true,
        Stmt::ControlFlow(control) => match control.as_ref() {
            ControlFlow::If {
                then_branch,
                else_branch,
                ..
            } => else_branch
                .as_ref()
                .is_some_and(|else_branch| terminates(then_branch) && terminates(else_branch)),
            ControlFlow::TryCatch {
                try_body,
                catch_body,
                finally_body,
                ..
            } => {
                finally_body.as_ref().is_some_and(terminates)
                    || (terminates(try_body) && catch_body.as_ref().is_none_or(terminates))
            }
            ControlFlow::Switch { cases, default, .. } => {
                default.as_ref().is_some_and(terminates)
                    && cases.iter().all(|(_, body)| terminates(body))
            }
            ControlFlow::While { .. } | ControlFlow::DoWhile { .. } | ControlFlow::For { .. } => {
                false
            }
        },
        Stmt::Assign { .. }
        | Stmt::Assert { .. }
        | Stmt::ExprStmt(_)
        | Stmt::Comment(_)
        | Stmt::Label(_) => false,
    }
}

struct StatementRenderer<'a> {
    plan: &'a DeclarationPlan,
    expressions: ExprContext,
    return_behavior: ReturnBehavior,
    return_type: Option<&'a str>,
    scopes: ScopeCursor<'a>,
    reserved_names: BTreeSet<String>,
    next_switch_value: usize,
    next_exception: usize,
    assert_message_helper: &'a str,
    vm_exception_type: &'a str,
    bare_throw_helper_call: Option<&'a str>,
    source_map: Option<&'a SourceMap>,
    instructions: &'a [crate::instruction::Instruction],
    next_statement_id: u32,
}

impl StatementRenderer<'_> {
    fn render_block_at(
        &mut self,
        block: &Block,
        scope: ScopeId,
        indent: usize,
        is_method_body: bool,
    ) -> Vec<String> {
        let mut lines = self.hoisted_declarations(scope, indent);
        let omitted_void_return = (is_method_body && self.return_behavior == ReturnBehavior::Void)
            .then(|| {
                block
                    .stmts
                    .iter()
                    .rposition(|statement| !matches!(statement, Stmt::Comment(_)))
            })
            .flatten()
            .filter(|index| matches!(block.stmts[*index], Stmt::Return(None)));

        for (index, statement) in block.stmts.iter().enumerate() {
            if omitted_void_return == Some(index) {
                continue;
            }
            self.render_trace_comments(indent, &mut lines);
            self.render_statement(statement, scope, indent, &mut lines);
        }
        lines
    }

    fn render_trace_comments(&mut self, indent: usize, lines: &mut Vec<String>) {
        let id = StatementId(self.next_statement_id);
        self.next_statement_id += 1;
        let Some(source_map) = self.source_map else {
            return;
        };
        let Some(origins) = source_map.statement_origins.get(&id) else {
            return;
        };
        for offset in origins {
            if let Some(instruction) = self
                .instructions
                .iter()
                .find(|instruction| instruction.offset == *offset)
            {
                lines.push(line(
                    indent,
                    format!("// {offset:04X}: {}", instruction.opcode.mnemonic()),
                ));
            }
        }
    }

    fn render_statement(
        &mut self,
        statement: &Stmt,
        scope: ScopeId,
        indent: usize,
        lines: &mut Vec<String>,
    ) {
        match statement {
            Stmt::Assign { target, value } => {
                if self.expressions.is_inlined(target) {
                    return;
                }
                lines.push(line(indent, self.render_assignment(target, value, true)));
            }
            Stmt::Return(Some(value)) => lines.push(line(indent, self.render_return(value))),
            Stmt::Return(None) => lines.push(line(
                indent,
                if self.return_behavior == ReturnBehavior::Void {
                    "return;".to_string()
                } else {
                    "return default;".to_string()
                },
            )),
            Stmt::Throw(None) if self.bare_throw_helper_call.is_some() => {
                lines.push(line(
                    indent,
                    format!("{}();", self.bare_throw_helper_call.unwrap()),
                ));
                lines.push(line(indent, self.render_vm_throw(None)));
            }
            Stmt::Throw(value) => lines.push(line(indent, self.render_vm_throw(value.as_ref()))),
            Stmt::Abort(message) => lines.push(line(
                indent,
                self.render_exception("InvalidOperationException", message.as_ref()),
            )),
            Stmt::Assert { condition, message } => lines.push(line(
                indent,
                format_vm_assertion(
                    &render_vm_condition(condition, &self.expressions),
                    message
                        .as_ref()
                        .map(|message| render_expr(message, &self.expressions))
                        .as_deref(),
                    self.assert_message_helper,
                ),
            )),
            Stmt::ExprStmt(expression) => {
                lines.push(line(indent, self.render_expression_statement(expression)));
            }
            Stmt::Comment(comment) => lines.push(line(indent, format!("// {comment}"))),
            Stmt::Break => lines.push(line(indent, "break;")),
            Stmt::Continue => lines.push(line(indent, "continue;")),
            Stmt::Label(label) => lines.push(line(indent, format!("label_{}:", label.0))),
            Stmt::Goto(label) => {
                lines.push(line(indent, format!("goto label_{};", label.0)));
            }
            Stmt::ControlFlow(control) => {
                self.render_control_flow(control, scope, indent, lines);
            }
        }
    }

    fn render_control_flow(
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

fn typed_array_csharp_type(element_type: ValueType) -> &'static str {
    match element_type {
        ValueType::Boolean => "bool[]",
        ValueType::Integer => "BigInteger[]",
        ValueType::ByteString => "ByteString[]",
        ValueType::Buffer => "byte[][]",
        ValueType::Array | ValueType::Struct => "object[][]",
        ValueType::Map => "Map<object, object>[]",
        ValueType::Unknown
        | ValueType::Any
        | ValueType::Null
        | ValueType::InteropInterface
        | ValueType::Pointer => "object[]",
    }
}

struct ScopeCursor<'a> {
    tree: &'a ScopeTree,
    next: usize,
}

impl<'a> ScopeCursor<'a> {
    fn new(tree: &'a ScopeTree) -> Self {
        Self { tree, next: 1 }
    }

    fn next_child(&mut self, parent: ScopeId) -> ScopeId {
        let scope = self
            .tree
            .scope_at(self.next)
            .expect("statement traversal must match planned structured scopes");
        self.next += 1;
        assert_eq!(
            self.tree.parent_of(scope),
            Some(parent),
            "statement traversal diverged from declaration planning"
        );
        scope
    }
}

fn replace_last_line(lines: &mut [String], indent: usize, replacement: impl AsRef<str>) {
    let last = lines
        .last_mut()
        .expect("a control-flow continuation must follow a closing brace");
    *last = line(indent, replacement.as_ref());
}

fn line(indent: usize, text: impl AsRef<str>) -> String {
    format!("{}{text}", INDENT.repeat(indent), text = text.as_ref())
}
