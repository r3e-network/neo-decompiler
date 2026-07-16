use std::collections::{BTreeMap, BTreeSet};

use crate::decompiler::analysis::method_contracts::ReturnBehavior;
use crate::decompiler::analysis::types::ValueType;
use crate::decompiler::cfg::ssa::{optimize_ssa, MethodContext, SsaBuilder};
use crate::decompiler::cfg::structure_cfg_with_source_names;
use crate::decompiler::high_level::MAX_HIGH_LEVEL_METHOD_INSTRUCTIONS;
use crate::decompiler::ir::Block;
use crate::instruction::{Instruction, OpCode, Operand};

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

pub(crate) use cfg::{build_method_cfg, build_method_cfg_with_non_returning_calls};
#[cfg(test)]
pub(crate) use opcode::classify_opcode;
pub(crate) use opcode::{classify_instruction, OpcodeFidelity};
use symbols::allocate_source_symbols;

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
    let pointer_values = instructions
        .iter()
        .filter_map(|instruction| {
            (instruction.opcode == OpCode::PushA)
                .then(|| match instruction.operand.as_ref() {
                    Some(Operand::I32(delta)) => instruction
                        .offset
                        .checked_add_signed(*delta as isize)
                        .and_then(|target| i64::try_from(target).ok()),
                    _ => None,
                })
                .flatten()
        })
        .collect::<BTreeSet<_>>();
    types::register_structured_temporaries_with_call_types(
        &body,
        &mut symbols,
        &request.context.call_return_types,
        &pointer_values,
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
    types::register_structured_temporaries(body, symbols);
}

#[cfg(test)]
#[path = "method_body_tests.rs"]
mod tests;
