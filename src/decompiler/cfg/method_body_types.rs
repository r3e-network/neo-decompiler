use std::collections::{BTreeMap, BTreeSet};

use crate::decompiler::analysis::types::ValueType;
use crate::decompiler::cfg::method_body::{SymbolInfo, SymbolOrigin};
use crate::decompiler::ir::{
    BinOp, Block, ControlFlow, Expr, Intrinsic, Literal, SemanticCallTarget, Stmt, UnaryOp,
};
use crate::decompiler::native_method_types;
use crate::decompiler::syscall_types;
use crate::instruction::OpCode;

pub(super) fn intrinsic_result_type(opcode: OpCode) -> ValueType {
    match opcode {
        OpCode::Newarray0
        | OpCode::Newarray
        | OpCode::NewarrayT
        | OpCode::Keys
        | OpCode::Values => ValueType::Array,
        OpCode::Newstruct0 | OpCode::Newstruct => ValueType::Struct,
        OpCode::Newmap => ValueType::Map,
        OpCode::Newbuffer => ValueType::Buffer,
        OpCode::Size | OpCode::Sqrt | OpCode::Min | OpCode::Max => ValueType::Integer,
        OpCode::Haskey | OpCode::Isnull | OpCode::Istype | OpCode::Nz => ValueType::Boolean,
        _ => ValueType::Unknown,
    }
}

pub(super) fn merge_value_types(left: ValueType, right: ValueType) -> ValueType {
    use ValueType::{Any, Null, Unknown};

    if left == right {
        return left;
    }
    match (left, right) {
        (Unknown, value) | (value, Unknown) => value,
        (Null, _) | (_, Null) => Any,
        _ => Any,
    }
}

#[cfg(test)]
pub(super) fn register_structured_temporaries(
    body: &Block,
    symbols: &mut BTreeMap<String, SymbolInfo>,
) {
    register_structured_temporaries_with_call_types(
        body,
        symbols,
        &BTreeMap::new(),
        &BTreeSet::new(),
    );
}

pub(super) fn register_structured_temporaries_with_call_types(
    body: &Block,
    symbols: &mut BTreeMap<String, SymbolInfo>,
    call_return_types: &BTreeMap<usize, ValueType>,
    pointer_values: &BTreeSet<i64>,
) {
    let mut names = BTreeSet::new();
    super::names::collect_block_names(body, &mut names);
    for name in names {
        if name == "?" {
            continue;
        }
        symbols.entry(name).or_insert(SymbolInfo {
            origin: SymbolOrigin::Temporary,
            value_type: ValueType::Unknown,
        });
    }

    // Re-run the body-wide definition pass until aliases and call results have
    // enough information to converge. Definitions are collected before any
    // symbol is updated, so branch order cannot decide which type wins.
    for _ in 0..symbols.len().max(1) {
        if !refine_structured_types(body, symbols, call_return_types, pointer_values) {
            break;
        }
    }
    widen_exception_payload_copies(body, symbols);
}

#[derive(Debug, Default)]
struct ObservedDefinitionType {
    candidate: Option<ValueType>,
    ambiguous: bool,
}

fn refine_structured_types(
    block: &Block,
    symbols: &mut BTreeMap<String, SymbolInfo>,
    call_return_types: &BTreeMap<usize, ValueType>,
    pointer_values: &BTreeSet<i64>,
) -> bool {
    let mut observed = BTreeMap::<String, ObservedDefinitionType>::new();
    collect_definition_types(
        block,
        symbols,
        call_return_types,
        pointer_values,
        &mut observed,
    );

    let mut changed = false;
    for (name, observed) in observed {
        let Some(symbol) = symbols.get_mut(&name) else {
            continue;
        };
        // Parameters describe the caller-visible ABI and exception payloads
        // are deliberately dynamic. Their body assignments must not rewrite
        // those contracts. Locals and statics are safe to refine only from a
        // unanimous set of observed definitions.
        if !matches!(
            symbol.origin,
            SymbolOrigin::Temporary
                | SymbolOrigin::Phi
                | SymbolOrigin::Local(_)
                | SymbolOrigin::Static(_)
        ) {
            continue;
        }
        let inferred = if observed.ambiguous {
            // Preserve a neutral Unknown for a symbol that has only unknown
            // definitions. Once a concrete definition is also observed, Any
            // blocks later fixed-point passes from guessing a type.
            if observed.candidate.is_none() && symbol.value_type == ValueType::Unknown {
                continue;
            }
            ValueType::Any
        } else {
            let Some(candidate) = observed.candidate else {
                continue;
            };
            candidate
        };
        let merged = merge_value_types(symbol.value_type, inferred);
        if merged != symbol.value_type {
            symbol.value_type = merged;
            changed = true;
        }
    }
    changed
}

fn collect_definition_types(
    block: &Block,
    symbols: &BTreeMap<String, SymbolInfo>,
    call_return_types: &BTreeMap<usize, ValueType>,
    pointer_values: &BTreeSet<i64>,
    observed: &mut BTreeMap<String, ObservedDefinitionType>,
) {
    for statement in &block.stmts {
        collect_statement_definition_types(
            statement,
            symbols,
            call_return_types,
            pointer_values,
            observed,
        );
    }
}

fn collect_statement_definition_types(
    statement: &Stmt,
    symbols: &BTreeMap<String, SymbolInfo>,
    call_return_types: &BTreeMap<usize, ValueType>,
    pointer_values: &BTreeSet<i64>,
    observed: &mut BTreeMap<String, ObservedDefinitionType>,
) {
    match statement {
        Stmt::Assign { target, value } => {
            let inferred = structured_expr_type(value, symbols, call_return_types, pointer_values);
            let entry = observed.entry(target.clone()).or_default();
            match inferred {
                ValueType::Unknown | ValueType::Any => entry.ambiguous = true,
                inferred => {
                    entry.candidate = Some(match entry.candidate {
                        Some(existing) => {
                            let merged = merge_value_types(existing, inferred);
                            if merged == ValueType::Any {
                                entry.ambiguous = true;
                            }
                            merged
                        }
                        None => inferred,
                    });
                }
            }
        }
        Stmt::ControlFlow(control) => {
            collect_control_definition_types(
                control,
                symbols,
                call_return_types,
                pointer_values,
                observed,
            );
        }
        _ => {}
    }
}

fn collect_control_definition_types(
    control: &ControlFlow,
    symbols: &BTreeMap<String, SymbolInfo>,
    call_return_types: &BTreeMap<usize, ValueType>,
    pointer_values: &BTreeSet<i64>,
    observed: &mut BTreeMap<String, ObservedDefinitionType>,
) {
    match control {
        ControlFlow::If {
            then_branch,
            else_branch,
            ..
        } => {
            collect_definition_types(
                then_branch,
                symbols,
                call_return_types,
                pointer_values,
                observed,
            );
            if let Some(else_branch) = else_branch {
                collect_definition_types(
                    else_branch,
                    symbols,
                    call_return_types,
                    pointer_values,
                    observed,
                );
            }
        }
        ControlFlow::While { body, .. } | ControlFlow::DoWhile { body, .. } => {
            collect_definition_types(body, symbols, call_return_types, pointer_values, observed);
        }
        ControlFlow::For { init, body, .. } => {
            if let Some(init) = init {
                collect_statement_definition_types(
                    init,
                    symbols,
                    call_return_types,
                    pointer_values,
                    observed,
                );
            }
            collect_definition_types(body, symbols, call_return_types, pointer_values, observed);
        }
        ControlFlow::TryCatch {
            try_body,
            catch_body,
            finally_body,
            ..
        } => {
            collect_definition_types(
                try_body,
                symbols,
                call_return_types,
                pointer_values,
                observed,
            );
            if let Some(catch_body) = catch_body {
                collect_definition_types(
                    catch_body,
                    symbols,
                    call_return_types,
                    pointer_values,
                    observed,
                );
            }
            if let Some(finally_body) = finally_body {
                collect_definition_types(
                    finally_body,
                    symbols,
                    call_return_types,
                    pointer_values,
                    observed,
                );
            }
        }
        ControlFlow::Switch { cases, default, .. } => {
            for (_, body) in cases {
                collect_definition_types(
                    body,
                    symbols,
                    call_return_types,
                    pointer_values,
                    observed,
                );
            }
            if let Some(default) = default {
                collect_definition_types(
                    default,
                    symbols,
                    call_return_types,
                    pointer_values,
                    observed,
                );
            }
        }
    }
}

fn widen_exception_payload_copies(block: &Block, symbols: &mut BTreeMap<String, SymbolInfo>) {
    let mut copies = Vec::new();
    collect_direct_copy_edges(block, &mut copies);
    let mut payloads = symbols
        .iter()
        .filter(|(_, symbol)| symbol.origin == SymbolOrigin::ExceptionPayload)
        .map(|(name, _)| name.clone())
        .collect::<BTreeSet<_>>();
    loop {
        let mut changed = false;
        for (target, source) in &copies {
            if payloads.contains(source) && payloads.insert(target.clone()) {
                changed = true;
                if let Some(symbol) = symbols.get_mut(target) {
                    symbol.value_type = ValueType::Any;
                }
            }
        }
        if !changed {
            break;
        }
    }
}

fn collect_direct_copy_edges(block: &Block, copies: &mut Vec<(String, String)>) {
    for statement in &block.stmts {
        match statement {
            Stmt::Assign {
                target,
                value: Expr::Variable(source),
            } => copies.push((target.clone(), source.clone())),
            Stmt::ControlFlow(control) => match control.as_ref() {
                ControlFlow::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    collect_direct_copy_edges(then_branch, copies);
                    if let Some(else_branch) = else_branch {
                        collect_direct_copy_edges(else_branch, copies);
                    }
                }
                ControlFlow::While { body, .. } | ControlFlow::DoWhile { body, .. } => {
                    collect_direct_copy_edges(body, copies);
                }
                ControlFlow::For { init, body, .. } => {
                    if let Some(init) = init {
                        if let Stmt::Assign {
                            target,
                            value: Expr::Variable(source),
                        } = init.as_ref()
                        {
                            copies.push((target.clone(), source.clone()));
                        }
                    }
                    collect_direct_copy_edges(body, copies);
                }
                ControlFlow::TryCatch {
                    try_body,
                    catch_body,
                    finally_body,
                    ..
                } => {
                    collect_direct_copy_edges(try_body, copies);
                    if let Some(catch_body) = catch_body {
                        collect_direct_copy_edges(catch_body, copies);
                    }
                    if let Some(finally_body) = finally_body {
                        collect_direct_copy_edges(finally_body, copies);
                    }
                }
                ControlFlow::Switch { cases, default, .. } => {
                    for (_, body) in cases {
                        collect_direct_copy_edges(body, copies);
                    }
                    if let Some(default) = default {
                        collect_direct_copy_edges(default, copies);
                    }
                }
            },
            _ => {}
        }
    }
}

fn structured_expr_type(
    expression: &Expr,
    symbols: &BTreeMap<String, SymbolInfo>,
    call_return_types: &BTreeMap<usize, ValueType>,
    pointer_values: &BTreeSet<i64>,
) -> ValueType {
    match expression {
        Expr::Unknown => ValueType::Unknown,
        Expr::Literal(Literal::Int(value)) if pointer_values.contains(value) => ValueType::Pointer,
        Expr::Literal(Literal::Int(_) | Literal::BigInt(_)) => ValueType::Integer,
        Expr::Literal(Literal::Bool(_)) => ValueType::Boolean,
        Expr::Literal(Literal::String(_)) => ValueType::ByteString,
        Expr::Literal(Literal::Bytes(_)) => ValueType::ByteString,
        Expr::Literal(Literal::Null) => ValueType::Null,
        Expr::Variable(name) => symbols
            .get(name)
            .map_or(ValueType::Unknown, |symbol| symbol.value_type),
        Expr::Binary { op, .. } => match op {
            BinOp::Eq
            | BinOp::Ne
            | BinOp::Lt
            | BinOp::Le
            | BinOp::Gt
            | BinOp::Ge
            | BinOp::LogicalAnd
            | BinOp::LogicalOr => ValueType::Boolean,
            BinOp::Add
            | BinOp::Sub
            | BinOp::Mul
            | BinOp::Div
            | BinOp::Mod
            | BinOp::Pow
            | BinOp::And
            | BinOp::Or
            | BinOp::Xor
            | BinOp::Shl
            | BinOp::Shr => ValueType::Integer,
        },
        Expr::Unary { op, .. } => match op {
            UnaryOp::LogicalNot => ValueType::Boolean,
            UnaryOp::Neg
            | UnaryOp::Not
            | UnaryOp::Inc
            | UnaryOp::Dec
            | UnaryOp::Abs
            | UnaryOp::Sign => ValueType::Integer,
        },
        Expr::Convert { target, .. } => *target,
        Expr::IsType { .. } => ValueType::Boolean,
        Expr::NewArray { .. } | Expr::Array(_) => ValueType::Array,
        Expr::Struct(_) => ValueType::Struct,
        Expr::Map(_) => ValueType::Map,
        Expr::Index { base, .. } => match base.as_ref() {
            Expr::NewArray {
                element_type: Some(element_type),
                ..
            } => *element_type,
            _ => ValueType::Unknown,
        },
        Expr::Ternary {
            then_expr,
            else_expr,
            ..
        } => merge_value_types(
            structured_expr_type(then_expr, symbols, call_return_types, pointer_values),
            structured_expr_type(else_expr, symbols, call_return_types, pointer_values),
        ),
        Expr::Call {
            target: SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Cat)),
            args,
        } => args.first().map_or(ValueType::Unknown, |left| {
            match structured_expr_type(left, symbols, call_return_types, pointer_values) {
                ValueType::ByteString => ValueType::ByteString,
                ValueType::Buffer => ValueType::Buffer,
                _ => ValueType::Unknown,
            }
        }),
        Expr::Call {
            target: SemanticCallTarget::Intrinsic(Intrinsic::Opcode(opcode)),
            ..
        } => intrinsic_result_type(*opcode),
        Expr::Call {
            target: SemanticCallTarget::Intrinsic(Intrinsic::UnpackPackStruct),
            ..
        } => ValueType::Struct,
        Expr::Call {
            target: SemanticCallTarget::Internal { offset, .. },
            ..
        } => call_return_types
            .get(offset)
            .copied()
            .unwrap_or(ValueType::Unknown),
        Expr::Call {
            target:
                SemanticCallTarget::MethodToken {
                    name,
                    hash_le,
                    call_flags,
                    ..
                },
            ..
        } => native_method_types::lookup(hash_le.as_deref(), name, *call_flags)
            .map_or(ValueType::Unknown, |return_type| return_type.value_type),
        Expr::Call {
            target: SemanticCallTarget::Syscall { hash, .. },
            ..
        } => syscall_types::lookup(*hash)
            .map_or(ValueType::Unknown, |return_type| return_type.value_type),
        Expr::Call { .. } | Expr::Member { .. } | Expr::Cast { .. } | Expr::StackTemp(_) => {
            ValueType::Unknown
        }
    }
}
