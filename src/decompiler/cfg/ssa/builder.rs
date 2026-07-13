//! Stack-effect SSA construction from a CFG and instruction stream.
//!
//! This replaces the earlier PUSH-only skeleton with a genuine stack-machine
//! SSA: every opcode's `(pop, push)` effect is modelled (see [`effects`]), the
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
use crate::decompiler::cfg::method_body::{
    classify_opcode, Fidelity, FidelityReport, LoweringIssue, LoweringIssueKind, OpcodeFidelity,
};
use crate::decompiler::cfg::{BlockId, Cfg, EdgeKind};
use crate::decompiler::helpers::{
    printable_utf8, signed_le_bytes_to_decimal, value_type_from_operand,
};
use crate::decompiler::ir::{BinOp, Intrinsic, Literal, SemanticCallTarget, UnaryOp};
use crate::instruction::{Instruction, OpCode, Operand, OperandEncoding};

use super::context::{
    CollectionArgumentEffect, CollectionShape, CollectionShapeFacts, MethodContext,
};
use super::dominance::{self, DominanceInfo};
use super::effects;
use super::form::{SsaBlock, SsaExpr, SsaForm, SsaStmt, UseSite};
use super::variable::SsaVariable;

mod collection;
mod expr;
mod helpers;
mod instructions;
mod slots;

use collection::*;
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
    SsaCollectionAnalysis,
);

/// SSA plus instruction-level semantic fidelity from the stabilized build.
#[derive(Debug)]
pub(crate) struct SsaBuildOutput {
    pub(crate) ssa: SsaForm,
    pub(crate) fidelity: FidelityReport,
    pub(crate) return_shape: Option<CollectionShape>,
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
        let (blocks, definitions, uses, covered_offsets, issues, return_shape, collection_analysis) =
            self.build_ssa_blocks();
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
            collection_analysis,
        }
    }

    /// Run the fixpoint that produces per-block φ nodes, exit stacks, and the
    /// assembled [`SsaForm`] pieces.
    fn build_ssa_blocks(&self) -> SsaBuildResult {
        let block_ids: Vec<BlockId> = self.cfg.blocks().map(|b| b.id).collect();
        let reachable_blocks = self.cfg.reachable_blocks();

        // Work space: per-block entry/exit symbolic stacks and slot states.
        // Exit-stack / exit-slot *identity* is canonical per def-site, so the
        // loop converges once the join structure stops changing.
        let mut entry_stacks: BTreeMap<BlockId, Vec<SsaVariable>> = BTreeMap::new();
        let mut exit_stacks: BTreeMap<BlockId, Vec<SsaVariable>> = BTreeMap::new();
        let mut entry_slots: BTreeMap<BlockId, SlotState> = BTreeMap::new();
        let mut exit_slots: BTreeMap<BlockId, SlotState> = BTreeMap::new();
        let mut entry_collection_invalidations: BTreeMap<BlockId, CollectionInvalidations> =
            BTreeMap::new();
        let mut exit_collection_invalidations: BTreeMap<BlockId, CollectionInvalidations> =
            BTreeMap::new();
        let mut block_uses: BTreeMap<BlockId, Vec<(SsaVariable, usize)>> = BTreeMap::new();
        // Per-pass variable-version counter. Reset at the start of every pass so
        // the deterministic (block-id, instruction) def order yields identical
        // names across iterations → stable exit stacks → fixpoint convergence.
        let mut facts = BuildFacts::default();

        // Upper bound on iterations: a couple of passes beyond the block count
        // is plenty for reducible + irreducible graphs given canonical naming.
        let max_iterations = block_ids.len() + 4;
        let mut changed = true;
        let mut iterations = 0usize;
        let no_tainted_variables = BTreeSet::new();
        while changed && iterations <= max_iterations {
            changed = false;
            iterations += 1;
            facts.versions.clear();
            facts.definitions.clear();
            self.reserve_argument_versions(&mut facts.versions);
            self.seed_context_collection_facts(&mut facts.definitions);
            for &bid in &block_ids {
                let (new_entry, _new_phis) = self.compute_join_entry(bid, &exit_stacks);
                let (new_slot_entry, _new_slot_phis) =
                    self.compute_join_slots(bid, &exit_slots, &mut facts.versions);
                let new_collection_invalidations =
                    self.compute_join_collection_invalidations(bid, &exit_collection_invalidations);
                let exec = self.execute_block(
                    bid,
                    &new_entry,
                    &new_slot_entry,
                    &new_collection_invalidations,
                    &no_tainted_variables,
                    &mut facts,
                );

                let exit_changed = exit_stacks.get(&bid) != Some(&exec.exit_stack);
                let entry_changed = entry_stacks.get(&bid) != Some(&new_entry);
                let slot_exit_changed = exit_slots.get(&bid) != Some(&exec.exit_slots);
                let slot_entry_changed = entry_slots.get(&bid) != Some(&new_slot_entry);
                let collection_exit_changed = exit_collection_invalidations.get(&bid)
                    != Some(&exec.exit_collection_invalidations);
                let collection_entry_changed =
                    entry_collection_invalidations.get(&bid) != Some(&new_collection_invalidations);
                if exit_changed
                    || entry_changed
                    || slot_exit_changed
                    || slot_entry_changed
                    || collection_exit_changed
                    || collection_entry_changed
                {
                    changed = true;
                }
                entry_stacks.insert(bid, new_entry);
                exit_stacks.insert(bid, exec.exit_stack);
                entry_slots.insert(bid, new_slot_entry);
                exit_slots.insert(bid, exec.exit_slots);
                entry_collection_invalidations.insert(bid, new_collection_invalidations);
                exit_collection_invalidations.insert(bid, exec.exit_collection_invalidations);
                block_uses.insert(bid, exec.uses);
            }
        }

        // Final pass: recompute phis from the stabilised exit stacks and assemble.
        let mut ssa_blocks = BTreeMap::new();
        let mut definitions = BTreeMap::new();
        let mut uses: BTreeMap<SsaVariable, BTreeSet<UseSite>> = BTreeMap::new();
        let mut covered_offsets = BTreeSet::new();
        let mut issues = Vec::new();
        let mut return_shapes = Vec::new();
        let mut argument_field_writes = Vec::new();
        let mut static_collection_writes = Vec::new();
        let mut call_argument_facts = BTreeMap::new();
        let tainted_variables = self.tainted_phi_targets(
            &block_ids,
            &entry_stacks,
            &exit_stacks,
            &entry_slots,
            &exit_slots,
        );

        facts.versions.clear();
        facts.definitions.clear();
        self.reserve_argument_versions(&mut facts.versions);
        self.seed_context_collection_facts(&mut facts.definitions);
        for &bid in &block_ids {
            let entry = entry_stacks.get(&bid).cloned().unwrap_or_default();
            let slot_entry = entry_slots.get(&bid).cloned().unwrap_or_default();
            let collection_invalidations = entry_collection_invalidations
                .get(&bid)
                .cloned()
                .unwrap_or_default();
            let (_, stack_phis) = self.compute_join_entry(bid, &exit_stacks);
            let (_, slot_phis) = self.compute_join_slots(bid, &exit_slots, &mut facts.versions);
            let exec = self.execute_block(
                bid,
                &entry,
                &slot_entry,
                &collection_invalidations,
                &tainted_variables,
                &mut facts,
            );
            covered_offsets.extend(exec.covered_offsets.iter().copied());
            if reachable_blocks.contains(&bid) {
                issues.extend(exec.issues.iter().cloned());
                return_shapes.extend(exec.return_shapes.iter().copied());
                argument_field_writes.extend(exec.argument_field_writes.iter().cloned());
                static_collection_writes.extend(exec.static_collection_writes.iter().cloned());
                call_argument_facts.extend(
                    exec.call_argument_facts
                        .iter()
                        .map(|(offset, facts)| (*offset, facts.clone())),
                );
            }

            let mut sb = SsaBlock::new();
            for phi in stack_phis.iter().chain(slot_phis.iter()) {
                definitions.insert(phi.target.clone(), bid);
                // φ operands are uses at the block head (stmt_index 0).
                for var in phi.operands.values() {
                    uses.entry(var.clone())
                        .or_default()
                        .insert(UseSite::new(bid, 0));
                }
                sb.add_phi(phi.clone());
            }
            for (i, stmt) in exec.stmts.iter().enumerate() {
                if let SsaStmt::Assign { target, value } = stmt {
                    definitions.insert(target.clone(), bid);
                    for used in collect_expr_uses(value) {
                        uses.entry(used).or_default().insert(UseSite::new(bid, i));
                    }
                }
                sb.add_stmt(stmt.clone());
            }
            if let Some(condition) = exec.terminator_condition {
                uses.entry(condition)
                    .or_default()
                    .insert(UseSite::terminator(bid));
            }
            // Fold in uses recorded for non-Assign consumers (stores, jumps, …).
            for (var, idx) in block_uses.get(&bid).cloned().unwrap_or_default() {
                uses.entry(var).or_default().insert(UseSite::new(bid, idx));
            }
            ssa_blocks.insert(bid, sb);
        }

        let return_shape = unanimous_collection_shape(&return_shapes);
        let collection_analysis = SsaCollectionAnalysis {
            argument_field_writes: unanimous_argument_field_writes(&argument_field_writes),
            static_writes: static_collection_writes,
            call_argument_facts,
        };
        (
            ssa_blocks,
            definitions,
            uses,
            covered_offsets,
            issues,
            return_shape,
            collection_analysis,
        )
    }

    fn tainted_phi_targets(
        &self,
        block_ids: &[BlockId],
        entry_stacks: &BTreeMap<BlockId, Vec<SsaVariable>>,
        exit_stacks: &BTreeMap<BlockId, Vec<SsaVariable>>,
        entry_slots: &BTreeMap<BlockId, SlotState>,
        exit_slots: &BTreeMap<BlockId, SlotState>,
    ) -> BTreeSet<SsaVariable> {
        let mut phis = Vec::new();
        let no_tainted_variables = BTreeSet::new();
        let mut facts = BuildFacts::default();
        self.reserve_argument_versions(&mut facts.versions);
        self.seed_context_collection_facts(&mut facts.definitions);
        for bid in block_ids {
            phis.extend(self.compute_join_entry(*bid, exit_stacks).1);
            phis.extend(
                self.compute_join_slots(*bid, exit_slots, &mut facts.versions)
                    .1,
            );
            let entry = entry_stacks.get(bid).cloned().unwrap_or_default();
            let slot_entry = entry_slots.get(bid).cloned().unwrap_or_default();
            let _ = self.execute_block(
                *bid,
                &entry,
                &slot_entry,
                &CollectionInvalidations::default(),
                &no_tainted_variables,
                &mut facts,
            );
        }
        let mut tainted = BTreeSet::from([unknown_var()]);

        loop {
            let mut changed = false;
            for phi in &phis {
                if phi.operands.values().any(|value| tainted.contains(value)) {
                    changed |= tainted.insert(phi.target.clone());
                }
            }
            if !changed {
                break;
            }
        }

        tainted.remove(&unknown_var());
        tainted
    }

    /// Compute a block's entry symbolic stack and the φ nodes it needs, from its
    /// predecessors' current exit stacks. Where all predecessors agree on a
    /// slot the value flows through; where they disagree a φ node is placed.
    fn compute_join_entry(
        &self,
        bid: BlockId,
        exit_stacks: &BTreeMap<BlockId, Vec<SsaVariable>>,
    ) -> (Vec<SsaVariable>, Vec<super::variable::PhiNode>) {
        use super::variable::PhiNode;
        let preds: Vec<_> = self
            .cfg
            .predecessors(bid)
            .iter()
            .copied()
            .filter(|pred| self.cfg.edge_kind(*pred, bid) != Some(EdgeKind::FinallyException))
            .collect();
        let initial_arguments = self.initial_entry_stack(bid);
        if preds.is_empty() {
            return (initial_arguments, Vec::new());
        }

        let mut entry = Vec::new();
        let mut phis = Vec::new();
        let incoming_stacks: Vec<_> = preds
            .iter()
            .filter_map(|pred| match self.cfg.edge_kind(*pred, bid) {
                Some(EdgeKind::Exception) => {
                    Some((*pred, vec![SsaVariable::exception_payload(bid)]))
                }
                _ => exit_stacks.get(pred).cloned().map(|stack| (*pred, stack)),
            })
            .collect();
        let predecessor_depth = incoming_stacks
            .iter()
            .map(|(_, stack)| stack.len())
            .max()
            .unwrap_or(0);
        let max_depth = predecessor_depth.max(initial_arguments.len());
        let entry_source = BlockId::from(usize::MAX);
        for depth in 0..max_depth {
            // A predecessor with a known but shorter stack contributes `?` at
            // this depth. Skipping it would fabricate a value on that path.
            let mut operands: Vec<(BlockId, SsaVariable)> = Vec::new();
            for (pred, stack) in &incoming_stacks {
                let leading_underflow = max_depth.saturating_sub(stack.len());
                let variable = depth
                    .checked_sub(leading_underflow)
                    .and_then(|index| stack.get(index))
                    .cloned()
                    .unwrap_or_else(unknown_var);
                operands.push((*pred, variable));
            }
            if !initial_arguments.is_empty() {
                let leading_underflow = max_depth.saturating_sub(initial_arguments.len());
                let variable = depth
                    .checked_sub(leading_underflow)
                    .and_then(|index| initial_arguments.get(index))
                    .cloned()
                    .unwrap_or_else(unknown_var);
                operands.push((entry_source, variable));
            }
            if operands.is_empty() {
                continue;
            }
            let first = operands[0].1.clone();
            let all_agree = operands.iter().all(|(_, v)| *v == first);
            if all_agree {
                entry.push(first);
            } else {
                let target = phi_var(bid, depth);
                let mut phi = PhiNode::new(target.clone());
                for (pred, var) in &operands {
                    phi.add_operand(*pred, var.clone());
                }
                // φ operands are uses of the incoming values.
                entry.push(target);
                phis.push(phi);
            }
        }

        (entry, phis)
    }

    /// Compute a block's entry slot state and the φ nodes it needs, from its
    /// predecessors' current exit slot states. For each slot name present across
    /// the predecessors: if they all agree, the reaching version flows through;
    /// if they disagree, a φ is placed. The φ target is named after the slot
    /// (`loc0_N`) so downstream `strip_version` keeps it associated with the
    /// slot. `versions` is the per-pass counter, shared with `execute_block` so
    /// φ targets and defs draw from one deterministic namespace.
    fn compute_join_slots(
        &self,
        bid: BlockId,
        exit_slots: &BTreeMap<BlockId, SlotState>,
        versions: &mut BTreeMap<String, usize>,
    ) -> (SlotState, Vec<super::variable::PhiNode>) {
        use super::variable::PhiNode;
        let preds: Vec<_> = self
            .cfg
            .predecessors(bid)
            .iter()
            .copied()
            .filter(|pred| self.cfg.edge_kind(*pred, bid) != Some(EdgeKind::FinallyException))
            .collect();
        let is_entry = self.cfg.entry_block().is_some_and(|entry| entry.id == bid);
        let argument_count = if is_entry {
            self.method_context
                .filter(|context| !context.arguments_on_entry_stack)
                .map_or(0, |context| context.argument_names.len())
        } else {
            0
        };
        if preds.is_empty() && argument_count == 0 {
            return (SlotState::new(), Vec::new());
        }

        // The method entry has a virtual incoming edge carrying ABI arguments.
        // Keep that source in entry-loop phis so a backedge cannot replace the
        // initial parameter value before the first iteration.
        let entry_source = BlockId::from(usize::MAX);
        let mut initial_arguments = SlotState::new();
        let mut names: BTreeSet<String> = BTreeSet::new();
        for index in 0..argument_count {
            let base = format!("arg{index}");
            initial_arguments.insert(base.clone(), SsaVariable::initial(base.clone()));
            names.insert(base.clone());
            versions.entry(base).or_insert(1);
        }

        // Union of slot names any predecessor holds.
        for pred in &preds {
            if let Some(state) = exit_slots.get(pred) {
                for name in state.keys() {
                    names.insert(name.clone());
                }
            }
        }

        let mut entry = SlotState::new();
        let mut phis = Vec::new();
        for name in names {
            if is_static_slot_name(&name) {
                versions.entry(name.clone()).or_insert(1);
            }
            let mut operands: Vec<(BlockId, SsaVariable)> = Vec::new();
            for pred in &preds {
                if let Some(state) = exit_slots.get(pred) {
                    operands.push((
                        *pred,
                        state
                            .get(&name)
                            .cloned()
                            .unwrap_or_else(|| absent_slot_value(&name)),
                    ));
                }
            }
            if is_entry {
                operands.push((
                    entry_source,
                    initial_arguments
                        .get(&name)
                        .cloned()
                        .unwrap_or_else(|| absent_slot_value(&name)),
                ));
            }
            if operands.is_empty() {
                continue;
            }
            let first = operands[0].1.clone();
            let all_agree = operands.iter().all(|(_, v)| *v == first);
            if all_agree {
                entry.insert(name, first);
            } else {
                let target = fresh_var(versions, &name);
                let mut phi = PhiNode::new(target.clone());
                for (pred, var) in &operands {
                    phi.add_operand(*pred, var.clone());
                }
                entry.insert(name, target);
                phis.push(phi);
            }
        }
        (entry, phis)
    }

    fn compute_join_collection_invalidations(
        &self,
        bid: BlockId,
        exit_invalidations: &BTreeMap<BlockId, CollectionInvalidations>,
    ) -> CollectionInvalidations {
        let mut joined = CollectionInvalidations::default();
        let predecessor_invalidations = self
            .cfg
            .predecessors(bid)
            .iter()
            .filter_map(|predecessor| exit_invalidations.get(predecessor))
            .collect::<Vec<_>>();
        for invalidations in &predecessor_invalidations {
            joined
                .contents
                .extend(invalidations.contents.iter().cloned());
            joined.shapes.extend(invalidations.shapes.iter().cloned());
            joined
                .static_shapes
                .extend(invalidations.static_shapes.iter().copied());
        }
        let indexed_roots = predecessor_invalidations
            .iter()
            .flat_map(|invalidations| invalidations.indexed_shapes.keys().cloned())
            .collect::<BTreeSet<_>>();
        for root in indexed_roots {
            let first = predecessor_invalidations
                .first()
                .and_then(|invalidations| invalidations.indexed_shapes.get(&root));
            let unanimous = first.is_some()
                && predecessor_invalidations
                    .iter()
                    .all(|invalidations| invalidations.indexed_shapes.get(&root) == first);
            joined.indexed_shapes.insert(
                root,
                if unanimous {
                    first.cloned().unwrap_or_default()
                } else {
                    BTreeMap::new()
                },
            );
        }
        joined
    }

    fn initial_entry_stack(&self, bid: BlockId) -> Vec<SsaVariable> {
        let is_entry = self.cfg.entry_block().is_some_and(|entry| entry.id == bid);
        let Some(context) = self
            .method_context
            .filter(|context| is_entry && context.arguments_on_entry_stack)
        else {
            return Vec::new();
        };

        (0..context.argument_names.len())
            .rev()
            .map(|index| SsaVariable::initial(format!("arg{index}")))
            .collect()
    }

    fn reserve_argument_versions(&self, versions: &mut BTreeMap<String, usize>) {
        let Some(context) = self.method_context else {
            return;
        };
        let argument_count = context.argument_names.len();
        for index in 0..argument_count {
            versions.insert(format!("arg{index}"), 1);
        }
        for index in context.static_collection_facts.keys() {
            versions.insert(format!("static{index}"), 1);
        }
    }

    fn seed_context_collection_facts(&self, facts: &mut DefinitionFacts) {
        let Some(context) = self.method_context else {
            return;
        };
        for index in 0..context.argument_names.len() {
            let variable = SsaVariable::initial(format!("arg{index}"));
            let shape_facts = context
                .argument_collection_facts
                .get(index)
                .cloned()
                .unwrap_or_default();
            facts.insert(
                variable,
                DefinitionFact {
                    expression: SsaExpr::call(
                        SemanticCallTarget::Unresolved {
                            display_name: format!("argument_{index}"),
                        },
                        Vec::new(),
                    ),
                    is_integer_literal: false,
                    collection_shape: shape_facts.shape,
                    indexed_shapes: shape_facts.indexed,
                    is_collection_root: true,
                    static_indexes: BTreeSet::new(),
                },
            );
        }
        for (index, shape_facts) in &context.static_collection_facts {
            facts.insert(
                SsaVariable::initial(format!("static{index}")),
                DefinitionFact {
                    expression: SsaExpr::call(
                        SemanticCallTarget::Unresolved {
                            display_name: format!("static_{index}"),
                        },
                        Vec::new(),
                    ),
                    is_integer_literal: false,
                    collection_shape: shape_facts.shape,
                    indexed_shapes: shape_facts.indexed.clone(),
                    is_collection_root: true,
                    static_indexes: BTreeSet::from([*index]),
                },
            );
        }
    }

    fn static_collection_facts_for_instruction(
        &self,
        instruction: &Instruction,
        invalidated_static_shapes: &BTreeSet<usize>,
    ) -> Option<CollectionShapeFacts> {
        let index = static_load_index(instruction)?;
        if invalidated_static_shapes.contains(&index) {
            return None;
        }
        self.method_context?
            .static_collection_facts
            .get(&index)
            .cloned()
    }

    /// Symbolically execute one block straight-line from `entry`, producing the
    /// exit stack, the SSA statements, and the use list
    /// (vars consumed by non-assignment opcodes such as stores / conditions).
    fn execute_block(
        &self,
        bid: BlockId,
        entry: &[SsaVariable],
        entry_slots: &SlotState,
        entry_collection_invalidations: &CollectionInvalidations,
        tainted_variables: &BTreeSet<SsaVariable>,
        facts: &mut BuildFacts,
    ) -> BlockExec {
        let Some(block) = self.cfg.block(bid) else {
            return BlockExec::default();
        };
        let mut stack: Vec<SsaVariable> = entry.to_vec();
        let mut slots: SlotState = entry_slots.clone();
        let mut stmts: Vec<SsaStmt> = Vec::new();
        let mut uses: Vec<(SsaVariable, usize)> = Vec::new();
        let mut terminator_condition = None;
        let mut covered_offsets = BTreeSet::new();
        let mut issues = Vec::new();
        let mut collection_invalidations = entry_collection_invalidations.clone();
        let mut static_collection_writes = Vec::new();
        let mut call_argument_facts = BTreeMap::new();

        {
            let mut state = BuildPassState {
                issues: &mut issues,
                tainted_variables,
                versions: &mut facts.versions,
                definition_facts: &mut facts.definitions,
                invalidated_collection_content_roots: &mut collection_invalidations.contents,
                invalidated_collection_roots: &mut collection_invalidations.shapes,
                invalidated_static_collection_shapes: &mut collection_invalidations.static_shapes,
                indexed_collection_shapes: &mut collection_invalidations.indexed_shapes,
                static_collection_writes: &mut static_collection_writes,
                call_argument_facts: &mut call_argument_facts,
            };
            let mut idx = block.instruction_range.start;
            while idx < block.instruction_range.end {
                let Some(instr) = self.instructions.get(idx) else {
                    idx += 1;
                    continue;
                };
                if instr.opcode == OpCode::Drop && stack.len() == 1 {
                    let next_idx = idx + 1;
                    if next_idx < block.instruction_range.end {
                        if let Some(throw) = self
                            .instructions
                            .get(next_idx)
                            .filter(|next| next.opcode == OpCode::Throw)
                        {
                            covered_offsets.insert(instr.offset);
                            covered_offsets.insert(throw.offset);
                            self.apply_drop_bare_throw(
                                instr, throw, &mut stack, &mut stmts, &mut state,
                            );
                            idx += 2;
                            continue;
                        }
                    }
                }
                if instr.opcode == OpCode::Unpack {
                    let next_idx = idx + 1;
                    if next_idx < block.instruction_range.end {
                        if let Some(packstruct) = self
                            .instructions
                            .get(next_idx)
                            .filter(|next| next.opcode == OpCode::Packstruct)
                        {
                            covered_offsets.insert(instr.offset);
                            covered_offsets.insert(packstruct.offset);
                            let statement_start = stmts.len();
                            self.apply_unpack_packstruct(
                                instr, packstruct, &mut stack, &mut stmts, &mut uses, &mut state,
                            );
                            record_definition_facts(
                                &stmts[statement_start..],
                                packstruct.opcode,
                                &mut state,
                                None,
                                None,
                                None,
                            );
                            idx += 2;
                            continue;
                        }
                    }
                }
                covered_offsets.insert(instr.offset);
                let statement_start = stmts.len();
                if let Some(condition) = self.apply_instruction(
                    instr, &mut stack, &mut slots, &mut stmts, &mut uses, &mut state,
                ) {
                    terminator_condition = Some(condition);
                }
                let seeded_static_facts = self.static_collection_facts_for_instruction(
                    instr,
                    state.invalidated_static_collection_shapes,
                );
                record_definition_facts(
                    &stmts[statement_start..],
                    instr.opcode,
                    &mut state,
                    self.method_context
                        .and_then(|context| context.calls_by_offset.get(&instr.offset))
                        .and_then(|contract| contract.return_shape),
                    seeded_static_facts,
                    static_load_index(instr).or_else(|| static_store_index(instr)),
                );
                idx += 1;
            }
        }

        let return_shapes = stmts
            .iter()
            .filter_map(|statement| match statement {
                SsaStmt::Return(value) => Some(value.as_ref().and_then(|value| {
                    collection_shape_for_expression(
                        value,
                        &facts.definitions,
                        &collection_invalidations.shapes,
                    )
                })),
                _ => None,
            })
            .collect();
        let argument_field_writes = stmts
            .iter()
            .filter(|statement| matches!(statement, SsaStmt::Return(_)))
            .map(|_| {
                self.method_context.map_or_else(Vec::new, |context| {
                    (0..context.argument_names.len())
                        .map(|index| {
                            collection_shape_facts_for_variable(
                                &SsaVariable::initial(format!("arg{index}")),
                                &facts.definitions,
                                &collection_invalidations,
                            )
                            .indexed
                        })
                        .collect()
                })
            })
            .collect();

        BlockExec {
            exit_stack: stack,
            exit_slots: slots,
            stmts,
            uses,
            terminator_condition,
            covered_offsets,
            issues,
            exit_collection_invalidations: collection_invalidations,
            return_shapes,
            argument_field_writes,
            static_collection_writes,
            call_argument_facts,
        }
    }
}

// ─────────────────────────── helpers ───────────────────────────

fn record_instruction_ceiling(instruction: &Instruction, issues: &mut Vec<LoweringIssue>) {
    match classify_opcode(instruction.opcode) {
        OpcodeFidelity::Exact => {}
        OpcodeFidelity::Conservative => {
            let detail = if matches!(instruction.opcode, OpCode::Abort | OpCode::Abortmsg) {
                format!(
                    "{} is represented as a catchable exception because an uncatchable VM abort has no direct structured equivalent",
                    instruction.opcode.mnemonic()
                )
            } else {
                format!(
                    "{} is preserved as a low-level call",
                    instruction.opcode.mnemonic()
                )
            };
            issues.push(LoweringIssue {
                offset: instruction.offset,
                opcode: instruction.opcode,
                kind: LoweringIssueKind::MissingProvenance,
                fidelity: Fidelity::Conservative,
                detail,
            });
        }
        OpcodeFidelity::Incomplete(kind) => record_incomplete_issue(
            instruction,
            kind,
            format!(
                "{} semantics are not represented exactly",
                instruction.opcode.mnemonic()
            ),
            issues,
        ),
    }
}

fn record_missing_operand_metadata(instruction: &Instruction, issues: &mut Vec<LoweringIssue>) {
    let encoding = instruction.opcode.operand_encoding();
    if operand_matches_encoding(encoding, instruction.operand.as_ref()) {
        return;
    }
    record_incomplete_issue(
        instruction,
        LoweringIssueKind::MissingOperandMetadata,
        format!(
            "{} requires {encoding:?} operand metadata",
            instruction.opcode.mnemonic()
        ),
        issues,
    );
}

fn operand_matches_encoding(encoding: OperandEncoding, operand: Option<&Operand>) -> bool {
    match (encoding, operand) {
        (OperandEncoding::None, _) => true,
        (OperandEncoding::I8, Some(Operand::I8(_)))
        | (OperandEncoding::I16, Some(Operand::I16(_)))
        | (OperandEncoding::I32, Some(Operand::I32(_)))
        | (OperandEncoding::I64, Some(Operand::I64(_)))
        | (OperandEncoding::Jump8, Some(Operand::Jump(_)))
        | (OperandEncoding::Jump32, Some(Operand::Jump32(_)))
        | (OperandEncoding::U8, Some(Operand::U8(_)))
        | (OperandEncoding::U16, Some(Operand::U16(_)))
        | (OperandEncoding::U32, Some(Operand::U32(_)))
        | (OperandEncoding::Syscall, Some(Operand::Syscall(_)))
        | (
            OperandEncoding::Data1 | OperandEncoding::Data2 | OperandEncoding::Data4,
            Some(Operand::Bytes(_)),
        ) => true,
        (OperandEncoding::Bytes(expected), Some(Operand::Bytes(bytes))) => bytes.len() == expected,
        _ => false,
    }
}

fn record_incomplete_issue(
    instruction: &Instruction,
    kind: LoweringIssueKind,
    detail: impl Into<String>,
    issues: &mut Vec<LoweringIssue>,
) {
    issues.push(LoweringIssue {
        offset: instruction.offset,
        opcode: instruction.opcode,
        kind,
        fidelity: Fidelity::Incomplete,
        detail: detail.into(),
    });
}

fn record_stack_underflow(
    instruction: &Instruction,
    required: usize,
    available: usize,
    issues: &mut Vec<LoweringIssue>,
) {
    record_incomplete_issue(
        instruction,
        LoweringIssueKind::LostStackValue,
        format!("requires {required} stack values, but only {available} are available"),
        issues,
    );
}

fn fixed_reorder_arity(opcode: OpCode) -> Option<usize> {
    match opcode {
        OpCode::Depth => None,
        OpCode::Drop | OpCode::Dup => Some(1),
        OpCode::Nip | OpCode::Over | OpCode::Tuck | OpCode::Swap => Some(2),
        OpCode::Rot | OpCode::Reverse3 => Some(3),
        OpCode::Reverse4 => Some(4),
        _ => None,
    }
}

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
