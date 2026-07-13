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

use collection::*;

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

    fn apply_drop_bare_throw(
        &self,
        drop: &Instruction,
        throw: &Instruction,
        stack: &mut Vec<SsaVariable>,
        stmts: &mut Vec<SsaStmt>,
        state: &mut BuildPassState<'_>,
    ) {
        record_instruction_ceiling(drop, state.issues);
        record_missing_operand_metadata(drop, state.issues);
        record_instruction_ceiling(throw, state.issues);
        record_missing_operand_metadata(throw, state.issues);

        if stack
            .last()
            .is_some_and(|value| state.tainted_variables.contains(value))
        {
            record_incomplete_issue(
                drop,
                LoweringIssueKind::LostStackValue,
                "stack operation consumes an unknown merged value",
                state.issues,
            );
        }
        stack.pop();
        stmts.push(SsaStmt::throw(None));
    }

    fn apply_unpack_packstruct(
        &self,
        unpack: &Instruction,
        packstruct: &Instruction,
        stack: &mut Vec<SsaVariable>,
        stmts: &mut Vec<SsaStmt>,
        uses: &mut Vec<(SsaVariable, usize)>,
        state: &mut BuildPassState<'_>,
    ) {
        record_instruction_ceiling(unpack, state.issues);
        record_missing_operand_metadata(unpack, state.issues);
        record_instruction_ceiling(packstruct, state.issues);
        record_missing_operand_metadata(packstruct, state.issues);

        let underflowed = stack.is_empty();
        if underflowed {
            record_stack_underflow(unpack, 1, 0, state.issues);
        }
        let source = stack.pop().unwrap_or_else(unknown_var);
        if !underflowed && is_unknown_or_tainted(&source, state.tainted_variables) {
            record_incomplete_issue(
                unpack,
                LoweringIssueKind::LostStackValue,
                "UNPACK/PACKSTRUCT clone consumes an unknown stack value",
                state.issues,
            );
        }
        if !is_unknown(&source) {
            uses.push((source.clone(), stmts.len()));
        }

        let target = fresh_var(state.versions, "t");
        stmts.push(SsaStmt::assign(
            target.clone(),
            SsaExpr::call(
                SemanticCallTarget::Intrinsic(Intrinsic::UnpackPackStruct),
                vec![SsaExpr::var(source)],
            ),
        ));
        stack.push(target);
    }

    /// Apply a single instruction's stack effect / transformation.
    fn apply_instruction(
        &self,
        instr: &Instruction,
        stack: &mut Vec<SsaVariable>,
        slots: &mut SlotState,
        stmts: &mut Vec<SsaStmt>,
        uses: &mut Vec<(SsaVariable, usize)>,
        state: &mut BuildPassState<'_>,
    ) -> Option<SsaVariable> {
        let op = instr.opcode;
        record_instruction_ceiling(instr, state.issues);
        if op == OpCode::Endfinally
            && !self.cfg.block_at_offset(instr.offset).is_some_and(|block| {
                matches!(
                    block.terminator,
                    crate::decompiler::cfg::Terminator::EndFinally { .. }
                )
            })
        {
            record_incomplete_issue(
                instr,
                LoweringIssueKind::UnsupportedControl,
                "control-transfer semantics are not represented exactly",
                state.issues,
            );
        }
        record_missing_operand_metadata(instr, state.issues);
        if matches!(op, OpCode::Convert | OpCode::Istype | OpCode::NewarrayT) {
            let target = instr.operand.as_ref().and_then(value_type_from_operand);
            if instr.operand.is_some() && target.is_none() {
                record_incomplete_issue(
                    instr,
                    LoweringIssueKind::MissingOperandMetadata,
                    "operand is not a recognized VM StackItemType tag",
                    state.issues,
                );
            } else if matches!(op, OpCode::Convert | OpCode::Istype)
                && target == Some(ValueType::Any)
            {
                record_incomplete_issue(
                    instr,
                    LoweringIssueKind::MissingOperandMetadata,
                    "StackItemType Any is invalid for CONVERT and ISTYPE",
                    state.issues,
                );
            }
        }

        if op == OpCode::Initslot {
            if let Some(Operand::Bytes(counts)) = &instr.operand {
                if let Some(&local_count) = counts.first() {
                    for index in 0..usize::from(local_count) {
                        let base = format!("loc{index}");
                        slots.insert(base, SsaVariable::vm_null());
                    }
                }
            }
            return None;
        }

        if op == OpCode::Ret {
            let returns_value = self
                .method_context
                .and_then(|context| context.returns_value);
            let value = if returns_value == Some(false) {
                None
            } else {
                stack.last().cloned()
            };
            if returns_value == Some(true) && value.is_none() {
                record_stack_underflow(instr, 1, 0, state.issues);
            } else if value
                .as_ref()
                .is_some_and(|value| is_unknown_or_tainted(value, state.tainted_variables))
            {
                record_incomplete_issue(
                    instr,
                    LoweringIssueKind::LostStackValue,
                    "unknown stack value reaches the method return",
                    state.issues,
                );
            }
            if let Some(value) = &value {
                if !is_unknown(value) {
                    uses.push((value.clone(), stmts.len()));
                }
            }
            stmts.push(SsaStmt::ret(value.map(SsaExpr::var)));
            return None;
        }

        if matches!(
            op,
            OpCode::Call | OpCode::Call_L | OpCode::CallA | OpCode::CallT
        ) {
            if let Some(contract) = self
                .method_context
                .and_then(|context| context.calls_by_offset.get(&instr.offset))
            {
                self.apply_known_call(instr, contract, stack, stmts, uses, state);
            } else {
                self.apply_opaque_call(instr, stack, stmts, uses, state);
            }
            return None;
        }

        if matches!(op, OpCode::Jmp | OpCode::Jmp_L) {
            if let Some(contract) = self
                .method_context
                .and_then(|context| context.calls_by_offset.get(&instr.offset))
            {
                self.apply_known_tail_call(instr, contract, stack, stmts, uses, state);
                return None;
            }
        }

        if effects::is_stack_reorder(op) {
            self.apply_reorder(instr, stack, stmts, state);
            return None;
        }
        if effects::is_stack_special(op) {
            self.apply_special(instr, stack, stmts, uses, state);
            return None;
        }

        let (pop, push) = effects::stack_effect(op);
        let available = stack.len();
        let underflowed = available < pop;
        if underflowed {
            record_stack_underflow(instr, pop, available, state.issues);
        }

        // Pop consumers (top-first). Reversed afterwards so `popped` is
        // ordered deep-to-top, matching source-language operand order.
        let mut popped: Vec<SsaVariable> = Vec::with_capacity(pop);
        for _ in 0..pop {
            let v = stack.pop().unwrap_or_else(unknown_var);
            popped.push(v);
        }
        popped.reverse();

        if !underflowed
            && popped
                .iter()
                .any(|value| is_unknown_or_tainted(value, state.tainted_variables))
        {
            record_incomplete_issue(
                instr,
                LoweringIssueKind::LostStackValue,
                "instruction consumes an unknown stack value",
                state.issues,
            );
        }

        // Record uses for the consumed values at the current statement index.
        let use_index = stmts.len();
        for v in &popped {
            if !is_unknown(v) {
                uses.push((v.clone(), use_index));
            }
        }

        match op {
            OpCode::Assert => {
                let condition = popped.first().cloned().unwrap_or_else(unknown_var);
                stmts.push(SsaStmt::assert(SsaExpr::var(condition), None));
                return None;
            }
            OpCode::Assertmsg => {
                let condition = popped.first().cloned().unwrap_or_else(unknown_var);
                let message = popped.get(1).cloned().unwrap_or_else(unknown_var);
                stmts.push(SsaStmt::assert(
                    SsaExpr::var(condition),
                    Some(SsaExpr::var(message)),
                ));
                return None;
            }
            OpCode::Throw => {
                let value = popped.first().cloned().unwrap_or_else(unknown_var);
                stmts.push(SsaStmt::throw(Some(SsaExpr::var(value))));
                return None;
            }
            OpCode::Abort => {
                stmts.push(SsaStmt::abort(None));
                return None;
            }
            OpCode::Abortmsg => {
                let message = popped.first().cloned().unwrap_or_else(unknown_var);
                stmts.push(SsaStmt::abort(Some(SsaExpr::var(message))));
                return None;
            }
            _ => {}
        }

        if is_boolean_branch(op) {
            return popped.first().cloned();
        }

        if let Some(branch_op) = comparison_branch_op(op) {
            let left = popped.first().cloned().unwrap_or_else(unknown_var);
            let right = popped.get(1).cloned().unwrap_or_else(unknown_var);
            let target = fresh_var(state.versions, "t");
            stmts.push(SsaStmt::assign(
                target.clone(),
                SsaExpr::binary(branch_op, SsaExpr::var(left), SsaExpr::var(right)),
            ));
            return Some(target);
        }

        if is_collection_mutation(op) {
            if let Some(receiver) = popped.first() {
                if is_shape_preserving_collection_mutation(op) {
                    if op == OpCode::Setitem {
                        update_indexed_shape_for_setitem(&popped, state);
                    } else {
                        clear_indexed_collection_shapes(receiver, state);
                    }
                    invalidate_collection_contents(
                        receiver,
                        state.definition_facts,
                        state.invalidated_collection_content_roots,
                    );
                    record_static_alias_mutation(receiver, true, false, state);
                } else {
                    invalidate_collection_aliases(
                        receiver,
                        state.definition_facts,
                        state.invalidated_collection_content_roots,
                        state.invalidated_collection_roots,
                    );
                    record_static_alias_mutation(receiver, false, false, state);
                }
            }
        }

        if is_effectful_collection(op) {
            stmts.push(SsaStmt::expr(SsaExpr::call(
                SemanticCallTarget::Intrinsic(Intrinsic::Opcode(op)),
                popped.into_iter().map(SsaExpr::var).collect(),
            )));
            return None;
        }

        if push == 1 {
            // A load whose slot has a reaching version reads that version instead
            // of an opaque ldloc0(); otherwise fall through to the call
            // placeholder (build_expr) so uninitialised reads stay opaque.
            let reaching =
                slot_name_for(op, &instr.operand).and_then(|name| slots.get(&name).cloned());
            if reaching.is_none() && requires_reaching_slot_definition(op) {
                record_incomplete_issue(
                    instr,
                    LoweringIssueKind::LostStackValue,
                    "slot load has no reaching definition",
                    state.issues,
                );
            }
            if reaching
                .as_ref()
                .is_some_and(|value| is_unknown_or_tainted(value, state.tainted_variables))
            {
                record_incomplete_issue(
                    instr,
                    LoweringIssueKind::LostStackValue,
                    "slot load reads an unknown merged value",
                    state.issues,
                );
            }
            let establishes_snapshot = reaching.is_none();
            let expr = match reaching {
                Some(var) => SsaExpr::var(var),
                None => self.build_expr(op, instr, &popped),
            };
            // Slot loads inherit their slot name (loc0/arg1/static2); everything
            // else gets a temp name. The version counter is per-pass-global and
            // deterministic, so names stay stable across fixpoint iterations.
            let base = slot_name_for(op, &instr.operand).unwrap_or_else(|| "t".to_string());
            let target = fresh_var(state.versions, &base);
            stmts.push(SsaStmt::assign(target.clone(), expr));
            if establishes_snapshot && base != "t" {
                slots.insert(base, target.clone());
            }
            stack.push(target);
        } else if push == 0 {
            // A store defines a new version of its target slot: `loc0_N = <v>`.
            // Other push==0 opcodes (assert/throw/jump condition) only consumed;
            // their uses were already recorded above.
            if let Some(name) = slot_name_for(op, &instr.operand) {
                if let Some(value) = popped.first().cloned() {
                    if let Some(index) = static_store_index(instr) {
                        state.invalidated_static_collection_shapes.remove(&index);
                        let shape_facts =
                            collection_shape_facts_for_variable_from_state(&value, state);
                        state.static_collection_writes.push(StaticCollectionWrite {
                            index,
                            facts: (!shape_facts.is_empty()).then_some(shape_facts),
                            is_null: resolves_to_null(
                                &value,
                                state.definition_facts,
                                &mut BTreeSet::new(),
                            ),
                            provisional: false,
                        });
                        mark_static_collection_alias(&value, index, state.definition_facts);
                    }
                    let target = fresh_var(state.versions, &name);
                    stmts.push(SsaStmt::assign(target.clone(), SsaExpr::var(value)));
                    slots.insert(name, target);
                }
            }
        }
        None
    }

    fn apply_opaque_call(
        &self,
        instruction: &Instruction,
        stack: &mut Vec<SsaVariable>,
        stmts: &mut Vec<SsaStmt>,
        uses: &mut Vec<(SsaVariable, usize)>,
        state: &mut BuildPassState<'_>,
    ) {
        record_incomplete_issue(
            instruction,
            LoweringIssueKind::UnresolvedCall,
            "call contract metadata is unavailable",
            state.issues,
        );
        if instruction.opcode == OpCode::CallA && stack.is_empty() {
            record_stack_underflow(instruction, 1, 0, state.issues);
        }
        let pointer =
            (instruction.opcode == OpCode::CallA).then(|| stack.pop().unwrap_or_else(unknown_var));
        if let Some(pointer) = &pointer {
            if is_unknown_or_tainted(pointer, state.tainted_variables) {
                record_incomplete_issue(
                    instruction,
                    LoweringIssueKind::LostStackValue,
                    "call consumes an unknown function pointer",
                    state.issues,
                );
            }
            if !is_unknown(pointer) {
                uses.push((pointer.clone(), stmts.len()));
            }
        }

        // This call site has no resolved contract metadata. Keeping deeper
        // values would let consumed arguments resurface after a dropped result,
        // so invalidate the unknown pre-call stack conservatively.
        invalidate_all_collection_facts(
            state.definition_facts,
            state.invalidated_collection_content_roots,
            state.invalidated_collection_roots,
        );
        stack.clear();

        let value = SsaExpr::call(
            context_free_call_target(instruction),
            pointer.into_iter().map(SsaExpr::var).collect(),
        );
        let target = fresh_var(state.versions, "t");
        stmts.push(SsaStmt::assign(target.clone(), value));
        stack.push(target);
    }

    fn apply_known_call(
        &self,
        instruction: &Instruction,
        contract: &super::context::CallContract,
        stack: &mut Vec<SsaVariable>,
        stmts: &mut Vec<SsaStmt>,
        uses: &mut Vec<(SsaVariable, usize)>,
        state: &mut BuildPassState<'_>,
    ) {
        if matches!(&contract.target, SemanticCallTarget::Unresolved { .. }) {
            record_incomplete_issue(
                instruction,
                LoweringIssueKind::UnresolvedCall,
                "call target identity is unresolved",
                state.issues,
            );
        }
        let pointer_count = usize::from(instruction.opcode == OpCode::CallA);
        let required = contract.argument_count + pointer_count;
        let available = stack.len();
        let underflowed = available < required;
        if underflowed {
            record_stack_underflow(instruction, required, available, state.issues);
        }
        let mut consumed_unknown = false;
        if instruction.opcode == OpCode::CallA {
            let pointer = stack.pop().unwrap_or_else(unknown_var);
            if is_unknown_or_tainted(&pointer, state.tainted_variables) {
                consumed_unknown = true;
            }
            if !is_unknown(&pointer) {
                uses.push((pointer, stmts.len()));
            }
        }

        let mut args = Vec::with_capacity(contract.argument_count);
        let mut argument_collection_facts = Vec::with_capacity(contract.argument_count);
        let mut argument_roots = Vec::with_capacity(contract.argument_count);
        let mut argument_effects = Vec::with_capacity(contract.argument_count);
        let mut shape_preserving_roots = BTreeSet::new();
        for argument_index in 0..contract.argument_count {
            let argument = stack.pop().unwrap_or_else(unknown_var);
            argument_collection_facts.push(collection_shape_facts_for_variable_from_state(
                &argument, state,
            ));
            let argument_root =
                collection_fact_root(&argument, state.definition_facts, &mut BTreeSet::new());
            if is_unknown_or_tainted(&argument, state.tainted_variables) {
                consumed_unknown = true;
            }
            if !is_unknown(&argument) {
                uses.push((argument.clone(), stmts.len()));
            }
            let effect = contract
                .argument_effects
                .get(argument_index)
                .copied()
                .unwrap_or_default();
            match effect {
                CollectionArgumentEffect::ReadOnly => {
                    if let Some(root) = &argument_root {
                        shape_preserving_roots.insert(root.clone());
                    }
                }
                CollectionArgumentEffect::PreservesShape => {
                    if let Some(root) = invalidate_collection_contents(
                        &argument,
                        state.definition_facts,
                        state.invalidated_collection_content_roots,
                    ) {
                        state
                            .indexed_collection_shapes
                            .insert(root.clone(), BTreeMap::new());
                        shape_preserving_roots.insert(root);
                    }
                }
                CollectionArgumentEffect::Unknown => invalidate_collection_aliases(
                    &argument,
                    state.definition_facts,
                    state.invalidated_collection_content_roots,
                    state.invalidated_collection_roots,
                ),
            }
            argument_effects.push(effect);
            argument_roots.push(argument_root);
            args.push(SsaExpr::var(argument));
        }
        state
            .call_argument_facts
            .insert(instruction.offset, argument_collection_facts);
        if matches!(&contract.target, SemanticCallTarget::Internal { .. }) {
            invalidate_all_collection_facts_except(
                state.definition_facts,
                state.invalidated_collection_content_roots,
                state.invalidated_collection_roots,
                &shape_preserving_roots,
            );
        }
        apply_argument_field_writes(contract, &argument_roots, state);
        record_static_call_argument_effects(contract, &argument_roots, &argument_effects, state);
        if !underflowed && consumed_unknown {
            record_incomplete_issue(
                instruction,
                LoweringIssueKind::LostStackValue,
                "call consumes an unknown stack value",
                state.issues,
            );
        }

        let call = SsaExpr::call(contract.target.clone(), args);
        if contract.returns_value && contract.may_return {
            let target = fresh_var(state.versions, "t");
            stmts.push(SsaStmt::assign(target.clone(), call));
            stack.push(target);
        } else {
            stmts.push(SsaStmt::expr(call));
        }
    }

    fn apply_known_tail_call(
        &self,
        instruction: &Instruction,
        contract: &super::context::CallContract,
        stack: &mut Vec<SsaVariable>,
        stmts: &mut Vec<SsaStmt>,
        uses: &mut Vec<(SsaVariable, usize)>,
        state: &mut BuildPassState<'_>,
    ) {
        let available = stack.len();
        let underflowed = available < contract.argument_count;
        if underflowed {
            record_stack_underflow(
                instruction,
                contract.argument_count,
                available,
                state.issues,
            );
        }

        let mut consumed_unknown = false;
        let mut args = Vec::with_capacity(contract.argument_count);
        let mut argument_collection_facts = Vec::with_capacity(contract.argument_count);
        let mut argument_roots = Vec::with_capacity(contract.argument_count);
        let mut argument_effects = Vec::with_capacity(contract.argument_count);
        let mut shape_preserving_roots = BTreeSet::new();
        for argument_index in 0..contract.argument_count {
            let argument = stack.pop().unwrap_or_else(unknown_var);
            argument_collection_facts.push(collection_shape_facts_for_variable_from_state(
                &argument, state,
            ));
            let argument_root =
                collection_fact_root(&argument, state.definition_facts, &mut BTreeSet::new());
            if is_unknown_or_tainted(&argument, state.tainted_variables) {
                consumed_unknown = true;
            }
            if !is_unknown(&argument) {
                uses.push((argument.clone(), stmts.len()));
            }
            let effect = contract
                .argument_effects
                .get(argument_index)
                .copied()
                .unwrap_or_default();
            match effect {
                CollectionArgumentEffect::ReadOnly => {
                    if let Some(root) = &argument_root {
                        shape_preserving_roots.insert(root.clone());
                    }
                }
                CollectionArgumentEffect::PreservesShape => {
                    if let Some(root) = invalidate_collection_contents(
                        &argument,
                        state.definition_facts,
                        state.invalidated_collection_content_roots,
                    ) {
                        state
                            .indexed_collection_shapes
                            .insert(root.clone(), BTreeMap::new());
                        shape_preserving_roots.insert(root);
                    }
                }
                CollectionArgumentEffect::Unknown => invalidate_collection_aliases(
                    &argument,
                    state.definition_facts,
                    state.invalidated_collection_content_roots,
                    state.invalidated_collection_roots,
                ),
            }
            argument_effects.push(effect);
            argument_roots.push(argument_root);
            args.push(SsaExpr::var(argument));
        }
        state
            .call_argument_facts
            .insert(instruction.offset, argument_collection_facts);
        if matches!(&contract.target, SemanticCallTarget::Internal { .. }) {
            invalidate_all_collection_facts_except(
                state.definition_facts,
                state.invalidated_collection_content_roots,
                state.invalidated_collection_roots,
                &shape_preserving_roots,
            );
        }
        apply_argument_field_writes(contract, &argument_roots, state);
        record_static_call_argument_effects(contract, &argument_roots, &argument_effects, state);
        if !underflowed && consumed_unknown {
            record_incomplete_issue(
                instruction,
                LoweringIssueKind::LostStackValue,
                "tail call consumes an unknown stack value",
                state.issues,
            );
        }

        let call = SsaExpr::call(contract.target.clone(), args);
        if !contract.may_return {
            stmts.push(SsaStmt::expr(call));
            return;
        }
        let returns_value = self
            .method_context
            .and_then(|context| context.returns_value)
            .unwrap_or(contract.returns_value);
        if returns_value {
            stmts.push(SsaStmt::ret(Some(call)));
        } else {
            stmts.push(SsaStmt::expr(call));
            stmts.push(SsaStmt::ret(None));
        }
    }

    /// Handle fixed-shape stack reorders (DUP/OVER/TUCK/SWAP/ROT/REVERSE3/4/
    /// DEPTH/DROP/NIP). New copies get a fresh SSA definition so the single-
    /// assignment property is preserved.
    fn apply_reorder(
        &self,
        instruction: &Instruction,
        stack: &mut Vec<SsaVariable>,
        stmts: &mut Vec<SsaStmt>,
        state: &mut BuildPassState<'_>,
    ) {
        let op = instruction.opcode;
        if let Some(required) = fixed_reorder_arity(op) {
            if stack.len() < required {
                record_stack_underflow(instruction, required, stack.len(), state.issues);
            } else if stack
                .iter()
                .rev()
                .take(required)
                .any(|value| state.tainted_variables.contains(value))
            {
                record_incomplete_issue(
                    instruction,
                    LoweringIssueKind::LostStackValue,
                    "stack operation consumes an unknown merged value",
                    state.issues,
                );
            }
        }
        let mut fresh_copy =
            |src: SsaVariable, stack: &mut Vec<SsaVariable>, stmts: &mut Vec<SsaStmt>| {
                let target = fresh_var(state.versions, "t");
                stmts.push(SsaStmt::assign(target.clone(), SsaExpr::var(src)));
                stack.push(target);
            };

        match op {
            OpCode::Dup => {
                if let Some(top) = stack.last().cloned() {
                    fresh_copy(top, stack, stmts);
                }
            }
            OpCode::Over => {
                // [.. a, b] -> push copy of a (second from top)
                if stack.len() >= 2 {
                    let second = stack[stack.len() - 2].clone();
                    fresh_copy(second, stack, stmts);
                }
            }
            OpCode::Tuck => {
                // [.. a, b] -> [.. b_copy, a, b]
                if stack.len() >= 2 {
                    let b = stack.pop().unwrap();
                    let a = stack.pop().unwrap();
                    fresh_copy(b.clone(), stack, stmts);
                    stack.push(a);
                    stack.push(b);
                }
            }
            OpCode::Swap => {
                let n = stack.len();
                if n >= 2 {
                    stack.swap(n - 1, n - 2);
                }
            }
            OpCode::Rot => {
                // [.. a, b, c] -> [.. b, c, a]
                if stack.len() >= 3 {
                    let n = stack.len();
                    let a = stack.remove(n - 3);
                    stack.push(a);
                }
            }
            OpCode::Reverse3 => reverse_top(stack, 3),
            OpCode::Reverse4 => reverse_top(stack, 4),
            OpCode::Depth => {
                let depth = stack.len() as i64;
                let target = fresh_var(state.versions, "t");
                stmts.push(SsaStmt::assign(
                    target.clone(),
                    SsaExpr::lit(Literal::Int(depth)),
                ));
                stack.push(target);
            }
            OpCode::Drop => {
                stack.pop();
            }
            OpCode::Nip => {
                // [.. a, b] -> [.. b]
                let n = stack.len();
                if n >= 2 {
                    stack.remove(n - 2);
                }
            }
            _ => {}
        }
    }

    /// Handle operand-dependent specials: PICK/ROLL/XDROP/REVERSEN (index from
    /// the stack), PACK/PACKMAP/PACKSTRUCT/UNPACK (count from the stack),
    /// CLEAR (empties), and SYSCALL (arity from the syscall table).
    fn apply_special(
        &self,
        instr: &Instruction,
        stack: &mut Vec<SsaVariable>,
        stmts: &mut Vec<SsaStmt>,
        uses: &mut Vec<(SsaVariable, usize)>,
        state: &mut BuildPassState<'_>,
    ) {
        match instr.opcode {
            OpCode::Pick | OpCode::Roll | OpCode::Xdrop | OpCode::Reversen => {
                // The index/count comes from the top of the stack. Neo first
                // coerces it to a signed 32-bit integer, then faults on a
                // negative value or a stack position outside the live depth.
                if stack.is_empty() {
                    record_stack_underflow(instr, 1, 0, state.issues);
                }
                let Some(index_variable) = stack.pop() else {
                    return;
                };
                if !is_unknown(&index_variable) {
                    uses.push((index_variable.clone(), stmts.len()));
                }
                if is_unknown_or_tainted(&index_variable, state.tainted_variables) {
                    record_incomplete_issue(
                        instr,
                        LoweringIssueKind::LostStackValue,
                        "dynamic stack operation consumes an unknown index or count",
                        state.issues,
                    );
                    return;
                }

                let Some(index) = resolve_nonnegative_i32_literal(
                    &index_variable,
                    state.definition_facts,
                    &mut BTreeSet::new(),
                ) else {
                    record_incomplete_issue(
                        instr,
                        LoweringIssueKind::MissingProvenance,
                        "dynamic stack operation requires a nonnegative 32-bit integer literal index or count",
                        state.issues,
                    );
                    return;
                };

                match instr.opcode {
                    OpCode::Pick | OpCode::Roll | OpCode::Xdrop => {
                        let Some(required) = index.checked_add(1) else {
                            record_incomplete_issue(
                                instr,
                                LoweringIssueKind::MissingProvenance,
                                "dynamic stack operation index overflows the host stack range",
                                state.issues,
                            );
                            return;
                        };
                        let Some(position) = stack.len().checked_sub(required) else {
                            record_stack_underflow(instr, required, stack.len(), state.issues);
                            return;
                        };
                        if is_unknown_or_tainted(&stack[position], state.tainted_variables) {
                            record_incomplete_issue(
                                instr,
                                LoweringIssueKind::LostStackValue,
                                "dynamic stack operation selects an unknown stack value",
                                state.issues,
                            );
                        }

                        match instr.opcode {
                            OpCode::Pick => {
                                let source = stack[position].clone();
                                let target = fresh_var(state.versions, "t");
                                stmts.push(SsaStmt::assign(target.clone(), SsaExpr::var(source)));
                                stack.push(target);
                            }
                            OpCode::Roll => {
                                let value = stack.remove(position);
                                stack.push(value);
                            }
                            OpCode::Xdrop => {
                                stack.remove(position);
                            }
                            _ => unreachable!("matched indexed stack operation"),
                        }
                    }
                    OpCode::Reversen => {
                        if index > stack.len() {
                            record_stack_underflow(instr, index, stack.len(), state.issues);
                            return;
                        }
                        if stack
                            .iter()
                            .rev()
                            .take(index)
                            .any(|value| is_unknown_or_tainted(value, state.tainted_variables))
                        {
                            record_incomplete_issue(
                                instr,
                                LoweringIssueKind::LostStackValue,
                                "REVERSEN includes an unknown stack value",
                                state.issues,
                            );
                        }
                        reverse_top(stack, index);
                    }
                    _ => unreachable!("matched dynamic stack operation"),
                }
            }
            OpCode::Clear => {
                stack.clear();
            }
            OpCode::Syscall => {
                self.apply_syscall(instr, stack, stmts, uses, state);
            }
            OpCode::Pack | OpCode::Packmap | OpCode::Packstruct => {
                if stack.is_empty() {
                    record_stack_underflow(instr, 1, 0, state.issues);
                }
                let count = stack.pop().unwrap_or_else(unknown_var);
                let Some(count_value) = resolve_nonnegative_literal(
                    &count,
                    state.definition_facts,
                    &mut BTreeSet::new(),
                ) else {
                    record_incomplete_issue(
                        instr,
                        LoweringIssueKind::MissingProvenance,
                        "collection packing requires a nonnegative literal element count",
                        state.issues,
                    );
                    let target = fresh_var(state.versions, "t");
                    stmts.push(SsaStmt::assign(
                        target.clone(),
                        SsaExpr::call(
                            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(instr.opcode)),
                            vec![SsaExpr::var(count)],
                        ),
                    ));
                    stack.clear();
                    stack.push(target);
                    return;
                };

                let values_per_entry = usize::from(instr.opcode == OpCode::Packmap) + 1;
                let Some(required) = count_value.checked_mul(values_per_entry) else {
                    record_incomplete_issue(
                        instr,
                        LoweringIssueKind::MissingProvenance,
                        "collection element count overflows the host index range",
                        state.issues,
                    );
                    let target = fresh_var(state.versions, "t");
                    stmts.push(SsaStmt::assign(
                        target.clone(),
                        SsaExpr::call(
                            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(instr.opcode)),
                            vec![SsaExpr::var(count)],
                        ),
                    ));
                    stack.clear();
                    stack.push(target);
                    return;
                };
                if stack.len() < required {
                    record_stack_underflow(instr, required, stack.len(), state.issues);
                    record_incomplete_issue(
                        instr,
                        LoweringIssueKind::MissingProvenance,
                        "collection packing has fewer values than its literal count requires",
                        state.issues,
                    );
                    let mut args = Vec::with_capacity(stack.len() + 1);
                    args.push(SsaExpr::var(count));
                    args.extend(stack.iter().cloned().map(SsaExpr::var));
                    let target = fresh_var(state.versions, "t");
                    stmts.push(SsaStmt::assign(
                        target.clone(),
                        SsaExpr::call(
                            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(instr.opcode)),
                            args,
                        ),
                    ));
                    stack.clear();
                    stack.push(target);
                    return;
                }

                let mut values = Vec::with_capacity(required);
                for _ in 0..required {
                    let value = stack.pop().expect("stack depth checked above");
                    if is_unknown_or_tainted(&value, state.tainted_variables) {
                        record_incomplete_issue(
                            instr,
                            LoweringIssueKind::LostStackValue,
                            "collection packing consumes an unknown stack value",
                            state.issues,
                        );
                    }
                    values.push(SsaExpr::var(value));
                }
                // Neo inserts PACK values in top-first pop order. PACKMAP also
                // pops each key before its value, so this order is already the
                // collection's semantic order.

                let expression = match instr.opcode {
                    OpCode::Pack => SsaExpr::Array(values),
                    OpCode::Packstruct => SsaExpr::Struct(values),
                    OpCode::Packmap => SsaExpr::Map(
                        values
                            .chunks_exact(2)
                            .map(|pair| (pair[0].clone(), pair[1].clone()))
                            .collect(),
                    ),
                    _ => unreachable!("matched PACK family above"),
                };
                let target = fresh_var(state.versions, "t");
                stmts.push(SsaStmt::assign(target.clone(), expression));
                stack.push(target);
            }
            OpCode::Unpack => {
                if stack.is_empty() {
                    record_stack_underflow(instr, 1, 0, state.issues);
                }
                let item = stack.pop().unwrap_or_else(unknown_var);
                let elements = match resolve_collection_fact(
                    &item,
                    state.definition_facts,
                    state.invalidated_collection_content_roots,
                    state.invalidated_collection_roots,
                    &mut BTreeSet::new(),
                ) {
                    Some(SsaExpr::Array(elements) | SsaExpr::Struct(elements)) => {
                        Some(elements.clone())
                    }
                    _ => None,
                };
                let shape = resolve_collection_shape(
                    &item,
                    state.definition_facts,
                    state.invalidated_collection_roots,
                    &mut BTreeSet::new(),
                );
                let element_count = elements
                    .as_ref()
                    .map(Vec::len)
                    .or_else(|| shape.map(CollectionShape::len));
                let Some(element_count) = element_count else {
                    record_incomplete_issue(
                        instr,
                        LoweringIssueKind::MissingProvenance,
                        "UNPACK source is not a direct unmodified PACK or PACKSTRUCT definition",
                        state.issues,
                    );
                    let target = fresh_var(state.versions, "t");
                    stmts.push(SsaStmt::assign(
                        target.clone(),
                        SsaExpr::call(
                            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Unpack)),
                            vec![SsaExpr::var(item)],
                        ),
                    ));
                    stack.clear();
                    stack.push(target);
                    return;
                };
                let Ok(count) = i64::try_from(element_count) else {
                    record_incomplete_issue(
                        instr,
                        LoweringIssueKind::MissingProvenance,
                        "UNPACK element count exceeds the IR integer range",
                        state.issues,
                    );
                    let target = fresh_var(state.versions, "t");
                    stmts.push(SsaStmt::assign(
                        target.clone(),
                        SsaExpr::call(
                            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Unpack)),
                            vec![SsaExpr::var(item)],
                        ),
                    ));
                    stack.clear();
                    stack.push(target);
                    return;
                };

                let mut variables = Vec::with_capacity(element_count);
                if let Some(elements) = elements {
                    for element in &elements {
                        let SsaExpr::Variable(variable) = element else {
                            record_incomplete_issue(
                                instr,
                                LoweringIssueKind::MissingProvenance,
                                "UNPACK source elements no longer have direct SSA provenance",
                                state.issues,
                            );
                            let target = fresh_var(state.versions, "t");
                            stmts.push(SsaStmt::assign(
                                target.clone(),
                                SsaExpr::call(
                                    SemanticCallTarget::Intrinsic(Intrinsic::Opcode(
                                        OpCode::Unpack,
                                    )),
                                    vec![SsaExpr::var(item.clone())],
                                ),
                            ));
                            stack.clear();
                            stack.push(target);
                            return;
                        };
                        if is_unknown_or_tainted(variable, state.tainted_variables) {
                            record_incomplete_issue(
                                instr,
                                LoweringIssueKind::LostStackValue,
                                "UNPACK source contains an unknown stack value",
                                state.issues,
                            );
                        }
                        variables.push(variable.clone());
                    }
                } else {
                    for index in 0..element_count {
                        let target = fresh_var(state.versions, "t");
                        stmts.push(SsaStmt::assign(
                            target.clone(),
                            SsaExpr::Index {
                                base: Box::new(SsaExpr::var(item.clone())),
                                index: Box::new(SsaExpr::lit(Literal::Int(
                                    i64::try_from(index)
                                        .expect("collection length already fits in i64"),
                                ))),
                            },
                        ));
                        variables.push(target);
                    }
                }
                for variable in variables.into_iter().rev() {
                    stack.push(variable);
                }
                let count_target = fresh_var(state.versions, "t");
                stmts.push(SsaStmt::assign(
                    count_target.clone(),
                    SsaExpr::lit(Literal::Int(count)),
                ));
                stack.push(count_target);
            }
            _ => {}
        }
    }

    fn apply_syscall(
        &self,
        instruction: &Instruction,
        stack: &mut Vec<SsaVariable>,
        stmts: &mut Vec<SsaStmt>,
        uses: &mut Vec<(SsaVariable, usize)>,
        state: &mut BuildPassState<'_>,
    ) {
        let hash = match &instruction.operand {
            Some(Operand::Syscall(hash)) => Some(*hash),
            _ => None,
        };
        let info = hash.and_then(crate::syscalls::lookup);

        let Some(info) = info else {
            record_incomplete_issue(
                instruction,
                LoweringIssueKind::UnresolvedCall,
                "syscall contract metadata is unavailable",
                state.issues,
            );
            let selector = match hash {
                Some(hash) => format!("0x{hash:08X}"),
                _ => "unknown".to_string(),
            };
            invalidate_all_collection_facts(
                state.definition_facts,
                state.invalidated_collection_content_roots,
                state.invalidated_collection_roots,
            );
            stack.clear();
            let target = hash.map_or_else(
                || SemanticCallTarget::Unresolved {
                    display_name: "syscall".to_string(),
                },
                |hash| SemanticCallTarget::Syscall { hash, name: None },
            );
            let call = SsaExpr::call(target, vec![SsaExpr::lit(Literal::String(selector))]);
            let target = fresh_var(state.versions, "t");
            stmts.push(SsaStmt::assign(target.clone(), call));
            stack.push(target);
            return;
        };

        let required = usize::from(info.param_count);
        let available = stack.len();
        let underflowed = available < required;
        if underflowed {
            record_stack_underflow(instruction, required, available, state.issues);
        }
        let use_index = stmts.len();
        let mut args = Vec::with_capacity(usize::from(info.param_count) + 1);
        args.push(SsaExpr::lit(Literal::String(info.name.to_string())));
        let mut consumed_unknown = false;
        for _ in 0..info.param_count {
            let argument = stack.pop().unwrap_or_else(unknown_var);
            if is_unknown_or_tainted(&argument, state.tainted_variables) {
                consumed_unknown = true;
            }
            if !is_unknown(&argument) {
                uses.push((argument.clone(), use_index));
            }
            invalidate_collection_aliases(
                &argument,
                state.definition_facts,
                state.invalidated_collection_content_roots,
                state.invalidated_collection_roots,
            );
            record_static_alias_mutation(&argument, false, false, state);
            args.push(SsaExpr::var(argument));
        }
        if !underflowed && consumed_unknown {
            record_incomplete_issue(
                instruction,
                LoweringIssueKind::LostStackValue,
                "syscall consumes an unknown stack value",
                state.issues,
            );
        }

        let call = SsaExpr::call(
            SemanticCallTarget::Syscall {
                hash: hash.expect("known syscall metadata requires a hash"),
                name: Some(info.name.to_string()),
            },
            args,
        );
        if info.returns_value {
            let target = fresh_var(state.versions, "t");
            stmts.push(SsaStmt::assign(target.clone(), call));
            stack.push(target);
        } else {
            stmts.push(SsaStmt::expr(call));
        }
    }

    /// Build the [`SsaExpr`] for a compute opcode given its already-popped
    /// operands (`popped`, ordered deep-to-top). Compute opcodes get a real
    /// binary/unary/literal tree; everything else surfaces as a `Call` placeholder
    /// that still preserves data-flow (the popped vars appear as arguments).
    fn build_expr(&self, op: OpCode, instr: &Instruction, popped: &[SsaVariable]) -> SsaExpr {
        // Push immediates → literals.
        if let Some(lit) = literal_for_push(op, instr) {
            return SsaExpr::lit(lit);
        }

        // Binary compute.
        if let Some(bin) = binary_op_for(op) {
            let mut it = popped.iter();
            let left = it.next().cloned().unwrap_or_else(unknown_var);
            let right = it.next().cloned().unwrap_or_else(unknown_var);
            return SsaExpr::binary(bin, SsaExpr::var(left), SsaExpr::var(right));
        }
        if op == OpCode::Convert {
            let value = popped.first().cloned().unwrap_or_else(unknown_var);
            let target = instr
                .operand
                .as_ref()
                .and_then(value_type_from_operand)
                .unwrap_or(ValueType::Unknown);
            return SsaExpr::Convert {
                value: Box::new(SsaExpr::var(value)),
                target,
            };
        }
        if op == OpCode::Istype {
            let value = popped.first().cloned().unwrap_or_else(unknown_var);
            let target = instr
                .operand
                .as_ref()
                .and_then(value_type_from_operand)
                .unwrap_or(ValueType::Unknown);
            return SsaExpr::IsType {
                value: Box::new(SsaExpr::var(value)),
                target,
            };
        }
        if matches!(op, OpCode::Newarray | OpCode::NewarrayT) {
            let length = popped.first().cloned().unwrap_or_else(unknown_var);
            let element_type = (op == OpCode::NewarrayT)
                .then(|| instr.operand.as_ref().and_then(value_type_from_operand))
                .flatten();
            return SsaExpr::NewArray {
                length: Box::new(SsaExpr::var(length)),
                element_type,
            };
        }
        // Ternary compute (Within/Substr/Modmul/Modpow): render as a call.
        if matches!(
            op,
            OpCode::Within | OpCode::Substr | OpCode::Modmul | OpCode::Modpow
        ) {
            return SsaExpr::call(
                SemanticCallTarget::Intrinsic(Intrinsic::Opcode(op)),
                popped.iter().cloned().map(SsaExpr::var).collect(),
            );
        }
        // Unary compute.
        if let Some(un) = unary_op_for(op) {
            let operand = popped.first().cloned().unwrap_or_else(unknown_var);
            return SsaExpr::unary(un, SsaExpr::var(operand));
        }
        // Unary compute with no dedicated UnaryOp → render as a call.
        if matches!(
            op,
            OpCode::Sqrt
                | OpCode::Nz
                | OpCode::Size
                | OpCode::Keys
                | OpCode::Values
                | OpCode::Isnull
        ) {
            return SsaExpr::call(
                SemanticCallTarget::Intrinsic(Intrinsic::Opcode(op)),
                popped.iter().cloned().map(SsaExpr::var).collect(),
            );
        }
        // Collection constructors / byte ops without a dedicated expr.
        SsaExpr::call(
            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(op)),
            popped.iter().cloned().map(SsaExpr::var).collect(),
        )
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
type SlotState = BTreeMap<String, SsaVariable>;

fn is_static_slot_name(name: &str) -> bool {
    name.strip_prefix("static").is_some_and(|index| {
        !index.is_empty() && index.chars().all(|character| character.is_ascii_digit())
    })
}

fn static_load_index(instruction: &Instruction) -> Option<usize> {
    match instruction.opcode {
        OpCode::Ldsfld0 => Some(0),
        OpCode::Ldsfld1 => Some(1),
        OpCode::Ldsfld2 => Some(2),
        OpCode::Ldsfld3 => Some(3),
        OpCode::Ldsfld4 => Some(4),
        OpCode::Ldsfld5 => Some(5),
        OpCode::Ldsfld6 => Some(6),
        OpCode::Ldsfld => match instruction.operand {
            Some(Operand::U8(index)) => Some(usize::from(index)),
            _ => None,
        },
        _ => None,
    }
}

fn static_store_index(instruction: &Instruction) -> Option<usize> {
    match instruction.opcode {
        OpCode::Stsfld0 => Some(0),
        OpCode::Stsfld1 => Some(1),
        OpCode::Stsfld2 => Some(2),
        OpCode::Stsfld3 => Some(3),
        OpCode::Stsfld4 => Some(4),
        OpCode::Stsfld5 => Some(5),
        OpCode::Stsfld6 => Some(6),
        OpCode::Stsfld => match instruction.operand {
            Some(Operand::U8(index)) => Some(usize::from(index)),
            _ => None,
        },
        _ => None,
    }
}

fn absent_slot_value(name: &str) -> SsaVariable {
    if is_static_slot_name(name) {
        SsaVariable::initial(name.to_string())
    } else {
        unknown_var()
    }
}

/// Derive the slot name (e.g. `loc0`, `arg1`, `static2`) for a slot load OR
/// store opcode, so SSA values that originate from / are written to a
/// local/argument/static carry their slot name instead of an opaque temp.
/// Returns `None` for non-slot ops. Used both to name load defs and to identify
/// the target slot of a store.
fn slot_name_for(op: OpCode, operand: &Option<Operand>) -> Option<String> {
    use OpCode::*;
    let (kind, idx): (&str, usize) = match op {
        // Loads.
        Ldloc0 => ("loc", 0),
        Ldloc1 => ("loc", 1),
        Ldloc2 => ("loc", 2),
        Ldloc3 => ("loc", 3),
        Ldloc4 => ("loc", 4),
        Ldloc5 => ("loc", 5),
        Ldloc6 => ("loc", 6),
        Ldarg0 => ("arg", 0),
        Ldarg1 => ("arg", 1),
        Ldarg2 => ("arg", 2),
        Ldarg3 => ("arg", 3),
        Ldarg4 => ("arg", 4),
        Ldarg5 => ("arg", 5),
        Ldarg6 => ("arg", 6),
        Ldsfld0 => ("static", 0),
        Ldsfld1 => ("static", 1),
        Ldsfld2 => ("static", 2),
        Ldsfld3 => ("static", 3),
        Ldsfld4 => ("static", 4),
        Ldsfld5 => ("static", 5),
        Ldsfld6 => ("static", 6),
        // Stores (symmetric to the loads above).
        Stloc0 => ("loc", 0),
        Stloc1 => ("loc", 1),
        Stloc2 => ("loc", 2),
        Stloc3 => ("loc", 3),
        Stloc4 => ("loc", 4),
        Stloc5 => ("loc", 5),
        Stloc6 => ("loc", 6),
        Starg0 => ("arg", 0),
        Starg1 => ("arg", 1),
        Starg2 => ("arg", 2),
        Starg3 => ("arg", 3),
        Starg4 => ("arg", 4),
        Starg5 => ("arg", 5),
        Starg6 => ("arg", 6),
        Stsfld0 => ("static", 0),
        Stsfld1 => ("static", 1),
        Stsfld2 => ("static", 2),
        Stsfld3 => ("static", 3),
        Stsfld4 => ("static", 4),
        Stsfld5 => ("static", 5),
        Stsfld6 => ("static", 6),
        Ldloc | Ldarg | Ldsfld | Stloc | Starg | Stsfld => {
            let kind = match op {
                Ldloc | Stloc => "loc",
                Ldarg | Starg => "arg",
                Ldsfld | Stsfld => "static",
                _ => return None,
            };
            match operand {
                Some(Operand::U8(n)) => (kind, *n as usize),
                _ => return None,
            }
        }
        _ => return None,
    };
    Some(format!("{kind}{idx}"))
}

fn requires_reaching_slot_definition(op: OpCode) -> bool {
    use OpCode::*;

    matches!(
        op,
        Ldloc0
            | Ldloc1
            | Ldloc2
            | Ldloc3
            | Ldloc4
            | Ldloc5
            | Ldloc6
            | Ldloc
            | Ldarg0
            | Ldarg1
            | Ldarg2
            | Ldarg3
            | Ldarg4
            | Ldarg5
            | Ldarg6
            | Ldarg
    )
}

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

/// Lower a push opcode (with its operand) to a literal, if it is one.
fn literal_for_push(op: OpCode, instr: &Instruction) -> Option<Literal> {
    use OpCode::*;
    match op {
        Push0 => Some(Literal::Int(0)),
        Push1 => Some(Literal::Int(1)),
        Push2 => Some(Literal::Int(2)),
        Push3 => Some(Literal::Int(3)),
        Push4 => Some(Literal::Int(4)),
        Push5 => Some(Literal::Int(5)),
        Push6 => Some(Literal::Int(6)),
        Push7 => Some(Literal::Int(7)),
        Push8 => Some(Literal::Int(8)),
        Push9 => Some(Literal::Int(9)),
        Push10 => Some(Literal::Int(10)),
        Push11 => Some(Literal::Int(11)),
        Push12 => Some(Literal::Int(12)),
        Push13 => Some(Literal::Int(13)),
        Push14 => Some(Literal::Int(14)),
        Push15 => Some(Literal::Int(15)),
        Push16 => Some(Literal::Int(16)),
        PushM1 => Some(Literal::Int(-1)),
        PushT => Some(Literal::Bool(true)),
        PushF => Some(Literal::Bool(false)),
        PushNull => Some(Literal::Null),
        PushA => match &instr.operand {
            Some(Operand::I32(delta)) => instr
                .offset
                .checked_add_signed(*delta as isize)
                .and_then(|target| i64::try_from(target).ok())
                .map(Literal::Int),
            _ => None,
        },
        Pushint8 | Pushint16 | Pushint32 | Pushint64 => match &instr.operand {
            Some(Operand::I8(v)) => Some(Literal::Int(i64::from(*v))),
            Some(Operand::I16(v)) => Some(Literal::Int(i64::from(*v))),
            Some(Operand::I32(v)) => Some(Literal::Int(i64::from(*v))),
            Some(Operand::I64(v)) => Some(Literal::Int(*v)),
            _ => None,
        },
        Pushint128 | Pushint256 => match &instr.operand {
            Some(Operand::Bytes(bytes)) => Some(Literal::BigInt(signed_le_bytes_to_decimal(bytes))),
            _ => None,
        },
        Pushdata1 | Pushdata2 | Pushdata4 => match &instr.operand {
            Some(Operand::Bytes(bytes)) => Some(
                printable_utf8(bytes)
                    .map(Literal::String)
                    .unwrap_or_else(|| Literal::Bytes(bytes.clone())),
            ),
            _ => None,
        },
        _ => None,
    }
}

/// Map a binary compute opcode to its IR operator, if applicable.
fn binary_op_for(op: OpCode) -> Option<BinOp> {
    use OpCode::*;
    Some(match op {
        Add => BinOp::Add,
        Sub => BinOp::Sub,
        Mul => BinOp::Mul,
        Div => BinOp::Div,
        Mod => BinOp::Mod,
        Pow => BinOp::Pow,
        Shl => BinOp::Shl,
        Shr => BinOp::Shr,
        And => BinOp::And,
        Or => BinOp::Or,
        Xor => BinOp::Xor,
        Equal | Numequal => BinOp::Eq,
        Notequal | Numnotequal => BinOp::Ne,
        Lt => BinOp::Lt,
        Le => BinOp::Le,
        Gt => BinOp::Gt,
        Ge => BinOp::Ge,
        Booland => BinOp::LogicalAnd,
        Boolor => BinOp::LogicalOr,
        _ => return None,
    })
}

fn is_boolean_branch(op: OpCode) -> bool {
    matches!(
        op,
        OpCode::Jmpif | OpCode::Jmpif_L | OpCode::Jmpifnot | OpCode::Jmpifnot_L
    )
}

fn comparison_branch_op(op: OpCode) -> Option<BinOp> {
    use OpCode::*;
    Some(match op {
        JmpEq | JmpEq_L => BinOp::Eq,
        JmpNe | JmpNe_L => BinOp::Ne,
        JmpGt | JmpGt_L => BinOp::Gt,
        JmpGe | JmpGe_L => BinOp::Ge,
        JmpLt | JmpLt_L => BinOp::Lt,
        JmpLe | JmpLe_L => BinOp::Le,
        _ => return None,
    })
}

fn is_effectful_collection(op: OpCode) -> bool {
    matches!(
        op,
        OpCode::Setitem
            | OpCode::Append
            | OpCode::Remove
            | OpCode::Clearitems
            | OpCode::Reverseitems
            | OpCode::Memcpy
    )
}

fn is_collection_mutation(op: OpCode) -> bool {
    is_effectful_collection(op) || op == OpCode::Popitem
}

fn is_shape_preserving_collection_mutation(op: OpCode) -> bool {
    matches!(op, OpCode::Setitem | OpCode::Reverseitems | OpCode::Memcpy)
}

/// Map a unary compute opcode to its IR operator, if applicable.
fn unary_op_for(op: OpCode) -> Option<UnaryOp> {
    use OpCode::*;
    Some(match op {
        Inc => UnaryOp::Inc,
        Dec => UnaryOp::Dec,
        Negate => UnaryOp::Neg,
        Abs => UnaryOp::Abs,
        Sign => UnaryOp::Sign,
        Not => UnaryOp::LogicalNot,
        Invert => UnaryOp::Not,
        _ => return None,
    })
}

/// A short mnemonic for call-placeholder expressions.
fn mnemonic(op: OpCode) -> String {
    format!("{op:?}").to_lowercase()
}

fn call_name(op: OpCode, instruction: &Instruction) -> String {
    match (op, &instruction.operand) {
        (OpCode::Call | OpCode::Call_L, Some(Operand::Jump(delta))) => instruction
            .offset
            .checked_add_signed(*delta as isize)
            .map_or_else(
                || "call".to_string(),
                |target| format!("call_0x{target:04X}"),
            ),
        (OpCode::Call | OpCode::Call_L, Some(Operand::Jump32(delta))) => instruction
            .offset
            .checked_add_signed(*delta as isize)
            .map_or_else(
                || "call".to_string(),
                |target| format!("call_0x{target:04X}"),
            ),
        (OpCode::CallT, Some(Operand::U16(index))) => format!("callt_0x{index:04X}"),
        (OpCode::CallA, _) => "calla".to_string(),
        _ => mnemonic(op),
    }
}

fn context_free_call_target(instruction: &Instruction) -> SemanticCallTarget {
    let internal = |offset: Option<usize>| {
        offset.map_or_else(
            || SemanticCallTarget::Unresolved {
                display_name: call_name(instruction.opcode, instruction),
            },
            |offset| SemanticCallTarget::Internal {
                offset,
                name: format!("call_0x{offset:04X}"),
            },
        )
    };

    match (instruction.opcode, &instruction.operand) {
        (OpCode::Call, Some(Operand::Jump(delta))) => {
            internal(instruction.offset.checked_add_signed(*delta as isize))
        }
        (OpCode::Call_L, Some(Operand::Jump32(delta))) => {
            internal(instruction.offset.checked_add_signed(*delta as isize))
        }
        (OpCode::CallT, Some(Operand::U16(index))) => SemanticCallTarget::MethodToken {
            index: usize::from(*index),
            name: format!("callt_0x{index:04X}"),
            hash_le: None,
            call_flags: None,
        },
        _ => SemanticCallTarget::Unresolved {
            display_name: call_name(instruction.opcode, instruction),
        },
    }
}

/// Collect every [`SsaVariable`] referenced by an [`SsaExpr`].
fn collect_expr_uses(expr: &SsaExpr) -> Vec<SsaVariable> {
    let mut out = Vec::new();
    collect_expr_uses_into(expr, &mut out);
    out
}

fn collect_expr_uses_into(expr: &SsaExpr, out: &mut Vec<SsaVariable>) {
    match expr {
        SsaExpr::Variable(v) => out.push(v.clone()),
        SsaExpr::Binary { left, right, .. } => {
            collect_expr_uses_into(left, out);
            collect_expr_uses_into(right, out);
        }
        SsaExpr::Unary { operand, .. } => collect_expr_uses_into(operand, out),
        SsaExpr::Call { args, .. } => {
            for a in args {
                collect_expr_uses_into(a, out);
            }
        }
        SsaExpr::Index { base, index } => {
            collect_expr_uses_into(base, out);
            collect_expr_uses_into(index, out);
        }
        SsaExpr::Member { base, .. } => collect_expr_uses_into(base, out),
        SsaExpr::Cast { expr, .. } => collect_expr_uses_into(expr, out),
        SsaExpr::Convert { value, .. } | SsaExpr::IsType { value, .. } => {
            collect_expr_uses_into(value, out);
        }
        SsaExpr::NewArray { length, .. } => collect_expr_uses_into(length, out),
        SsaExpr::Array(els) => els.iter().for_each(|e| collect_expr_uses_into(e, out)),
        SsaExpr::Struct(elements) => elements
            .iter()
            .for_each(|element| collect_expr_uses_into(element, out)),
        SsaExpr::Map(pairs) => pairs.iter().for_each(|(k, v)| {
            collect_expr_uses_into(k, out);
            collect_expr_uses_into(v, out);
        }),
        SsaExpr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_expr_uses_into(condition, out);
            collect_expr_uses_into(then_expr, out);
            collect_expr_uses_into(else_expr, out);
        }
        SsaExpr::Literal(_) => {}
    }
}

#[cfg(test)]
mod tests;
