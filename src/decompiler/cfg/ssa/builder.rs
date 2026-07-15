//! Stack-effect SSA construction from a CFG and instruction stream.
//!
//! This replaces the earlier PUSH-only skeleton with a genuine stack-machine
//! SSA: every opcode's `(pop, push)` effect is modelled (see the `effects`
//! module), the
//! eval stack is tracked symbolically as `Vec<SsaVariable>`, and φ nodes are
//! placed at control-flow joins where predecessors disagree on a stack slot.
//!
//! ### Algorithm
//!
//! 1. Compute dominance (Cooper-Harvey-Kennedy) — gives idom / dominator tree
//!    / dominance frontiers (used by downstream analyses and exposed on
//!    [`SsaForm`]).
//! 2. Fixpoint over blocks in program order:
//!    - Compute each block's **entry symbolic stack** from its predecessors'
//!      exit stacks. Where predecessors agree on a slot, the value flows
//!      through unchanged; where they disagree, a φ node is placed (canonical
//!      target per `(block, depth)`).
//!    - **Execute** the block straight-line: each compute opcode pops N uses
//!      and pushes a fresh SSA definition carrying a real [`SsaExpr`] (binary,
//!      unary, literal, or a `Call` placeholder); reorder opcodes transform the
//!      symbolic stack directly.
//!    - Repeat until exit stacks and φ sets stop changing.
//!
//! Convergence is guaranteed because each block's exit-slot *identity* is
//! canonical (`b{block}_v{ordinal}`) and thus deterministic, so the join
//! structure reaches a fixed point within a small number of passes.
//!
//! The result carries real def/use chains and φ nodes suitable for the
//! constant-propagation / DCE passes (Phase 3).

#![allow(clippy::needless_return)]

use std::collections::{BTreeMap, BTreeSet};

use crate::decompiler::analysis::types::ValueType;
use crate::decompiler::cfg::method_body::{FidelityReport, LoweringIssue, LoweringIssueKind};
use crate::decompiler::cfg::{BlockId, Cfg};
use crate::decompiler::helpers::{
    printable_utf8, signed_le_bytes_to_decimal, value_type_from_operand,
};
use crate::decompiler::ir::{BinOp, Intrinsic, Literal, SemanticCallTarget, UnaryOp};
use crate::instruction::{Instruction, OpCode, Operand};

use super::context::{
    CollectionArgumentEffect, CollectionShape, CollectionShapeFacts, MethodContext,
};
use super::dominance::{self, DominanceInfo};
use super::effects;
use super::form::{SsaBlock, SsaExpr, SsaForm, SsaStmt, UseSite};
use super::variable::SsaVariable;

mod collection;
mod diagnostics;
mod expr;
mod helpers;
mod instructions;
mod joins;
mod pipeline;
mod slots;

use collection::*;
use diagnostics::*;
use helpers::*;
use slots::*;

/// `(blocks, definitions, uses, covered offsets, issues)` from the final pass.
type SsaBuildResult = (
    BTreeMap<BlockId, SsaBlock>,
    BTreeMap<SsaVariable, BlockId>,
    BTreeMap<SsaVariable, BTreeSet<UseSite>>,
    BTreeSet<usize>,
    Vec<LoweringIssue>,
    Option<CollectionShape>,
    Option<CollectionShapeFacts>,
    SsaCollectionAnalysis,
);

/// SSA plus instruction-level semantic fidelity from the stabilized build.
#[derive(Debug)]
pub(crate) struct SsaBuildOutput {
    pub(crate) ssa: SsaForm,
    pub(crate) fidelity: FidelityReport,
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) return_shape: Option<CollectionShape>,
    pub(crate) return_facts: Option<CollectionShapeFacts>,
    pub(crate) collection_analysis: SsaCollectionAnalysis,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SsaCollectionAnalysis {
    pub(crate) argument_field_writes: Vec<BTreeMap<usize, CollectionShape>>,
    pub(crate) static_writes: Vec<StaticCollectionWrite>,
    pub(crate) call_argument_facts: BTreeMap<usize, Vec<CollectionShapeFacts>>,
}

#[derive(Debug, Clone)]
pub(crate) struct StaticCollectionWrite {
    pub(crate) index: usize,
    pub(crate) facts: Option<CollectionShapeFacts>,
    pub(crate) is_null: bool,
    pub(crate) provisional: bool,
}

struct BuildPassState<'a> {
    issues: &'a mut Vec<LoweringIssue>,
    tainted_variables: &'a BTreeSet<SsaVariable>,
    versions: &'a mut BTreeMap<String, usize>,
    definition_facts: &'a mut DefinitionFacts,
    invalidated_collection_content_roots: &'a mut BTreeSet<SsaVariable>,
    invalidated_collection_roots: &'a mut BTreeSet<SsaVariable>,
    invalidated_static_collection_shapes: &'a mut BTreeSet<usize>,
    indexed_collection_shapes: &'a mut BTreeMap<SsaVariable, BTreeMap<usize, CollectionShape>>,
    static_collection_writes: &'a mut Vec<StaticCollectionWrite>,
    call_argument_facts: &'a mut BTreeMap<usize, Vec<CollectionShapeFacts>>,
}

struct DefinitionFact {
    expression: SsaExpr,
    // PUSHA also lowers to Literal::Int, but it is not GetInteger-compatible.
    is_integer_literal: bool,
    collection_shape: Option<CollectionShape>,
    indexed_shapes: BTreeMap<usize, CollectionShape>,
    is_collection_root: bool,
    static_indexes: BTreeSet<usize>,
}

type DefinitionFacts = BTreeMap<SsaVariable, DefinitionFact>;

#[derive(Default)]
struct BuildFacts {
    versions: BTreeMap<String, usize>,
    definitions: DefinitionFacts,
}

#[derive(Clone, Default, PartialEq, Eq)]
struct CollectionInvalidations {
    contents: BTreeSet<SsaVariable>,
    shapes: BTreeSet<SsaVariable>,
    static_shapes: BTreeSet<usize>,
    indexed_shapes: BTreeMap<SsaVariable, BTreeMap<usize, CollectionShape>>,
}

/// Builder for stack-effect SSA form from a CFG and instructions.
pub struct SsaBuilder<'a> {
    cfg: &'a Cfg,
    instructions: &'a [Instruction],
    dominance: DominanceInfo,
    method_context: Option<&'a MethodContext>,
}

impl<'a> SsaBuilder<'a> {
    /// Create a new SSA builder for the given CFG and instructions.
    #[must_use]
    pub fn new(cfg: &'a Cfg, instructions: &'a [Instruction]) -> Self {
        let dominance = dominance::compute(cfg);
        Self {
            cfg,
            instructions,
            dominance,
            method_context: None,
        }
    }

    /// Attach source-level method and call metadata to this SSA build.
    #[must_use]
    pub(crate) fn with_method_context(mut self, context: &'a MethodContext) -> Self {
        self.method_context = Some(context);
        self
    }

    /// Build the stack-effect SSA form from the CFG and instructions.
    #[must_use]
    pub fn build(self) -> SsaForm {
        self.build_with_report().ssa
    }

    /// Build SSA and report any semantic fidelity loss at its source instruction.
    #[must_use]
    pub(crate) fn build_with_report(self) -> SsaBuildOutput {
        let (
            blocks,
            definitions,
            uses,
            covered_offsets,
            issues,
            return_shape,
            return_facts,
            collection_analysis,
        ) = self.build_ssa_blocks();
        let ssa = SsaForm {
            cfg: self.cfg.clone(),
            dominance: self.dominance,
            blocks,
            definitions,
            uses,
        };
        let mut fidelity = FidelityReport::exact(self.instructions.len());
        fidelity.covered_offsets = covered_offsets;
        fidelity.issues = issues;
        fidelity.finish();
        SsaBuildOutput {
            ssa,
            fidelity,
            return_shape,
            return_facts,
            collection_analysis,
        }
    }
}

// ─────────────────────────── helpers ───────────────────────────

/// Straight-line execution result for one block.
#[derive(Default)]
struct BlockExec {
    exit_stack: Vec<SsaVariable>,
    exit_slots: SlotState,
    exit_collection_invalidations: CollectionInvalidations,
    stmts: Vec<SsaStmt>,
    uses: Vec<(SsaVariable, usize)>,
    terminator_condition: Option<SsaVariable>,
    covered_offsets: BTreeSet<usize>,
    issues: Vec<LoweringIssue>,
    return_shapes: Vec<Option<CollectionShape>>,
    return_facts: Vec<Option<CollectionShapeFacts>>,
    argument_field_writes: Vec<Vec<BTreeMap<usize, CollectionShape>>>,
    static_collection_writes: Vec<StaticCollectionWrite>,
    call_argument_facts: BTreeMap<usize, Vec<CollectionShapeFacts>>,
}

/// Canonical SSA variable for the φ placed at `depth` in `block`.
fn phi_var(block: BlockId, depth: usize) -> SsaVariable {
    SsaVariable::new(format!("p{}", block.0), depth)
}

/// Mint a fresh SSA variable for a new definition with the given `base` name,
/// drawing the next version from the per-pass counter `versions`. The counter
/// is reset at the start of each fixpoint pass and increments in deterministic
/// (block-id, instruction) order, so the same def-site always receives the same
/// version — this keeps exit-stack identity stable and the fixpoint convergent.
fn fresh_var(versions: &mut BTreeMap<String, usize>, base: &str) -> SsaVariable {
    let slot = versions.entry(base.to_string()).or_insert(0);
    let var = SsaVariable::new(base.to_string(), *slot);
    *slot += 1;
    var
}

/// Per-block reaching definition for each named slot (`"loc0"` → latest SSA
/// version). Threaded through `execute_block`; stores define a new version,
/// loads read the reaching version, and at joins `compute_join_slots` places φ
/// where predecessors disagree.
/// Placeholder used when the symbolic stack underflows (malformed input).
fn unknown_var() -> SsaVariable {
    SsaVariable::new("?".to_string(), 0)
}

fn is_unknown(v: &SsaVariable) -> bool {
    v.base == "?"
}

fn is_unknown_or_tainted(
    variable: &SsaVariable,
    tainted_variables: &BTreeSet<SsaVariable>,
) -> bool {
    is_unknown(variable) || tainted_variables.contains(variable)
}

/// Reverse the top `n` slots of the symbolic stack in place.
fn reverse_top(stack: &mut [SsaVariable], n: usize) {
    let len = stack.len();
    if n <= 1 || n > len {
        return;
    }
    stack[len - n..].reverse();
}

#[cfg(test)]
mod tests;
