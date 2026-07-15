use std::collections::{BTreeMap, BTreeSet};

use crate::decompiler::analysis::method_contracts::ReturnBehavior;
use crate::decompiler::analysis::types::ValueType;
use crate::decompiler::cfg::method_body::{SourceMap, StatementId, SymbolInfo};
use crate::decompiler::csharp::helpers::{
    format_vm_assertion, VM_ASSERT_MESSAGE_HELPER, VM_EXCEPTION_TYPE,
};
use crate::decompiler::csharp::render::events::EventSignatures;
use crate::decompiler::ir::{Block, ControlFlow, Expr, Stmt};

use super::expr::{render_expr, render_vm_condition, ExprContext};
use super::plan::{DeclarationPlan, ScopeId, ScopeTree};

const INDENT: &str = "    ";

#[path = "stmt_control_flow.rs"]
mod stmt_control_flow;
#[path = "stmt_facts.rs"]
mod stmt_facts;
#[path = "stmt_foreach.rs"]
mod stmt_foreach;
#[path = "stmt_values.rs"]
mod stmt_values;

use stmt_facts::{update_definition_facts, DefinitionFacts};

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
        &BTreeMap::new(),
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
    event_signatures: &EventSignatures,
) -> String {
    let mut reserved_names = symbols.keys().cloned().collect::<BTreeSet<_>>();
    reserved_names.extend(
        plan.declarations
            .values()
            .map(|declaration| declaration.emitted_name.clone()),
    );
    let concrete_types = plan
        .parameter_types
        .iter()
        .filter(|(_, type_name)| !type_name.eq_ignore_ascii_case("dynamic"))
        .map(|(name, type_name)| (name.clone(), type_name.clone()))
        .chain(
            plan.declarations
                .iter()
                .filter(|(_, declaration)| !declaration.csharp_type.eq_ignore_ascii_case("dynamic"))
                .map(|(name, declaration)| (name.clone(), declaration.csharp_type.clone())),
        )
        .chain(
            plan.static_field_types
                .iter()
                .filter(|(_, type_name)| !type_name.eq_ignore_ascii_case("dynamic"))
                .map(|(name, type_name)| (name.clone(), type_name.clone())),
        )
        .collect::<BTreeMap<_, _>>();
    let mut renderer = StatementRenderer {
        plan,
        expressions: ExprContext::for_block(block, symbols, inline_single_use_temps)
            .with_concrete_types(&concrete_types)
            .with_typed_array_literals(plan.typed)
            .with_emitted_names(
                plan.declarations
                    .iter()
                    .map(|(name, declaration)| (name.clone(), declaration.emitted_name.clone()))
                    .collect(),
            )
            .with_unpack_packstruct_helper_call(unpack_packstruct_helper_call)
            .with_tagged_opcode_helper_calls(tagged_opcode_helper_calls)
            .with_internal_call_return_types(internal_call_return_types)
            .with_event_signatures(event_signatures),
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
        self.render_block_at_with_facts(
            block,
            scope,
            indent,
            is_method_body,
            &DefinitionFacts::new(),
        )
    }

    fn render_block_at_with_facts(
        &mut self,
        block: &Block,
        scope: ScopeId,
        indent: usize,
        is_method_body: bool,
        inherited_facts: &DefinitionFacts,
    ) -> Vec<String> {
        self.render_block_at_omitting(
            block,
            scope,
            indent,
            is_method_body,
            &BTreeSet::new(),
            inherited_facts,
        )
    }

    fn render_block_at_omitting(
        &mut self,
        block: &Block,
        scope: ScopeId,
        indent: usize,
        is_method_body: bool,
        omitted: &BTreeSet<usize>,
        inherited_facts: &DefinitionFacts,
    ) -> Vec<String> {
        let mut lines = self.hoisted_declarations(scope, indent);
        let mut facts = inherited_facts.clone();
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
            if omitted.contains(&index) {
                // Keep source-map statement IDs aligned even when a compiler-
                // generated extraction is represented by a foreach variable.
                self.next_statement_id += 1;
                continue;
            }
            if omitted_void_return == Some(index) {
                continue;
            }
            if self.should_omit_statement(statement) {
                // Keep source-map statement IDs aligned when a compiler-
                // generated Debug notification array is removed.
                self.next_statement_id += 1;
                continue;
            }
            self.render_trace_comments(indent, &mut lines);
            self.render_statement(statement, scope, indent, &mut lines, &facts);
            update_definition_facts(&mut facts, statement);
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
        facts: &DefinitionFacts,
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
                self.render_control_flow(control, scope, indent, lines, facts);
            }
        }
    }

    fn should_omit_statement(&self, statement: &Stmt) -> bool {
        matches!(
            statement,
            Stmt::Assign {
                target,
                value: Expr::Array(elements),
            } if (elements.len() == 1
                && self.expressions.is_debug_singleton_array_target(target))
                || self.expressions.is_event_array_target(target)
        )
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

fn line(indent: usize, text: impl AsRef<str>) -> String {
    format!("{}{text}", INDENT.repeat(indent), text = text.as_ref())
}
