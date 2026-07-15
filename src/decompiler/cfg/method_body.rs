use std::collections::{BTreeMap, BTreeSet};

use crate::decompiler::analysis::method_contracts::ReturnBehavior;
use crate::decompiler::analysis::types::ValueType;
use crate::decompiler::cfg::ssa::{optimize_ssa, MethodContext, SsaBuilder};
use crate::decompiler::cfg::structure_cfg_with_source_names;
use crate::decompiler::high_level::MAX_HIGH_LEVEL_METHOD_INSTRUCTIONS;
use crate::decompiler::ir::{
    BinOp, Block, ControlFlow, Expr, Intrinsic, Literal, SemanticCallTarget, Stmt, UnaryOp,
};
use crate::decompiler::native_method_types;
use crate::decompiler::syscall_types;
use crate::instruction::{Instruction, OpCode};

mod cfg;
#[path = "method_body_names.rs"]
mod names;
mod opcode;
mod source_map;
#[path = "method_body_symbols.rs"]
mod symbols;
#[path = "method_body_types.rs"]
mod types;
mod validation;

use names::collect_block_names;
#[cfg(test)]
pub(crate) use opcode::classify_opcode;
pub(crate) use opcode::{classify_instruction, OpcodeFidelity};
use symbols::allocate_source_symbols;
use types::{intrinsic_result_type, merge_value_types};

pub(crate) use cfg::{build_method_cfg, build_method_cfg_with_non_returning_calls};

pub(crate) struct MethodIrRequest<'a> {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) instructions: &'a [Instruction],
    pub(crate) context: MethodContext,
    pub(crate) symbol_types: MethodSymbolTypes,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct MethodSymbolTypes {
    pub(crate) parameters: Vec<ValueType>,
    pub(crate) locals: Vec<ValueType>,
    pub(crate) statics: Vec<ValueType>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SymbolOrigin {
    Parameter(usize),
    Local(usize),
    Static(usize),
    Temporary,
    Phi,
    ExceptionPayload,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SymbolInfo {
    pub(crate) origin: SymbolOrigin,
    pub(crate) value_type: ValueType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[allow(dead_code)]
pub(crate) struct StatementId(pub(crate) u32);

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SourceMap {
    pub(crate) statement_origins: BTreeMap<StatementId, BTreeSet<usize>>,
}

#[allow(dead_code)]
pub(crate) struct StructuredMethodBody {
    pub(crate) body: Block,
    pub(crate) symbols: BTreeMap<String, SymbolInfo>,
    pub(crate) return_behavior: ReturnBehavior,
    pub(crate) fidelity: FidelityReport,
    pub(crate) source_map: SourceMap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Fidelity {
    Exact,
    Conservative,
    Incomplete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum LoweringIssueKind {
    UnsupportedControl,
    UnsupportedOpcode,
    LostStackValue,
    MissingOperandMetadata,
    UnresolvedCall,
    MissingProvenance,
    BudgetExceeded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoweringIssue {
    pub(crate) offset: usize,
    pub(crate) opcode: OpCode,
    pub(crate) kind: LoweringIssueKind,
    pub(crate) fidelity: Fidelity,
    pub(crate) detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FidelityReport {
    pub(crate) status: Fidelity,
    pub(crate) issues: Vec<LoweringIssue>,
    pub(crate) covered_offsets: BTreeSet<usize>,
    pub(crate) instruction_count: usize,
}

impl FidelityReport {
    pub(crate) fn exact(instruction_count: usize) -> Self {
        Self {
            status: Fidelity::Exact,
            issues: Vec::new(),
            covered_offsets: BTreeSet::new(),
            instruction_count,
        }
    }

    pub(crate) fn finish(&mut self) {
        self.issues.sort_by(|left, right| {
            (
                left.offset,
                left.opcode.byte(),
                left.opcode.mnemonic(),
                left.kind,
                left.detail.as_str(),
            )
                .cmp(&(
                    right.offset,
                    right.opcode.byte(),
                    right.opcode.mnemonic(),
                    right.kind,
                    right.detail.as_str(),
                ))
                .then_with(|| right.fidelity.cmp(&left.fidelity))
        });
        self.issues.dedup_by(|current, previous| {
            current.offset == previous.offset
                && current.opcode == previous.opcode
                && current.kind == previous.kind
                && current.detail == previous.detail
        });
        self.status = self
            .issues
            .iter()
            .map(|issue| issue.fidelity)
            .max()
            .unwrap_or(Fidelity::Exact);
    }

    pub(crate) fn primary_issue(&self) -> Option<&LoweringIssue> {
        self.issues
            .iter()
            .find(|issue| issue.fidelity == Fidelity::Incomplete)
            .or_else(|| self.issues.first())
    }
}

pub(crate) fn lower_method_body(request: MethodIrRequest<'_>) -> StructuredMethodBody {
    let instructions: Vec<_> = request
        .instructions
        .iter()
        .filter(|instruction| {
            instruction.offset >= request.start && instruction.offset < request.end
        })
        .cloned()
        .collect();
    let return_behavior = return_behavior(&request.context);

    if instructions.len() > MAX_HIGH_LEVEL_METHOD_INSTRUCTIONS {
        let first = instructions
            .first()
            .expect("an oversized method slice cannot be empty");
        let mut fidelity = FidelityReport::exact(instructions.len());
        fidelity.issues.push(LoweringIssue {
            offset: first.offset,
            opcode: first.opcode,
            kind: LoweringIssueKind::BudgetExceeded,
            fidelity: Fidelity::Incomplete,
            detail: format!(
                "method has {} instructions; limit is {MAX_HIGH_LEVEL_METHOD_INSTRUCTIONS}",
                instructions.len()
            ),
        });
        fidelity.finish();
        return StructuredMethodBody {
            body: Block::new(),
            symbols: BTreeMap::new(),
            return_behavior,
            fidelity,
            source_map: SourceMap::default(),
        };
    }

    let non_returning_calls = request
        .context
        .calls_by_offset
        .iter()
        .filter_map(|(offset, contract)| (!contract.may_return).then_some(*offset))
        .collect();
    let cfg = build_method_cfg_with_non_returning_calls(
        &instructions,
        request.start,
        request.end,
        &non_returning_calls,
    );
    let built = SsaBuilder::new(&cfg, &instructions)
        .with_method_context(&request.context)
        .build_with_report();
    let mut ssa = built.ssa;
    optimize_ssa(&mut ssa);
    let (source_names, mut symbols) =
        allocate_source_symbols(&request.context, &request.symbol_types, &ssa);
    let body = structure_cfg_with_source_names(&ssa, &source_names);
    register_structured_temporaries_with_call_types(
        &body,
        &mut symbols,
        &request.context.call_return_types,
    );
    let source_map = source_map::build_source_map(&body, &ssa, &source_names, &instructions);

    let mut fidelity = built.fidelity;
    if instructions
        .iter()
        .any(|instruction| matches!(instruction.opcode, OpCode::Endtry | OpCode::EndtryL))
        && !instructions
            .iter()
            .any(|instruction| matches!(instruction.opcode, OpCode::Try | OpCode::TryL))
    {
        if let Some(instruction) = instructions
            .iter()
            .find(|instruction| matches!(instruction.opcode, OpCode::Endtry | OpCode::EndtryL))
        {
            fidelity.issues.push(LoweringIssue {
                offset: instruction.offset,
                opcode: instruction.opcode,
                kind: LoweringIssueKind::UnsupportedControl,
                fidelity: Fidelity::Incomplete,
                detail: "ENDTRY has no enclosing TRY region".to_string(),
            });
        }
    }
    validation::validate_renderable(&body, &instructions, &mut fidelity);
    fidelity.finish();

    StructuredMethodBody {
        body,
        symbols,
        return_behavior,
        fidelity,
        source_map,
    }
}

fn return_behavior(context: &MethodContext) -> ReturnBehavior {
    match context.returns_value {
        Some(true) => ReturnBehavior::Value,
        Some(false) => ReturnBehavior::Void,
        None => ReturnBehavior::Unknown,
    }
}

#[cfg(test)]
fn register_structured_temporaries(body: &Block, symbols: &mut BTreeMap<String, SymbolInfo>) {
    register_structured_temporaries_with_call_types(body, symbols, &BTreeMap::new());
}

fn register_structured_temporaries_with_call_types(
    body: &Block,
    symbols: &mut BTreeMap<String, SymbolInfo>,
    call_return_types: &BTreeMap<usize, ValueType>,
) {
    let mut names = BTreeSet::new();
    collect_block_names(body, &mut names);
    for name in names {
        if name == "?" {
            continue;
        }
        symbols.entry(name).or_insert(SymbolInfo {
            origin: SymbolOrigin::Temporary,
            value_type: ValueType::Unknown,
        });
    }

    for _ in 0..symbols.len() {
        if !refine_block_temporary_types(body, symbols, call_return_types) {
            break;
        }
    }
    widen_exception_payload_copies(body, symbols);
}

fn widen_exception_payload_copies(body: &Block, symbols: &mut BTreeMap<String, SymbolInfo>) {
    let mut copies = Vec::new();
    collect_direct_copy_edges(body, &mut copies);
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

fn refine_block_temporary_types(
    block: &Block,
    symbols: &mut BTreeMap<String, SymbolInfo>,
    call_return_types: &BTreeMap<usize, ValueType>,
) -> bool {
    let mut changed = false;
    for statement in &block.stmts {
        changed |= refine_statement_temporary_types(statement, symbols, call_return_types);
    }
    changed
}

fn refine_statement_temporary_types(
    statement: &Stmt,
    symbols: &mut BTreeMap<String, SymbolInfo>,
    call_return_types: &BTreeMap<usize, ValueType>,
) -> bool {
    match statement {
        Stmt::Assign { target, value } => {
            let inferred = structured_expr_type(value, symbols, call_return_types);
            let Some(symbol) = symbols.get_mut(target) else {
                return false;
            };
            if symbol.origin != SymbolOrigin::Temporary {
                return false;
            }
            let merged = merge_value_types(symbol.value_type, inferred);
            let changed = merged != symbol.value_type;
            symbol.value_type = merged;
            changed
        }
        Stmt::ControlFlow(control) => match control.as_ref() {
            ControlFlow::If {
                then_branch,
                else_branch,
                ..
            } => {
                let mut changed =
                    refine_block_temporary_types(then_branch, symbols, call_return_types);
                if let Some(else_branch) = else_branch {
                    changed |=
                        refine_block_temporary_types(else_branch, symbols, call_return_types);
                }
                changed
            }
            ControlFlow::While { body, .. } | ControlFlow::DoWhile { body, .. } => {
                refine_block_temporary_types(body, symbols, call_return_types)
            }
            ControlFlow::For { init, body, .. } => {
                let mut changed = init.as_deref().is_some_and(|init| {
                    refine_statement_temporary_types(init, symbols, call_return_types)
                });
                changed |= refine_block_temporary_types(body, symbols, call_return_types);
                changed
            }
            ControlFlow::TryCatch {
                try_body,
                catch_body,
                finally_body,
                ..
            } => {
                let mut changed =
                    refine_block_temporary_types(try_body, symbols, call_return_types);
                if let Some(catch_body) = catch_body {
                    changed |= refine_block_temporary_types(catch_body, symbols, call_return_types);
                }
                if let Some(finally_body) = finally_body {
                    changed |=
                        refine_block_temporary_types(finally_body, symbols, call_return_types);
                }
                changed
            }
            ControlFlow::Switch { cases, default, .. } => {
                let mut changed = false;
                for (_, body) in cases {
                    changed |= refine_block_temporary_types(body, symbols, call_return_types);
                }
                if let Some(default) = default {
                    changed |= refine_block_temporary_types(default, symbols, call_return_types);
                }
                changed
            }
        },
        Stmt::Return(_)
        | Stmt::Throw(_)
        | Stmt::Abort(_)
        | Stmt::Assert { .. }
        | Stmt::ExprStmt(_)
        | Stmt::Comment(_)
        | Stmt::Break
        | Stmt::Continue
        | Stmt::Label(_)
        | Stmt::Goto(_) => false,
    }
}

fn structured_expr_type(
    expression: &Expr,
    symbols: &BTreeMap<String, SymbolInfo>,
    call_return_types: &BTreeMap<usize, ValueType>,
) -> ValueType {
    match expression {
        Expr::Unknown => ValueType::Unknown,
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
            structured_expr_type(then_expr, symbols, call_return_types),
            structured_expr_type(else_expr, symbols, call_return_types),
        ),
        Expr::Call {
            target: SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Cat)),
            args,
        } => args.first().map_or(ValueType::Unknown, |left| {
            match structured_expr_type(left, symbols, call_return_types) {
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

#[cfg(test)]
#[path = "method_body_tests.rs"]
mod tests;
