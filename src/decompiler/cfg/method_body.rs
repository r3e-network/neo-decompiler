use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::decompiler::analysis::method_contracts::ReturnBehavior;
use crate::decompiler::analysis::types::ValueType;
use crate::decompiler::cfg::ssa::{optimize_ssa, MethodContext, SsaBuilder, SsaForm, SsaVariable};
use crate::decompiler::cfg::{
    structure_cfg_with_source_names, BasicBlock, Cfg, CfgBuilder, EdgeKind, Terminator,
};
use crate::decompiler::helpers::make_unique_identifier;
use crate::decompiler::high_level::MAX_HIGH_LEVEL_METHOD_INSTRUCTIONS;
use crate::decompiler::ir::{
    BinOp, Block, ControlFlow, Expr, Intrinsic, Literal, SemanticCallTarget, Stmt, UnaryOp,
};
use crate::instruction::{Instruction, OpCode, Operand};

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
    register_structured_temporaries(&body, &mut symbols);
    let source_map = build_source_map(&body, &ssa, &source_names, &instructions);

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
    validate_renderable(&body, &instructions, &mut fidelity);
    fidelity.finish();

    StructuredMethodBody {
        body,
        symbols,
        return_behavior,
        fidelity,
        source_map,
    }
}

fn build_source_map(
    body: &Block,
    ssa: &SsaForm,
    source_names: &BTreeMap<String, String>,
    instructions: &[Instruction],
) -> SourceMap {
    let all_offsets = instructions
        .iter()
        .map(|instruction| instruction.offset)
        .collect();
    let mut variable_origins = BTreeMap::<String, BTreeSet<usize>>::new();
    for (variable, block) in &ssa.definitions {
        let Some(cfg_block) = ssa.cfg.block(*block) else {
            continue;
        };
        let origins = cfg_block
            .instruction_range
            .clone()
            .filter_map(|index| {
                instructions
                    .get(index)
                    .map(|instruction| instruction.offset)
            })
            .collect::<BTreeSet<_>>();
        let raw_name = format!("{}_{}", variable.base, variable.version);
        let emitted = source_names
            .get(&variable.base)
            .or_else(|| source_names.get(&raw_name))
            .cloned()
            .unwrap_or(raw_name);
        variable_origins.entry(emitted).or_default().extend(origins);
    }

    let mut origins = BTreeMap::new();
    let mut next_id = 0u32;
    collect_source_origins(
        body,
        &variable_origins,
        &all_offsets,
        &mut next_id,
        &mut origins,
    );
    SourceMap {
        statement_origins: origins,
    }
}

fn collect_source_origins(
    block: &Block,
    variable_origins: &BTreeMap<String, BTreeSet<usize>>,
    all_offsets: &BTreeSet<usize>,
    next_id: &mut u32,
    origins: &mut BTreeMap<StatementId, BTreeSet<usize>>,
) {
    for statement in &block.stmts {
        let id = StatementId(*next_id);
        *next_id += 1;
        let mut names = BTreeSet::new();
        collect_statement_names(statement, &mut names);
        let mut statement_origins = BTreeSet::new();
        for name in names {
            if let Some(source) = variable_origins.get(&name) {
                statement_origins.extend(source.iter().copied());
            }
        }
        if statement_origins.is_empty() {
            statement_origins = all_offsets.clone();
        }
        origins.insert(id, statement_origins);
        collect_nested_source_origins(statement, variable_origins, all_offsets, next_id, origins);
    }
}

fn collect_nested_source_origins(
    statement: &Stmt,
    variable_origins: &BTreeMap<String, BTreeSet<usize>>,
    all_offsets: &BTreeSet<usize>,
    next_id: &mut u32,
    origins: &mut BTreeMap<StatementId, BTreeSet<usize>>,
) {
    if let Stmt::ControlFlow(control) = statement {
        match control.as_ref() {
            ControlFlow::If {
                then_branch,
                else_branch,
                ..
            } => {
                collect_source_origins(
                    then_branch,
                    variable_origins,
                    all_offsets,
                    next_id,
                    origins,
                );
                if let Some(branch) = else_branch {
                    collect_source_origins(branch, variable_origins, all_offsets, next_id, origins);
                }
            }
            ControlFlow::While { body, .. } | ControlFlow::DoWhile { body, .. } => {
                collect_source_origins(body, variable_origins, all_offsets, next_id, origins);
            }
            ControlFlow::For { init, body, .. } => {
                if let Some(init) = init {
                    collect_nested_source_origins(
                        init,
                        variable_origins,
                        all_offsets,
                        next_id,
                        origins,
                    );
                }
                collect_source_origins(body, variable_origins, all_offsets, next_id, origins);
            }
            ControlFlow::TryCatch {
                try_body,
                catch_body,
                finally_body,
                ..
            } => {
                collect_source_origins(try_body, variable_origins, all_offsets, next_id, origins);
                if let Some(body) = catch_body {
                    collect_source_origins(body, variable_origins, all_offsets, next_id, origins);
                }
                if let Some(body) = finally_body {
                    collect_source_origins(body, variable_origins, all_offsets, next_id, origins);
                }
            }
            ControlFlow::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    collect_source_origins(body, variable_origins, all_offsets, next_id, origins);
                }
                if let Some(body) = default {
                    collect_source_origins(body, variable_origins, all_offsets, next_id, origins);
                }
            }
        }
    }
}

pub(crate) fn build_method_cfg(instructions: &[Instruction], start: usize, end: usize) -> Cfg {
    build_method_cfg_with_non_returning_calls(instructions, start, end, &BTreeSet::new())
}

pub(crate) fn build_method_cfg_with_non_returning_calls(
    instructions: &[Instruction],
    start: usize,
    end: usize,
    non_returning_calls: &BTreeSet<usize>,
) -> Cfg {
    let built = CfgBuilder::new(instructions)
        .with_non_returning_calls(non_returning_calls.iter().copied())
        .build();
    let mut cfg = Cfg::new();

    for block in built.blocks() {
        let mut block = block.clone();
        if control_transfer_leaves_method(&block, instructions, start, end) {
            block.terminator = Terminator::Return;
        }
        cfg.add_block(block);
    }

    for edge in built.edges() {
        let retained = cfg
            .block(edge.from)
            .is_some_and(|block| block.terminator.successors().contains(&edge.to));
        if retained {
            cfg.add_edge(edge.from, edge.to, edge.kind);
        }
    }

    cfg
}

fn control_transfer_leaves_method(
    block: &BasicBlock,
    instructions: &[Instruction],
    start: usize,
    end: usize,
) -> bool {
    let Some(last_index) = block.instruction_range.end.checked_sub(1) else {
        return false;
    };
    let Some(instruction) = instructions.get(last_index) else {
        return false;
    };
    let is_conditional = matches!(
        instruction.opcode,
        OpCode::Jmpif
            | OpCode::Jmpif_L
            | OpCode::Jmpifnot
            | OpCode::Jmpifnot_L
            | OpCode::JmpEq
            | OpCode::JmpEq_L
            | OpCode::JmpNe
            | OpCode::JmpNe_L
            | OpCode::JmpGt
            | OpCode::JmpGt_L
            | OpCode::JmpGe
            | OpCode::JmpGe_L
            | OpCode::JmpLt
            | OpCode::JmpLt_L
            | OpCode::JmpLe
            | OpCode::JmpLe_L
    );
    let is_jump = is_conditional || matches!(instruction.opcode, OpCode::Jmp | OpCode::Jmp_L);
    if !is_jump {
        return false;
    }

    let target = match instruction.operand {
        Some(Operand::Jump(delta)) => instruction.offset.checked_add_signed(delta as isize),
        Some(Operand::Jump32(delta)) => instruction.offset.checked_add_signed(delta as isize),
        _ => None,
    };
    let target_leaves = target.is_some_and(|target| target < start || target >= end);
    let fallthrough_leaves = is_conditional && instructions.get(last_index + 1).is_none();
    target_leaves || fallthrough_leaves
}

fn return_behavior(context: &MethodContext) -> ReturnBehavior {
    match context.returns_value {
        Some(true) => ReturnBehavior::Value,
        Some(false) => ReturnBehavior::Void,
        None => ReturnBehavior::Unknown,
    }
}

fn allocate_source_symbols(
    context: &MethodContext,
    symbol_types: &MethodSymbolTypes,
    ssa: &SsaForm,
) -> (BTreeMap<String, String>, BTreeMap<String, SymbolInfo>) {
    let variables = ssa_variables(ssa);
    let mut source_names = context.source_names();
    let mut symbols = BTreeMap::new();
    let mut used = HashSet::new();

    for (index, name) in context.argument_names.iter().enumerate() {
        source_names.insert(format!("arg{index}"), name.clone());
        used.insert(name.clone());
        symbols.insert(
            name.clone(),
            SymbolInfo {
                origin: SymbolOrigin::Parameter(index),
                value_type: symbol_type(&symbol_types.parameters, index),
            },
        );
    }

    let mut argument_indices: BTreeSet<_> = (0..symbol_types.parameters.len()).collect();
    let mut local_indices: BTreeSet<_> = (0..symbol_types.locals.len()).collect();
    let mut static_indices: BTreeSet<_> = (0..symbol_types.statics.len()).collect();
    for variable in &variables {
        if let Some(index) = slot_index(&variable.base, "arg") {
            argument_indices.insert(index);
        } else if let Some(index) = slot_index(&variable.base, "loc") {
            local_indices.insert(index);
        } else if let Some(index) = slot_index(&variable.base, "static") {
            static_indices.insert(index);
        }
    }

    for index in argument_indices {
        if index < context.argument_names.len() {
            continue;
        }
        register_source_family(
            "arg",
            index,
            SymbolOrigin::Parameter(index),
            symbol_type(&symbol_types.parameters, index),
            &mut source_names,
            &mut symbols,
            &mut used,
        );
    }
    for index in local_indices {
        register_source_family(
            "loc",
            index,
            SymbolOrigin::Local(index),
            symbol_type(&symbol_types.locals, index),
            &mut source_names,
            &mut symbols,
            &mut used,
        );
    }
    for index in static_indices {
        register_source_family(
            "static",
            index,
            SymbolOrigin::Static(index),
            symbol_type(&symbol_types.statics, index),
            &mut source_names,
            &mut symbols,
            &mut used,
        );
    }

    for variable in variables {
        if variable.base == "?" || is_source_family(&variable.base) {
            continue;
        }
        let generated = format!("{}_{}", variable.base, variable.version);
        let emitted = make_unique_identifier(generated.clone(), &mut used);
        source_names.insert(generated, emitted.clone());
        symbols.entry(emitted).or_insert(SymbolInfo {
            origin: if variable.is_exception_payload() {
                SymbolOrigin::ExceptionPayload
            } else if is_stack_phi_base(&variable.base) {
                SymbolOrigin::Phi
            } else {
                SymbolOrigin::Temporary
            },
            value_type: if variable.is_exception_payload() {
                ValueType::Any
            } else {
                ValueType::Unknown
            },
        });
    }

    (source_names, symbols)
}

fn register_source_family(
    prefix: &str,
    index: usize,
    origin: SymbolOrigin,
    value_type: ValueType,
    source_names: &mut BTreeMap<String, String>,
    symbols: &mut BTreeMap<String, SymbolInfo>,
    used: &mut HashSet<String>,
) {
    let base = format!("{prefix}{index}");
    let emitted = make_unique_identifier(base.clone(), used);
    source_names.insert(base, emitted.clone());
    symbols.insert(emitted, SymbolInfo { origin, value_type });
}

fn ssa_variables(ssa: &SsaForm) -> BTreeSet<SsaVariable> {
    let mut variables: BTreeSet<_> = ssa
        .definitions
        .keys()
        .chain(ssa.uses.keys())
        .cloned()
        .collect();
    for block in ssa.blocks.values() {
        for phi in &block.phi_nodes {
            variables.insert(phi.target.clone());
            variables.extend(phi.operands.values().cloned());
        }
    }
    for edge in ssa
        .cfg
        .edges()
        .iter()
        .filter(|edge| edge.kind == EdgeKind::Exception)
    {
        variables.insert(SsaVariable::exception_payload(edge.to));
    }
    variables
}

fn slot_index(base: &str, prefix: &str) -> Option<usize> {
    let suffix = base.strip_prefix(prefix)?;
    (!suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| suffix.parse().ok())
        .flatten()
}

fn is_source_family(base: &str) -> bool {
    slot_index(base, "arg").is_some()
        || slot_index(base, "loc").is_some()
        || slot_index(base, "static").is_some()
}

fn is_stack_phi_base(base: &str) -> bool {
    base.strip_prefix('p').is_some_and(|suffix| {
        !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
    })
}

fn symbol_type(types: &[ValueType], index: usize) -> ValueType {
    types.get(index).copied().unwrap_or(ValueType::Unknown)
}

fn register_structured_temporaries(body: &Block, symbols: &mut BTreeMap<String, SymbolInfo>) {
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
        if !refine_block_temporary_types(body, symbols) {
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

fn refine_block_temporary_types(block: &Block, symbols: &mut BTreeMap<String, SymbolInfo>) -> bool {
    let mut changed = false;
    for statement in &block.stmts {
        changed |= refine_statement_temporary_types(statement, symbols);
    }
    changed
}

fn refine_statement_temporary_types(
    statement: &Stmt,
    symbols: &mut BTreeMap<String, SymbolInfo>,
) -> bool {
    match statement {
        Stmt::Assign { target, value } => {
            let inferred = structured_expr_type(value, symbols);
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
                let mut changed = refine_block_temporary_types(then_branch, symbols);
                if let Some(else_branch) = else_branch {
                    changed |= refine_block_temporary_types(else_branch, symbols);
                }
                changed
            }
            ControlFlow::While { body, .. } | ControlFlow::DoWhile { body, .. } => {
                refine_block_temporary_types(body, symbols)
            }
            ControlFlow::For { init, body, .. } => {
                let mut changed = init
                    .as_deref()
                    .is_some_and(|init| refine_statement_temporary_types(init, symbols));
                changed |= refine_block_temporary_types(body, symbols);
                changed
            }
            ControlFlow::TryCatch {
                try_body,
                catch_body,
                finally_body,
                ..
            } => {
                let mut changed = refine_block_temporary_types(try_body, symbols);
                if let Some(catch_body) = catch_body {
                    changed |= refine_block_temporary_types(catch_body, symbols);
                }
                if let Some(finally_body) = finally_body {
                    changed |= refine_block_temporary_types(finally_body, symbols);
                }
                changed
            }
            ControlFlow::Switch { cases, default, .. } => {
                let mut changed = false;
                for (_, body) in cases {
                    changed |= refine_block_temporary_types(body, symbols);
                }
                if let Some(default) = default {
                    changed |= refine_block_temporary_types(default, symbols);
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

fn structured_expr_type(expression: &Expr, symbols: &BTreeMap<String, SymbolInfo>) -> ValueType {
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
        Expr::Ternary {
            then_expr,
            else_expr,
            ..
        } => merge_value_types(
            structured_expr_type(then_expr, symbols),
            structured_expr_type(else_expr, symbols),
        ),
        Expr::Call {
            target: SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Cat)),
            args,
        } => args.first().map_or(ValueType::Unknown, |left| {
            match structured_expr_type(left, symbols) {
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
        Expr::Call { .. }
        | Expr::Index { .. }
        | Expr::Member { .. }
        | Expr::Cast { .. }
        | Expr::StackTemp(_) => ValueType::Unknown,
    }
}

fn intrinsic_result_type(opcode: OpCode) -> ValueType {
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

fn merge_value_types(left: ValueType, right: ValueType) -> ValueType {
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

fn collect_block_names(block: &Block, names: &mut BTreeSet<String>) {
    for statement in &block.stmts {
        collect_statement_names(statement, names);
    }
}

fn collect_statement_names(statement: &Stmt, names: &mut BTreeSet<String>) {
    match statement {
        Stmt::Assign { target, value } => {
            names.insert(target.clone());
            collect_expr_names(value, names);
        }
        Stmt::Return(value) => {
            if let Some(value) = value {
                collect_expr_names(value, names);
            }
        }
        Stmt::Throw(value) | Stmt::Abort(value) => {
            if let Some(value) = value {
                collect_expr_names(value, names);
            }
        }
        Stmt::Assert { condition, message } => {
            collect_expr_names(condition, names);
            if let Some(message) = message {
                collect_expr_names(message, names);
            }
        }
        Stmt::ExprStmt(value) => collect_expr_names(value, names),
        Stmt::Comment(_) | Stmt::Break | Stmt::Continue | Stmt::Label(_) | Stmt::Goto(_) => {}
        Stmt::ControlFlow(control) => collect_control_names(control, names),
    }
}

fn collect_control_names(control: &ControlFlow, names: &mut BTreeSet<String>) {
    match control {
        ControlFlow::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_expr_names(condition, names);
            collect_block_names(then_branch, names);
            if let Some(branch) = else_branch {
                collect_block_names(branch, names);
            }
        }
        ControlFlow::While { condition, body } => {
            collect_expr_names(condition, names);
            collect_block_names(body, names);
        }
        ControlFlow::DoWhile { body, condition } => {
            collect_block_names(body, names);
            collect_expr_names(condition, names);
        }
        ControlFlow::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                collect_statement_names(init, names);
            }
            if let Some(condition) = condition {
                collect_expr_names(condition, names);
            }
            if let Some(update) = update {
                collect_expr_names(update, names);
            }
            collect_block_names(body, names);
        }
        ControlFlow::TryCatch {
            try_body,
            catch_var,
            catch_body,
            finally_body,
        } => {
            collect_block_names(try_body, names);
            if let Some(catch_var) = catch_var {
                names.insert(catch_var.clone());
            }
            if let Some(body) = catch_body {
                collect_block_names(body, names);
            }
            if let Some(body) = finally_body {
                collect_block_names(body, names);
            }
        }
        ControlFlow::Switch {
            expr,
            cases,
            default,
        } => {
            collect_expr_names(expr, names);
            for (value, body) in cases {
                collect_expr_names(value, names);
                collect_block_names(body, names);
            }
            if let Some(body) = default {
                collect_block_names(body, names);
            }
        }
    }
}

fn collect_expr_names(expression: &Expr, names: &mut BTreeSet<String>) {
    match expression {
        Expr::Variable(name) => {
            names.insert(name.clone());
        }
        Expr::Binary { left, right, .. } => {
            collect_expr_names(left, names);
            collect_expr_names(right, names);
        }
        Expr::Unary { operand, .. } => collect_expr_names(operand, names),
        Expr::Call { args, .. } | Expr::Array(args) => {
            for argument in args {
                collect_expr_names(argument, names);
            }
        }
        Expr::Index { base, index } => {
            collect_expr_names(base, names);
            collect_expr_names(index, names);
        }
        Expr::Member { base, .. } => collect_expr_names(base, names),
        Expr::Cast { expr, .. } => collect_expr_names(expr, names),
        Expr::Convert { value, .. } | Expr::IsType { value, .. } => {
            collect_expr_names(value, names);
        }
        Expr::NewArray { length, .. } => collect_expr_names(length, names),
        Expr::Map(pairs) => {
            for (key, value) in pairs {
                collect_expr_names(key, names);
                collect_expr_names(value, names);
            }
        }
        Expr::Struct(values) => {
            for value in values {
                collect_expr_names(value, names);
            }
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_expr_names(condition, names);
            collect_expr_names(then_expr, names);
            collect_expr_names(else_expr, names);
        }
        Expr::Unknown | Expr::Literal(_) | Expr::StackTemp(_) => {}
    }
}

fn validate_renderable(body: &Block, instructions: &[Instruction], fidelity: &mut FidelityReport) {
    let mut validation = Validation::default();
    validate_block(body, &mut validation);
    let Some(first) = instructions.first() else {
        return;
    };

    let mut add_issue = |kind, detail: &str| {
        if fidelity.issues.iter().any(|issue| issue.kind == kind) {
            return;
        }
        fidelity.issues.push(LoweringIssue {
            offset: first.offset,
            opcode: first.opcode,
            kind,
            fidelity: Fidelity::Incomplete,
            detail: detail.to_string(),
        });
    };
    if validation.unknown_value {
        add_issue(
            LoweringIssueKind::LostStackValue,
            "unknown value survives to structured output",
        );
    }
    if validation.unresolved_call {
        add_issue(
            LoweringIssueKind::UnresolvedCall,
            "unresolved call survives to structured output",
        );
    }
    if validation.missing_provenance {
        add_issue(
            LoweringIssueKind::MissingProvenance,
            "structured output contains a value without source provenance",
        );
    }
    if validation.unsupported_control {
        add_issue(
            LoweringIssueKind::UnsupportedControl,
            "structured output contains an unresolved control transfer",
        );
    }
}

#[derive(Default)]
struct Validation {
    unknown_value: bool,
    unresolved_call: bool,
    missing_provenance: bool,
    unsupported_control: bool,
}

fn validate_block(block: &Block, validation: &mut Validation) {
    for statement in &block.stmts {
        validate_statement(statement, validation);
    }
}

fn validate_statement(statement: &Stmt, validation: &mut Validation) {
    match statement {
        Stmt::Assign { value, .. } | Stmt::ExprStmt(value) => {
            validate_expr(value, validation);
        }
        Stmt::Return(value) => {
            if let Some(value) = value {
                validate_expr(value, validation);
            }
        }
        Stmt::Throw(value) | Stmt::Abort(value) => {
            if let Some(value) = value {
                validate_expr(value, validation);
            }
        }
        Stmt::Assert { condition, message } => {
            validate_expr(condition, validation);
            if let Some(message) = message {
                validate_expr(message, validation);
            }
        }
        Stmt::Comment(comment) => {
            validation.unsupported_control |=
                comment.starts_with("return at ") || comment.starts_with("return/throw/abort at ");
        }
        Stmt::Break | Stmt::Continue | Stmt::Label(_) | Stmt::Goto(_) => {}
        Stmt::ControlFlow(control) => validate_control(control, validation),
    }
}

fn validate_control(control: &ControlFlow, validation: &mut Validation) {
    match control {
        ControlFlow::If {
            condition,
            then_branch,
            else_branch,
        } => {
            validate_expr(condition, validation);
            validate_block(then_branch, validation);
            if let Some(branch) = else_branch {
                validate_block(branch, validation);
            }
        }
        ControlFlow::While { condition, body } => {
            validate_expr(condition, validation);
            validate_block(body, validation);
        }
        ControlFlow::DoWhile { body, condition } => {
            validate_block(body, validation);
            validate_expr(condition, validation);
        }
        ControlFlow::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                validate_statement(init, validation);
            }
            if let Some(condition) = condition {
                validate_expr(condition, validation);
            }
            if let Some(update) = update {
                validate_expr(update, validation);
            }
            validate_block(body, validation);
        }
        ControlFlow::TryCatch {
            try_body,
            catch_body,
            finally_body,
            ..
        } => {
            validate_block(try_body, validation);
            if let Some(body) = catch_body {
                validate_block(body, validation);
            }
            if let Some(body) = finally_body {
                validate_block(body, validation);
            }
        }
        ControlFlow::Switch {
            expr,
            cases,
            default,
        } => {
            validate_expr(expr, validation);
            for (value, body) in cases {
                validate_expr(value, validation);
                validate_block(body, validation);
            }
            if let Some(body) = default {
                validate_block(body, validation);
            }
        }
    }
}

fn validate_expr(expression: &Expr, validation: &mut Validation) {
    match expression {
        Expr::Variable(name) => validation.unknown_value |= name == "?",
        Expr::Binary { left, right, .. } => {
            validate_expr(left, validation);
            validate_expr(right, validation);
        }
        Expr::Unary { operand, .. } => validate_expr(operand, validation),
        Expr::Call { target, args } => {
            validation.unresolved_call |= matches!(target, SemanticCallTarget::Unresolved { .. });
            for argument in args {
                validate_expr(argument, validation);
            }
        }
        Expr::Index { base, index } => {
            validate_expr(base, validation);
            validate_expr(index, validation);
        }
        Expr::Member { base, .. } => validate_expr(base, validation),
        Expr::Cast { expr, .. } => validate_expr(expr, validation),
        Expr::Convert { value, .. } | Expr::IsType { value, .. } => {
            validate_expr(value, validation);
        }
        Expr::NewArray { length, .. } => validate_expr(length, validation),
        Expr::Array(values) => {
            for value in values {
                validate_expr(value, validation);
            }
        }
        Expr::Struct(values) => {
            for value in values {
                validate_expr(value, validation);
            }
        }
        Expr::Map(pairs) => {
            for (key, value) in pairs {
                validate_expr(key, validation);
                validate_expr(value, validation);
            }
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            validate_expr(condition, validation);
            validate_expr(then_expr, validation);
            validate_expr(else_expr, validation);
        }
        Expr::StackTemp(_) => validation.missing_provenance = true,
        Expr::Unknown => validation.unknown_value = true,
        Expr::Literal(_) => {}
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OpcodeFidelity {
    Exact,
    Conservative,
    Incomplete(LoweringIssueKind),
}

pub(crate) fn classify_opcode(opcode: OpCode) -> OpcodeFidelity {
    use OpCode::*;

    match opcode {
        Unknown(_) => OpcodeFidelity::Incomplete(LoweringIssueKind::UnsupportedOpcode),
        Xdrop | Pick | Roll | Reversen => OpcodeFidelity::Exact,
        Abort | Abortmsg | Syscall => OpcodeFidelity::Conservative,
        Pushint8 | Pushint16 | Pushint32 | Pushint64 | Pushint128 | Pushint256 | PushT | PushF
        | PushA | PushNull | Pushdata1 | Pushdata2 | Pushdata4 | PushM1 | Push0 | Push1 | Push2
        | Push3 | Push4 | Push5 | Push6 | Push7 | Push8 | Push9 | Push10 | Push11 | Push12
        | Push13 | Push14 | Push15 | Push16 | Nop | Jmp | Jmp_L | Jmpif | Jmpif_L | Jmpifnot
        | Jmpifnot_L | JmpEq | JmpEq_L | JmpNe | JmpNe_L | JmpGt | JmpGt_L | JmpGe | JmpGe_L
        | JmpLt | JmpLt_L | JmpLe | JmpLe_L | Call | Call_L | CallA | CallT | Ret | Depth
        | Drop | Nip | Clear | Dup | Over | Tuck | Swap | Rot | Reverse3 | Reverse4 | Initsslot
        | Initslot | Ldsfld0 | Ldsfld1 | Ldsfld2 | Ldsfld3 | Ldsfld4 | Ldsfld5 | Ldsfld6
        | Ldsfld | Stsfld0 | Stsfld1 | Stsfld2 | Stsfld3 | Stsfld4 | Stsfld5 | Stsfld6 | Stsfld
        | Ldloc0 | Ldloc1 | Ldloc2 | Ldloc3 | Ldloc4 | Ldloc5 | Ldloc6 | Ldloc | Stloc0
        | Stloc1 | Stloc2 | Stloc3 | Stloc4 | Stloc5 | Stloc6 | Stloc | Ldarg0 | Ldarg1
        | Ldarg2 | Ldarg3 | Ldarg4 | Ldarg5 | Ldarg6 | Ldarg | Starg0 | Starg1 | Starg2
        | Starg3 | Starg4 | Starg5 | Starg6 | Starg | Newbuffer | Memcpy | Cat | Substr | Left
        | Right | Invert | And | Or | Xor | Equal | Notequal | Sign | Abs | Negate | Inc | Dec
        | Add | Sub | Mul | Div | Mod | Pow | Sqrt | Modmul | Modpow | Shl | Shr | Not
        | Booland | Boolor | Nz | Numequal | Numnotequal | Lt | Le | Gt | Ge | Min | Max
        | Within | Newarray0 | Newarray | NewarrayT | Newstruct0 | Newstruct | Newmap | Pack
        | Packmap | Packstruct | Unpack | Size | Haskey | Keys | Values | Pickitem | Append
        | Setitem | Reverseitems | Remove | Clearitems | Popitem | Isnull | Istype | Convert
        | Assert | Assertmsg | Throw | Try | TryL | Endtry | EndtryL | Endfinally => {
            OpcodeFidelity::Exact
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        classify_opcode, lower_method_body, register_structured_temporaries, Fidelity,
        FidelityReport, LoweringIssue, LoweringIssueKind, MethodIrRequest, MethodSymbolTypes,
        OpcodeFidelity, StatementId, SymbolInfo, SymbolOrigin,
    };
    use crate::decompiler::analysis::method_contracts::ReturnBehavior;
    use crate::decompiler::analysis::types::ValueType;
    use crate::decompiler::cfg::ssa::MethodContext;
    use crate::decompiler::high_level::MAX_HIGH_LEVEL_METHOD_INSTRUCTIONS;
    use crate::decompiler::ir::{
        render_block, Block, Expr, Intrinsic, Literal, SemanticCallTarget, Stmt,
    };
    use crate::instruction::{Instruction, OpCode, Operand};
    use std::collections::{BTreeMap, BTreeSet};

    fn instruction(offset: usize, opcode: OpCode) -> Instruction {
        Instruction::new(offset, opcode, None)
    }

    #[test]
    fn all_known_opcodes_have_an_explicit_classification() {
        let known = OpCode::all_known();
        assert!(!known.is_empty());
        for opcode in known {
            assert!(!matches!(opcode, OpCode::Unknown(_)));
            let _classification = classify_opcode(opcode);
        }
        assert!(matches!(
            classify_opcode(OpCode::Unknown(0xFF)),
            OpcodeFidelity::Incomplete(_)
        ));
    }

    #[test]
    fn type_operand_opcodes_are_exact_once_tags_are_preserved() {
        for opcode in [OpCode::Convert, OpCode::Istype, OpCode::NewarrayT] {
            assert_eq!(
                classify_opcode(opcode),
                OpcodeFidelity::Exact,
                "{opcode:?} carries its operand type tag in structured IR"
            );
        }
    }

    #[test]
    fn cat_temporaries_preserve_known_byte_container_types() {
        let cat = |left, right| {
            Expr::call(
                SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Cat)),
                vec![left, right],
            )
        };
        let text = || Expr::Literal(Literal::String("text".to_string()));
        let body = Block::with_stmts(vec![
            Stmt::assign("text0", cat(text(), text())),
            Stmt::assign("text1", cat(Expr::var("text0"), text())),
            Stmt::assign("buffer0", cat(Expr::var("buffer"), text())),
            Stmt::assign("unknown0", cat(Expr::var("unknown"), text())),
        ]);
        let mut symbols = BTreeMap::from([
            (
                "buffer".to_string(),
                SymbolInfo {
                    origin: SymbolOrigin::Local(0),
                    value_type: ValueType::Buffer,
                },
            ),
            (
                "unknown".to_string(),
                SymbolInfo {
                    origin: SymbolOrigin::Local(1),
                    value_type: ValueType::Unknown,
                },
            ),
        ]);

        register_structured_temporaries(&body, &mut symbols);

        assert_eq!(symbols["text0"].value_type, ValueType::ByteString);
        assert_eq!(symbols["text1"].value_type, ValueType::ByteString);
        assert_eq!(symbols["buffer0"].value_type, ValueType::Buffer);
        assert_eq!(symbols["unknown0"].value_type, ValueType::Unknown);
    }

    #[test]
    fn dynamic_stack_opcodes_defer_fidelity_to_literal_resolution() {
        for opcode in [OpCode::Pick, OpCode::Roll, OpCode::Xdrop, OpCode::Reversen] {
            assert_eq!(
                classify_opcode(opcode),
                OpcodeFidelity::Exact,
                "{opcode:?} is validated against its resolved operand by the SSA builder"
            );
        }
    }

    #[test]
    fn report_finish_sorts_and_deduplicates_by_diagnostic_identity() {
        let duplicate = |fidelity| LoweringIssue {
            offset: 7,
            opcode: OpCode::Syscall,
            kind: LoweringIssueKind::MissingProvenance,
            fidelity,
            detail: "low-level syscall wrapper".to_string(),
        };
        let mut report = FidelityReport::exact(3);
        report.issues = vec![
            duplicate(Fidelity::Conservative),
            LoweringIssue {
                offset: 2,
                opcode: OpCode::Assert,
                kind: LoweringIssueKind::UnsupportedOpcode,
                fidelity: Fidelity::Incomplete,
                detail: "assertion effect is not represented".to_string(),
            },
            duplicate(Fidelity::Incomplete),
        ];

        report.finish();

        assert_eq!(report.status, Fidelity::Incomplete);
        assert_eq!(report.issues.len(), 2);
        assert_eq!(report.issues[0].offset, 2);
        assert_eq!(report.issues[1].offset, 7);
        assert_eq!(report.issues[1].fidelity, Fidelity::Incomplete);
    }

    #[test]
    fn lowers_only_the_exact_slice_with_neutral_source_symbols() {
        let instructions = vec![
            instruction(0, OpCode::Assert),
            Instruction::new(10, OpCode::Initslot, Some(Operand::Bytes(vec![1, 1]))),
            instruction(11, OpCode::Ldarg0),
            instruction(12, OpCode::Stloc0),
            instruction(13, OpCode::Ldloc0),
            instruction(14, OpCode::Stsfld1),
            instruction(15, OpCode::Ldsfld1),
            instruction(16, OpCode::Ret),
            instruction(20, OpCode::Pack),
        ];
        let request = MethodIrRequest {
            start: 10,
            end: 17,
            instructions: &instructions,
            context: MethodContext {
                argument_names: vec!["amount".to_string()],
                returns_value: Some(true),
                ..MethodContext::default()
            },
            symbol_types: MethodSymbolTypes {
                parameters: vec![ValueType::Integer],
                locals: vec![ValueType::Boolean],
                statics: vec![ValueType::Unknown, ValueType::ByteString],
            },
        };

        let lowered = lower_method_body(request);

        assert_eq!(
            lowered.fidelity.status,
            Fidelity::Exact,
            "{:#?}",
            lowered.fidelity.issues
        );
        assert_eq!(lowered.fidelity.instruction_count, 7);
        assert_eq!(
            lowered.fidelity.covered_offsets,
            std::collections::BTreeSet::from([10, 11, 12, 13, 14, 15, 16])
        );
        assert_eq!(lowered.symbols["amount"].origin, SymbolOrigin::Parameter(0));
        assert_eq!(lowered.symbols["amount"].value_type, ValueType::Integer);
        assert_eq!(lowered.symbols["loc0"].origin, SymbolOrigin::Local(0));
        assert_eq!(lowered.symbols["loc0"].value_type, ValueType::Boolean);
        assert_eq!(lowered.symbols["static1"].origin, SymbolOrigin::Static(1));
        assert_eq!(lowered.symbols["static1"].value_type, ValueType::ByteString);
        assert_eq!(lowered.return_behavior, ReturnBehavior::Value);
        assert!(!lowered.source_map.statement_origins.is_empty());
        assert!(lowered
            .source_map
            .statement_origins
            .values()
            .all(|origins| origins.iter().all(|offset| (10..17).contains(offset))));

        let rendered = render_block(&lowered.body, 0);
        assert!(!rendered.contains("arg0"), "{rendered}");
        assert!(!rendered.contains("loc0_"), "{rendered}");
        assert!(!rendered.contains("static1_"), "{rendered}");
    }

    #[test]
    fn catch_exception_symbol_is_a_dynamic_vm_payload() {
        let instructions = vec![
            Instruction::new(0, OpCode::Try, Some(Operand::Bytes(vec![6, 0]))),
            instruction(3, OpCode::Nop),
            Instruction::new(4, OpCode::Endtry, Some(Operand::Jump(5))),
            instruction(6, OpCode::Drop),
            Instruction::new(7, OpCode::Endtry, Some(Operand::Jump(2))),
            instruction(9, OpCode::Ret),
        ];
        let lowered = lower_method_body(MethodIrRequest {
            start: 0,
            end: 10,
            instructions: &instructions,
            context: MethodContext {
                returns_value: Some(false),
                ..MethodContext::default()
            },
            symbol_types: MethodSymbolTypes::default(),
        });

        assert_eq!(
            lowered.fidelity.status,
            Fidelity::Exact,
            "{:#?}",
            lowered.fidelity.issues
        );
        let (payload_name, payload) = lowered
            .symbols
            .iter()
            .find(|(_, symbol)| symbol.origin == SymbolOrigin::ExceptionPayload)
            .expect("handler payload symbol");
        assert_eq!(payload.value_type, ValueType::Any);
        let rendered = render_block(&lowered.body, 0);
        assert!(
            rendered.contains(&format!("catch({payload_name})")),
            "{rendered}"
        );
        assert!(!rendered.contains('?'), "{rendered}");
    }

    #[test]
    fn source_map_unions_offsets_for_folded_return() {
        let instructions = vec![
            instruction(0, OpCode::Push1),
            instruction(1, OpCode::Push1),
            instruction(2, OpCode::Add),
            instruction(3, OpCode::Ret),
        ];
        let lowered = lower_method_body(MethodIrRequest {
            start: 0,
            end: 4,
            instructions: &instructions,
            context: MethodContext {
                returns_value: Some(true),
                ..MethodContext::default()
            },
            symbol_types: MethodSymbolTypes::default(),
        });
        assert_eq!(lowered.fidelity.status, Fidelity::Exact);
        assert_eq!(
            lowered.source_map.statement_origins.get(&StatementId(0)),
            Some(&BTreeSet::from([0, 1, 2, 3]))
        );
    }

    #[test]
    fn rejects_an_oversized_slice_before_cfg_construction() {
        let instructions: Vec<_> = (0..=MAX_HIGH_LEVEL_METHOD_INSTRUCTIONS)
            .map(|offset| instruction(offset, OpCode::Nop))
            .collect();
        let request = MethodIrRequest {
            start: 0,
            end: instructions.len(),
            instructions: &instructions,
            context: MethodContext::default(),
            symbol_types: MethodSymbolTypes::default(),
        };

        let lowered = lower_method_body(request);

        assert!(lowered.body.is_empty());
        assert_eq!(lowered.fidelity.status, Fidelity::Incomplete);
        assert_eq!(lowered.fidelity.instruction_count, instructions.len());
        assert!(lowered.fidelity.covered_offsets.is_empty());
        assert!(lowered.fidelity.issues.iter().any(|issue| {
            issue.offset == 0
                && issue.opcode == OpCode::Nop
                && issue.kind == LoweringIssueKind::BudgetExceeded
                && issue.fidelity == Fidelity::Incomplete
        }));
    }

    #[test]
    fn unknown_merge_value_keeps_the_method_incomplete() {
        let instructions = vec![
            instruction(0, OpCode::Push1),
            Instruction::new(1, OpCode::Jmpif, Some(Operand::Jump(4))),
            instruction(3, OpCode::Push1),
            Instruction::new(4, OpCode::Jmp, Some(Operand::Jump(2))),
            instruction(5, OpCode::Nop),
            instruction(6, OpCode::Ret),
        ];
        let request = MethodIrRequest {
            start: 0,
            end: 7,
            instructions: &instructions,
            context: MethodContext {
                returns_value: Some(true),
                ..MethodContext::default()
            },
            symbol_types: MethodSymbolTypes::default(),
        };

        let lowered = lower_method_body(request);

        assert_eq!(lowered.fidelity.status, Fidelity::Incomplete);
        assert!(lowered
            .fidelity
            .issues
            .iter()
            .any(|issue| issue.kind == LoweringIssueKind::LostStackValue));
    }

    #[test]
    fn preserves_unknown_return_behavior() {
        let instructions = vec![instruction(0, OpCode::Push1), instruction(1, OpCode::Ret)];
        let lowered = lower_method_body(MethodIrRequest {
            start: 0,
            end: 2,
            instructions: &instructions,
            context: MethodContext::default(),
            symbol_types: MethodSymbolTypes::default(),
        });

        assert_eq!(lowered.return_behavior, ReturnBehavior::Unknown);
    }
}
