//! Conservative recovery of compiler-generated indexed `foreach` loops.

use std::collections::BTreeSet;

use crate::decompiler::analysis::types::ValueType;
use crate::decompiler::csharp::helpers::sanitize_csharp_identifier;
use crate::decompiler::ir::{Block, Expr, Intrinsic, Literal, SemanticCallTarget, Stmt, UnaryOp};
use crate::instruction::OpCode;

use super::super::expr::render_expr;
use super::super::plan::{DeclarationKind, ScopeId};
use super::{line, DefinitionFacts, StatementRenderer};

#[path = "stmt_foreach_guards.rs"]
mod guards;
use guards::{
    block_assigns_variable, block_has_opaque_calls, block_mentions_variable, block_writes_static,
};

impl StatementRenderer<'_> {
    pub(super) fn render_foreach(
        &mut self,
        pattern: ForeachPattern,
        body: &Block,
        loop_scope: ScopeId,
        indent: usize,
        lines: &mut Vec<String>,
        facts: &DefinitionFacts,
    ) {
        let item_name = sanitize_csharp_identifier(&pattern.item);
        let item_type = self
            .plan
            .declarations
            .get(&pattern.item)
            .map(|declaration| declaration.csharp_type.as_str())
            .unwrap_or("dynamic");
        let collection = render_expr(&Expr::var(pattern.collection), &self.expressions);
        lines.push(line(
            indent,
            format!("foreach ({item_type} {item_name} in {collection}) {{"),
        ));
        let body_scope = self.scopes.next_child(loop_scope);
        lines.extend(self.render_block_at_omitting(
            body,
            body_scope,
            indent + 1,
            false,
            &pattern.omitted,
            facts,
        ));
        lines.push(line(indent, "}"));
    }

    pub(super) fn detect_foreach(
        &self,
        init: Option<&Stmt>,
        condition: Option<&Expr>,
        update: Option<&Expr>,
        body: &Block,
        facts: &DefinitionFacts,
    ) -> Option<ForeachPattern> {
        let Some(Stmt::Assign {
            target: index,
            value: Expr::Literal(value),
        }) = init
        else {
            return None;
        };
        if !is_zero_literal(value) {
            return None;
        }

        let Some(Expr::Binary {
            op: crate::decompiler::ir::BinOp::Lt,
            left,
            right,
        }) = condition
        else {
            return None;
        };
        if !matches!(left.as_ref(), Expr::Variable(name) if name == index) {
            return None;
        }
        let Some(Expr::Unary {
            op: UnaryOp::Inc,
            operand,
        }) = update
        else {
            return None;
        };
        if !matches!(operand.as_ref(), Expr::Variable(name) if name == index) {
            return None;
        }

        let (collection, item, omitted_count, temporary) = match body.stmts.as_slice() {
            [Stmt::Assign {
                target: temporary,
                value,
            }, Stmt::Assign {
                target: item,
                value: Expr::Variable(source),
            }, ..]
                if source == temporary =>
            {
                (
                    indexed_collection(value, index)?,
                    item.clone(),
                    2,
                    Some(temporary.clone()),
                )
            }
            [Stmt::Assign {
                target: item,
                value,
            }, ..] => (indexed_collection(value, index)?, item.clone(), 1, None),
            _ => return None,
        };
        let collection_root = resolve_collection_alias(&collection, facts);
        if !is_collection_bound(right, &collection_root, facts) {
            return None;
        }

        let item_declaration = self.plan.declarations.get(&item)?;
        if item_declaration.kind != DeclarationKind::Inline || self.expressions.is_inlined(&item) {
            return None;
        }
        if let Some(temporary) = temporary.as_ref() {
            let temporary_declaration = self.plan.declarations.get(temporary)?;
            if temporary_declaration.kind != DeclarationKind::Inline
                || self.expressions.is_inlined(temporary)
            {
                return None;
            }
        }
        if !matches!(
            self.expressions.value_type(&Expr::var(&collection)),
            ValueType::Array | ValueType::Struct | ValueType::Buffer
        ) {
            return None;
        }

        let omitted = (0..omitted_count).collect::<BTreeSet<_>>();
        let remainder = Block::with_stmts(body.stmts[omitted_count..].to_vec());
        if block_mentions_variable(&remainder, index)
            || block_mentions_variable(&remainder, &collection)
            || temporary
                .as_deref()
                .is_some_and(|temporary| block_mentions_variable(&remainder, temporary))
            || block_assigns_variable(&remainder, &item)
            || block_has_opaque_calls(&remainder)
            || block_writes_static(&remainder)
        {
            return None;
        }
        Some(ForeachPattern {
            index: index.clone(),
            collection,
            item,
            omitted,
        })
    }
}

pub(super) struct ForeachPattern {
    pub(super) index: String,
    collection: String,
    item: String,
    omitted: BTreeSet<usize>,
}

fn is_zero_literal(literal: &Literal) -> bool {
    match literal {
        Literal::Int(0) => true,
        Literal::BigInt(value) => value == "0",
        _ => false,
    }
}

fn indexed_collection(expression: &Expr, index: &str) -> Option<String> {
    let (base, item_index) = match expression {
        Expr::Index { base, index } => (base.as_ref(), index.as_ref()),
        Expr::Call {
            target: SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Pickitem)),
            args,
        } => {
            let [base, index] = args.as_slice() else {
                return None;
            };
            (base, index)
        }
        _ => return None,
    };
    let Expr::Variable(collection) = base else {
        return None;
    };
    matches!(item_index, Expr::Variable(item_index) if item_index == index)
        .then(|| collection.clone())
}

fn resolve_collection_alias(name: &str, facts: &DefinitionFacts) -> String {
    let mut current = name;
    let mut seen = BTreeSet::new();
    while seen.insert(current.to_string()) {
        let Some(Expr::Variable(source)) = facts.get(current) else {
            break;
        };
        current = source;
    }
    current.to_string()
}

fn is_collection_bound(expression: &Expr, collection_root: &str, facts: &DefinitionFacts) -> bool {
    let mut seen = BTreeSet::new();
    is_collection_bound_inner(expression, collection_root, facts, &mut seen)
}

fn is_collection_bound_inner(
    expression: &Expr,
    collection_root: &str,
    facts: &DefinitionFacts,
    seen: &mut BTreeSet<String>,
) -> bool {
    match expression {
        Expr::Variable(name) => {
            if !seen.insert(name.clone()) {
                return false;
            }
            let Some(value) = facts.get(name) else {
                return false;
            };
            is_collection_bound_inner(value, collection_root, facts, seen)
        }
        Expr::Member { base, name } if name.eq_ignore_ascii_case("Length") => {
            is_collection_expression(base, collection_root, facts, seen)
        }
        Expr::Call {
            target: SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Size)),
            args,
        } => {
            let [base] = args.as_slice() else {
                return false;
            };
            is_collection_expression(base, collection_root, facts, seen)
        }
        Expr::Cast { expr, .. } => is_collection_bound_inner(expr, collection_root, facts, seen),
        Expr::Convert { value, .. } => {
            is_collection_bound_inner(value, collection_root, facts, seen)
        }
        _ => false,
    }
}

fn is_collection_expression(
    expression: &Expr,
    collection_root: &str,
    facts: &DefinitionFacts,
    seen: &mut BTreeSet<String>,
) -> bool {
    let Expr::Variable(name) = expression else {
        return false;
    };
    if !seen.insert(name.clone()) {
        return false;
    }
    resolve_collection_alias(name, facts) == collection_root
}
