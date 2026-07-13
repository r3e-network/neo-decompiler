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

use super::context::MethodContext;
use super::dominance::{self, DominanceInfo};
use super::effects;
use super::form::{SsaBlock, SsaExpr, SsaForm, SsaStmt, UseSite};
use super::variable::SsaVariable;

/// `(blocks, definitions, uses, covered offsets, issues)` from the final pass.
type SsaBuildResult = (
    BTreeMap<BlockId, SsaBlock>,
    BTreeMap<SsaVariable, BlockId>,
    BTreeMap<SsaVariable, BTreeSet<UseSite>>,
    BTreeSet<usize>,
    Vec<LoweringIssue>,
);

/// SSA plus instruction-level semantic fidelity from the stabilized build.
#[derive(Debug)]
pub(crate) struct SsaBuildOutput {
    pub(crate) ssa: SsaForm,
    pub(crate) fidelity: FidelityReport,
}

struct BuildPassState<'a> {
    issues: &'a mut Vec<LoweringIssue>,
    tainted_variables: &'a BTreeSet<SsaVariable>,
    versions: &'a mut BTreeMap<String, usize>,
    definition_facts: &'a mut DefinitionFacts,
    invalidated_collection_roots: &'a mut BTreeSet<SsaVariable>,
}

struct DefinitionFact {
    expression: SsaExpr,
    // PUSHA also lowers to Literal::Int, but it is not GetInteger-compatible.
    is_integer_literal: bool,
}

type DefinitionFacts = BTreeMap<SsaVariable, DefinitionFact>;

#[derive(Default)]
struct BuildFacts {
    versions: BTreeMap<String, usize>,
    definitions: DefinitionFacts,
    invalidated_collection_roots: BTreeSet<SsaVariable>,
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
        let (blocks, definitions, uses, covered_offsets, issues) = self.build_ssa_blocks();
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
        SsaBuildOutput { ssa, fidelity }
    }

    /// Run the fixpoint that produces per-block φ nodes, exit stacks, and the
    /// assembled [`SsaForm`] pieces.
    fn build_ssa_blocks(&self) -> SsaBuildResult {
        let block_ids: Vec<BlockId> = self.cfg.blocks().map(|b| b.id).collect();

        // Work space: per-block entry/exit symbolic stacks and slot states.
        // Exit-stack / exit-slot *identity* is canonical per def-site, so the
        // loop converges once the join structure stops changing.
        let mut entry_stacks: BTreeMap<BlockId, Vec<SsaVariable>> = BTreeMap::new();
        let mut exit_stacks: BTreeMap<BlockId, Vec<SsaVariable>> = BTreeMap::new();
        let mut entry_slots: BTreeMap<BlockId, SlotState> = BTreeMap::new();
        let mut exit_slots: BTreeMap<BlockId, SlotState> = BTreeMap::new();
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
            for &bid in &block_ids {
                let (new_entry, _new_phis) = self.compute_join_entry(bid, &exit_stacks);
                let (new_slot_entry, _new_slot_phis) =
                    self.compute_join_slots(bid, &exit_slots, &mut facts.versions);
                let exec = self.execute_block(
                    bid,
                    &new_entry,
                    &new_slot_entry,
                    &no_tainted_variables,
                    &mut facts,
                );

                let exit_changed = exit_stacks.get(&bid) != Some(&exec.exit_stack);
                let entry_changed = entry_stacks.get(&bid) != Some(&new_entry);
                let slot_exit_changed = exit_slots.get(&bid) != Some(&exec.exit_slots);
                let slot_entry_changed = entry_slots.get(&bid) != Some(&new_slot_entry);
                if exit_changed || entry_changed || slot_exit_changed || slot_entry_changed {
                    changed = true;
                }
                entry_stacks.insert(bid, new_entry);
                exit_stacks.insert(bid, exec.exit_stack);
                entry_slots.insert(bid, new_slot_entry);
                exit_slots.insert(bid, exec.exit_slots);
                block_uses.insert(bid, exec.uses);
            }
        }

        // Final pass: recompute phis from the stabilised exit stacks and assemble.
        let mut ssa_blocks = BTreeMap::new();
        let mut definitions = BTreeMap::new();
        let mut uses: BTreeMap<SsaVariable, BTreeSet<UseSite>> = BTreeMap::new();
        let mut covered_offsets = BTreeSet::new();
        let mut issues = Vec::new();
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
        for &bid in &block_ids {
            let entry = entry_stacks.get(&bid).cloned().unwrap_or_default();
            let slot_entry = entry_slots.get(&bid).cloned().unwrap_or_default();
            let (_, stack_phis) = self.compute_join_entry(bid, &exit_stacks);
            let (_, slot_phis) = self.compute_join_slots(bid, &exit_slots, &mut facts.versions);
            let exec = self.execute_block(bid, &entry, &slot_entry, &tainted_variables, &mut facts);
            covered_offsets.extend(exec.covered_offsets.iter().copied());
            issues.extend(exec.issues.iter().cloned());

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

        (ssa_blocks, definitions, uses, covered_offsets, issues)
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
        for bid in block_ids {
            phis.extend(self.compute_join_entry(*bid, exit_stacks).1);
            phis.extend(
                self.compute_join_slots(*bid, exit_slots, &mut facts.versions)
                    .1,
            );
            let entry = entry_stacks.get(bid).cloned().unwrap_or_default();
            let slot_entry = entry_slots.get(bid).cloned().unwrap_or_default();
            let _ =
                self.execute_block(*bid, &entry, &slot_entry, &no_tainted_variables, &mut facts);
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
        let argument_count = self
            .method_context
            .map_or(0, |context| context.argument_names.len());
        for index in 0..argument_count {
            versions.insert(format!("arg{index}"), 1);
        }
    }

    /// Symbolically execute one block straight-line from `entry`, producing the
    /// exit stack, the SSA statements, and the use list
    /// (vars consumed by non-assignment opcodes such as stores / conditions).
    fn execute_block(
        &self,
        bid: BlockId,
        entry: &[SsaVariable],
        entry_slots: &SlotState,
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

        {
            let mut state = BuildPassState {
                issues: &mut issues,
                tainted_variables,
                versions: &mut facts.versions,
                definition_facts: &mut facts.definitions,
                invalidated_collection_roots: &mut facts.invalidated_collection_roots,
            };
            let mut idx = block.instruction_range.start;
            while idx < block.instruction_range.end {
                let Some(instr) = self.instructions.get(idx) else {
                    idx += 1;
                    continue;
                };
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
                                state.definition_facts,
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
                record_definition_facts(
                    &stmts[statement_start..],
                    instr.opcode,
                    state.definition_facts,
                );
                idx += 1;
            }
        }

        BlockExec {
            exit_stack: stack,
            exit_slots: slots,
            stmts,
            uses,
            terminator_condition,
            covered_offsets,
            issues,
        }
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
                invalidate_collection_aliases(
                    receiver,
                    state.definition_facts,
                    state.invalidated_collection_roots,
                );
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
        invalidate_all_collection_facts(state.definition_facts, state.invalidated_collection_roots);
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
        for _ in 0..contract.argument_count {
            let argument = stack.pop().unwrap_or_else(unknown_var);
            if is_unknown_or_tainted(&argument, state.tainted_variables) {
                consumed_unknown = true;
            }
            if !is_unknown(&argument) {
                uses.push((argument.clone(), stmts.len()));
            }
            invalidate_collection_aliases(
                &argument,
                state.definition_facts,
                state.invalidated_collection_roots,
            );
            args.push(SsaExpr::var(argument));
        }
        if matches!(&contract.target, SemanticCallTarget::Internal { .. }) {
            invalidate_all_collection_facts(
                state.definition_facts,
                state.invalidated_collection_roots,
            );
        }
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
        for _ in 0..contract.argument_count {
            let argument = stack.pop().unwrap_or_else(unknown_var);
            if is_unknown_or_tainted(&argument, state.tainted_variables) {
                consumed_unknown = true;
            }
            if !is_unknown(&argument) {
                uses.push((argument.clone(), stmts.len()));
            }
            invalidate_collection_aliases(
                &argument,
                state.definition_facts,
                state.invalidated_collection_roots,
            );
            args.push(SsaExpr::var(argument));
        }
        if matches!(&contract.target, SemanticCallTarget::Internal { .. }) {
            invalidate_all_collection_facts(
                state.definition_facts,
                state.invalidated_collection_roots,
            );
        }
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
                    state.invalidated_collection_roots,
                    &mut BTreeSet::new(),
                ) {
                    Some(SsaExpr::Array(elements) | SsaExpr::Struct(elements)) => {
                        Some(elements.clone())
                    }
                    _ => None,
                };
                let Some(elements) = elements else {
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

                let mut variables = Vec::with_capacity(elements.len());
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
                                SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Unpack)),
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
                for variable in variables.into_iter().rev() {
                    stack.push(variable);
                }
                let Ok(count) = i64::try_from(elements.len()) else {
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
                state.invalidated_collection_roots,
            );
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
    stmts: Vec<SsaStmt>,
    uses: Vec<(SsaVariable, usize)>,
    terminator_condition: Option<SsaVariable>,
    covered_offsets: BTreeSet<usize>,
    issues: Vec<LoweringIssue>,
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

fn record_definition_facts(statements: &[SsaStmt], opcode: OpCode, facts: &mut DefinitionFacts) {
    for statement in statements {
        let SsaStmt::Assign { target, value } = statement else {
            continue;
        };
        let is_integer_literal = match value {
            SsaExpr::Literal(Literal::Int(_) | Literal::BigInt(_)) => {
                opcode_produces_integer_literal(opcode)
            }
            SsaExpr::Variable(source) => facts
                .get(source)
                .is_some_and(|fact| fact.is_integer_literal),
            _ => false,
        };
        facts.insert(
            target.clone(),
            DefinitionFact {
                expression: value.clone(),
                is_integer_literal,
            },
        );
    }
}

fn opcode_produces_integer_literal(opcode: OpCode) -> bool {
    matches!(
        opcode,
        OpCode::PushM1
            | OpCode::Push0
            | OpCode::Push1
            | OpCode::Push2
            | OpCode::Push3
            | OpCode::Push4
            | OpCode::Push5
            | OpCode::Push6
            | OpCode::Push7
            | OpCode::Push8
            | OpCode::Push9
            | OpCode::Push10
            | OpCode::Push11
            | OpCode::Push12
            | OpCode::Push13
            | OpCode::Push14
            | OpCode::Push15
            | OpCode::Push16
            | OpCode::Pushint8
            | OpCode::Pushint16
            | OpCode::Pushint32
            | OpCode::Pushint64
            | OpCode::Pushint128
            | OpCode::Pushint256
            | OpCode::Depth
            | OpCode::Unpack
    )
}

fn is_collection_fact(expression: &SsaExpr) -> bool {
    matches!(
        expression,
        SsaExpr::Array(_) | SsaExpr::Struct(_) | SsaExpr::Map(_)
    )
}

fn resolve_collection_fact<'a>(
    variable: &SsaVariable,
    facts: &'a DefinitionFacts,
    invalidated_roots: &BTreeSet<SsaVariable>,
    visited: &mut BTreeSet<SsaVariable>,
) -> Option<&'a SsaExpr> {
    if invalidated_roots.contains(variable) {
        return None;
    }
    if !visited.insert(variable.clone()) {
        return None;
    }
    let expression = &facts.get(variable)?.expression;
    match expression {
        expression if is_collection_fact(expression) => Some(expression),
        SsaExpr::Variable(source) => {
            resolve_collection_fact(source, facts, invalidated_roots, visited)
        }
        _ => None,
    }
}

fn collection_fact_root(
    variable: &SsaVariable,
    facts: &DefinitionFacts,
    visited: &mut BTreeSet<SsaVariable>,
) -> Option<SsaVariable> {
    if !visited.insert(variable.clone()) {
        return None;
    }
    match &facts.get(variable)?.expression {
        expression if is_collection_fact(expression) => Some(variable.clone()),
        SsaExpr::Variable(source) => collection_fact_root(source, facts, visited),
        _ => None,
    }
}

fn invalidate_collection_aliases(
    receiver: &SsaVariable,
    facts: &mut DefinitionFacts,
    invalidated_roots: &mut BTreeSet<SsaVariable>,
) {
    let Some(root) = collection_fact_root(receiver, facts, &mut BTreeSet::new()) else {
        return;
    };
    invalidated_roots.insert(root.clone());
    let aliases = facts
        .keys()
        .filter(|candidate| {
            collection_fact_root(candidate, facts, &mut BTreeSet::new()).as_ref() == Some(&root)
        })
        .cloned()
        .collect::<Vec<_>>();
    for alias in aliases {
        facts.remove(&alias);
    }
}

fn invalidate_all_collection_facts(
    facts: &mut DefinitionFacts,
    invalidated_roots: &mut BTreeSet<SsaVariable>,
) {
    let collection_variables = facts
        .keys()
        .filter(|candidate| collection_fact_root(candidate, facts, &mut BTreeSet::new()).is_some())
        .cloned()
        .collect::<Vec<_>>();
    invalidated_roots.extend(
        collection_variables
            .iter()
            .filter_map(|variable| collection_fact_root(variable, facts, &mut BTreeSet::new())),
    );
    for variable in collection_variables {
        facts.remove(&variable);
    }
}

fn resolve_nonnegative_literal(
    variable: &SsaVariable,
    facts: &DefinitionFacts,
    visited: &mut BTreeSet<SsaVariable>,
) -> Option<usize> {
    if !visited.insert(variable.clone()) {
        return None;
    }
    match &facts.get(variable)?.expression {
        SsaExpr::Literal(Literal::Int(value)) => usize::try_from(*value).ok(),
        SsaExpr::Literal(Literal::BigInt(value)) => value.parse().ok(),
        SsaExpr::Variable(source) => resolve_nonnegative_literal(source, facts, visited),
        _ => None,
    }
}

fn resolve_nonnegative_i32_literal(
    variable: &SsaVariable,
    facts: &DefinitionFacts,
    visited: &mut BTreeSet<SsaVariable>,
) -> Option<usize> {
    if !facts.get(variable)?.is_integer_literal {
        return None;
    }
    let value = resolve_nonnegative_literal(variable, facts, visited)?;
    let max = usize::try_from(i32::MAX).ok()?;
    (value <= max).then_some(value)
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
mod tests {
    use super::*;
    use crate::decompiler::cfg::method_body::{Fidelity, LoweringIssueKind};
    use crate::decompiler::cfg::ssa::{CallContract, MethodContext};
    use crate::decompiler::cfg::{BasicBlock, BlockId, CfgBuilder, EdgeKind, Terminator};
    use crate::decompiler::ir::{Intrinsic, SemanticCallTarget};
    use crate::instruction::{Instruction, OpCode, Operand};

    /// Build instructions + a matching CFG for a straight-line program.
    fn linear(instrs: Vec<Instruction>) -> (Vec<Instruction>, Cfg) {
        let cfg = CfgBuilder::new(&instrs).build();
        (instrs, cfg)
    }

    fn instr(off: usize, op: OpCode) -> Instruction {
        Instruction::new(off, op, None)
    }

    fn uneven_stack_merge(tail: Vec<Instruction>) -> (Vec<Instruction>, Cfg) {
        let mut instructions = vec![
            instr(0, OpCode::Nop),
            instr(1, OpCode::Push1),
            instr(2, OpCode::Nop),
        ];
        instructions.extend(tail);

        let mut cfg = Cfg::new();
        cfg.add_block(BasicBlock::new(
            BlockId(0),
            0,
            1,
            0..1,
            Terminator::Branch {
                then_target: BlockId(1),
                else_target: BlockId(2),
            },
        ));
        cfg.add_block(BasicBlock::new(
            BlockId(1),
            1,
            2,
            1..2,
            Terminator::Jump { target: BlockId(3) },
        ));
        cfg.add_block(BasicBlock::new(
            BlockId(2),
            2,
            3,
            2..3,
            Terminator::Jump { target: BlockId(3) },
        ));
        cfg.add_block(BasicBlock::new(
            BlockId(3),
            3,
            instructions.len(),
            3..instructions.len(),
            Terminator::Return,
        ));
        cfg.add_edge(BlockId(0), BlockId(1), EdgeKind::ConditionalTrue);
        cfg.add_edge(BlockId(0), BlockId(2), EdgeKind::ConditionalFalse);
        cfg.add_edge(BlockId(1), BlockId(3), EdgeKind::Unconditional);
        cfg.add_edge(BlockId(2), BlockId(3), EdgeKind::Unconditional);

        (instructions, cfg)
    }

    fn first_nonliteral_assignment(instructions: &[Instruction]) -> SsaExpr {
        let cfg = CfgBuilder::new(instructions).build();
        SsaBuilder::new(&cfg, instructions)
            .build()
            .blocks_iter()
            .flat_map(|(_, block)| &block.stmts)
            .find_map(|statement| match statement {
                SsaStmt::Assign { value, .. } if !matches!(value, SsaExpr::Literal(_)) => {
                    Some(value.clone())
                }
                _ => None,
            })
            .expect("program should produce a non-literal assignment")
    }

    fn optimized_return_expression(instructions: &[Instruction]) -> SsaExpr {
        let cfg = CfgBuilder::new(instructions).build();
        let mut ssa = SsaBuilder::new(&cfg, instructions).build();
        crate::decompiler::cfg::ssa::optimize_ssa(&mut ssa);
        let returned = ssa
            .blocks_iter()
            .flat_map(|(_, block)| &block.stmts)
            .find_map(|statement| match statement {
                SsaStmt::Return(Some(value)) => Some(value.clone()),
                _ => None,
            })
            .expect("program should return a value");
        returned
    }

    fn optimized_collection_expression(instructions: &[Instruction]) -> SsaExpr {
        let cfg = CfgBuilder::new(instructions).build();
        let mut ssa = SsaBuilder::new(&cfg, instructions).build();
        crate::decompiler::cfg::ssa::optimize_ssa(&mut ssa);
        let collection = ssa
            .blocks_iter()
            .flat_map(|(_, block)| &block.stmts)
            .find_map(|statement| match statement {
                SsaStmt::Assign { value, .. }
                    if matches!(
                        value,
                        SsaExpr::Array(_) | SsaExpr::Struct(_) | SsaExpr::Map(_)
                    ) =>
                {
                    Some(value.clone())
                }
                _ => None,
            })
            .expect("program should construct a collection");
        collection
    }

    fn has_unpack_packstruct_intrinsic(form: &SsaForm) -> bool {
        form.blocks_iter()
            .flat_map(|(_, block)| &block.stmts)
            .any(|statement| {
                matches!(
                    statement,
                    SsaStmt::Assign {
                        value: SsaExpr::Call {
                            target: SemanticCallTarget::Intrinsic(Intrinsic::UnpackPackStruct),
                            ..
                        },
                        ..
                    }
                )
            })
    }

    #[test]
    fn convert_consumes_one_value() {
        let instructions = vec![
            instr(0, OpCode::Push1),
            Instruction::new(1, OpCode::Convert, Some(Operand::U8(0x21))),
            instr(3, OpCode::Ret),
        ];

        let converted = first_nonliteral_assignment(&instructions);

        assert_eq!(collect_expr_uses(&converted).len(), 1, "{converted:?}");
        assert!(
            format!("{converted:?}").contains("Integer"),
            "CONVERT must retain its integer target tag: {converted:?}"
        );
    }

    #[test]
    fn istype_preserves_target_tag() {
        let instructions = vec![
            instr(0, OpCode::Push1),
            Instruction::new(1, OpCode::Istype, Some(Operand::U8(0x21))),
            instr(3, OpCode::Ret),
        ];

        let checked = first_nonliteral_assignment(&instructions);

        assert_eq!(collect_expr_uses(&checked).len(), 1, "{checked:?}");
        assert!(
            format!("{checked:?}").contains("Integer"),
            "ISTYPE must retain its integer target tag: {checked:?}"
        );
    }

    #[test]
    fn convert_and_istype_reject_any_target_tag() {
        for opcode in [OpCode::Convert, OpCode::Istype] {
            let instructions = vec![
                instr(0, OpCode::Push1),
                Instruction::new(1, opcode, Some(Operand::U8(0x00))),
                instr(3, OpCode::Ret),
            ];
            let cfg = CfgBuilder::new(&instructions).build();

            let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

            assert_eq!(built.fidelity.status, Fidelity::Incomplete, "{opcode:?}");
            assert!(
                built.fidelity.issues.iter().any(|issue| {
                    issue.offset == 1
                        && issue.opcode == opcode
                        && issue.kind == LoweringIssueKind::MissingOperandMetadata
                        && issue.detail.contains("Any")
                }),
                "{opcode:?}: {:#?}",
                built.fidelity
            );
        }
    }

    #[test]
    fn newarray_t_accepts_any_target_tag() {
        let instructions = vec![
            instr(0, OpCode::Push1),
            Instruction::new(1, OpCode::NewarrayT, Some(Operand::U8(0x00))),
            instr(3, OpCode::Ret),
        ];
        let cfg = CfgBuilder::new(&instructions).build();

        let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

        assert_eq!(
            built.fidelity.status,
            Fidelity::Exact,
            "{:#?}",
            built.fidelity
        );
    }

    #[test]
    fn newarray_t_preserves_element_type() {
        let instructions = vec![
            instr(0, OpCode::Push2),
            Instruction::new(1, OpCode::NewarrayT, Some(Operand::U8(0x21))),
            instr(3, OpCode::Ret),
        ];

        let array = first_nonliteral_assignment(&instructions);

        assert_eq!(collect_expr_uses(&array).len(), 1, "{array:?}");
        assert!(
            format!("{array:?}").contains("Integer"),
            "NEWARRAY_T must retain its integer element tag: {array:?}"
        );
    }

    #[test]
    fn pack_preserves_elements() {
        let instructions = vec![
            instr(0, OpCode::Push2),
            instr(1, OpCode::Push1),
            instr(2, OpCode::Push2),
            instr(3, OpCode::Pack),
            instr(4, OpCode::Ret),
        ];

        assert_eq!(
            optimized_collection_expression(&instructions),
            SsaExpr::Array(vec![
                SsaExpr::lit(Literal::Int(1)),
                SsaExpr::lit(Literal::Int(2)),
            ])
        );
    }

    #[test]
    fn pack_accepts_nonnegative_wide_literal_count() {
        let mut count = vec![0; 16];
        count[0] = 2;
        let instructions = vec![
            instr(0, OpCode::Push2),
            instr(1, OpCode::Push1),
            Instruction::new(2, OpCode::Pushint128, Some(Operand::Bytes(count))),
            instr(19, OpCode::Pack),
            instr(20, OpCode::Ret),
        ];

        assert_eq!(
            optimized_collection_expression(&instructions),
            SsaExpr::Array(vec![
                SsaExpr::lit(Literal::Int(1)),
                SsaExpr::lit(Literal::Int(2)),
            ])
        );
    }

    #[test]
    fn packstruct_preserves_elements() {
        let instructions = vec![
            instr(0, OpCode::Push2),
            instr(1, OpCode::Push1),
            instr(2, OpCode::Push2),
            instr(3, OpCode::Packstruct),
            instr(4, OpCode::Ret),
        ];

        let packed = optimized_collection_expression(&instructions);

        assert_eq!(
            format!("{packed:?}"),
            "Struct([Literal(Int(1)), Literal(Int(2))])"
        );
    }

    #[test]
    fn packmap_preserves_pairs_in_source_order() {
        let instructions = vec![
            instr(0, OpCode::Push4),
            instr(1, OpCode::Push3),
            instr(2, OpCode::Push2),
            instr(3, OpCode::Push1),
            instr(4, OpCode::Push2),
            instr(5, OpCode::Packmap),
            instr(6, OpCode::Ret),
        ];

        assert_eq!(
            optimized_collection_expression(&instructions),
            SsaExpr::Map(vec![
                (SsaExpr::lit(Literal::Int(1)), SsaExpr::lit(Literal::Int(2)),),
                (SsaExpr::lit(Literal::Int(3)), SsaExpr::lit(Literal::Int(4)),),
            ])
        );
    }

    #[test]
    fn unpack_constant_pack_pushes_literal_count() {
        let instructions = vec![
            instr(0, OpCode::Push2),
            instr(1, OpCode::Push1),
            instr(2, OpCode::Push2),
            instr(3, OpCode::Pack),
            instr(4, OpCode::Unpack),
            instr(5, OpCode::Ret),
        ];

        assert_eq!(
            optimized_return_expression(&instructions),
            SsaExpr::lit(Literal::Int(2))
        );
    }

    #[test]
    fn unpack_constant_pack_replays_vm_element_order() {
        let instructions = vec![
            instr(0, OpCode::Push2),
            instr(1, OpCode::Push1),
            instr(2, OpCode::Push2),
            instr(3, OpCode::Pack),
            instr(4, OpCode::Unpack),
            instr(5, OpCode::Drop),
            instr(6, OpCode::Sub),
            instr(7, OpCode::Ret),
        ];

        assert_eq!(
            optimized_return_expression(&instructions),
            SsaExpr::lit(Literal::Int(1))
        );
    }

    #[test]
    fn adjacent_unpack_packstruct_becomes_exact_clone_intrinsic() {
        let instructions = vec![
            instr(0, OpCode::Push1),
            instr(1, OpCode::Unpack),
            instr(2, OpCode::Packstruct),
            instr(3, OpCode::Ret),
        ];
        let cfg = CfgBuilder::new(&instructions).build();

        let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

        assert_eq!(
            built.fidelity.status,
            Fidelity::Exact,
            "{:#?}",
            built.fidelity
        );
        assert!(has_unpack_packstruct_intrinsic(&built.ssa));
        let call = built
            .ssa
            .blocks_iter()
            .flat_map(|(_, block)| &block.stmts)
            .find_map(|statement| match statement {
                SsaStmt::Assign {
                    value:
                        SsaExpr::Call {
                            target: SemanticCallTarget::Intrinsic(Intrinsic::UnpackPackStruct),
                            args,
                        },
                    ..
                } => Some(args),
                _ => None,
            })
            .expect("adjacent pair must emit the clone intrinsic");
        assert_eq!(
            call.len(),
            1,
            "the clone must consume only its source value"
        );
    }

    #[test]
    fn unpack_packstruct_fusion_preserves_ambient_stack_values() {
        let instructions = vec![
            instr(0, OpCode::Push1),
            instr(1, OpCode::Push2),
            instr(2, OpCode::Unpack),
            instr(3, OpCode::Packstruct),
            instr(4, OpCode::Drop),
            instr(5, OpCode::Ret),
        ];

        assert_eq!(
            optimized_return_expression(&instructions),
            SsaExpr::lit(Literal::Int(1))
        );
    }

    #[test]
    fn non_adjacent_unpack_packstruct_is_not_fused() {
        let instructions = vec![
            instr(0, OpCode::Push1),
            instr(1, OpCode::Unpack),
            instr(2, OpCode::Nop),
            instr(3, OpCode::Packstruct),
            instr(4, OpCode::Ret),
        ];
        let cfg = CfgBuilder::new(&instructions).build();

        let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

        assert!(!has_unpack_packstruct_intrinsic(&built.ssa));
    }

    #[test]
    fn unpack_packstruct_is_not_fused_across_basic_block_boundary() {
        let instructions = vec![
            instr(0, OpCode::Push1),
            instr(1, OpCode::Unpack),
            instr(2, OpCode::Packstruct),
            instr(3, OpCode::Ret),
        ];
        let mut cfg = Cfg::new();
        cfg.add_block(BasicBlock::new(
            BlockId(0),
            0,
            2,
            0..2,
            Terminator::Jump { target: BlockId(1) },
        ));
        cfg.add_block(BasicBlock::new(BlockId(1), 2, 4, 2..4, Terminator::Return));
        cfg.add_edge(BlockId(0), BlockId(1), EdgeKind::Unconditional);

        let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

        assert!(!has_unpack_packstruct_intrinsic(&built.ssa));
    }

    #[test]
    fn unpack_packstruct_fusion_preserves_source_underflow_diagnostic() {
        let instructions = vec![
            instr(0, OpCode::Unpack),
            instr(1, OpCode::Packstruct),
            instr(2, OpCode::Ret),
        ];
        let cfg = CfgBuilder::new(&instructions).build();

        let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

        assert!(built.fidelity.issues.iter().any(|issue| {
            issue.offset == 0
                && issue.opcode == OpCode::Unpack
                && issue.kind == LoweringIssueKind::LostStackValue
                && issue.detail.contains("requires 1 stack values")
        }));
    }

    #[test]
    fn unpack_packstruct_fusion_preserves_unknown_source_diagnostic() {
        let (instructions, cfg) = uneven_stack_merge(vec![
            instr(3, OpCode::Unpack),
            instr(4, OpCode::Packstruct),
            instr(5, OpCode::Ret),
        ]);

        let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

        assert!(built.fidelity.issues.iter().any(|issue| {
            issue.offset == 3
                && issue.opcode == OpCode::Unpack
                && issue.kind == LoweringIssueKind::LostStackValue
                && issue
                    .detail
                    .contains("clone consumes an unknown stack value")
        }));
    }

    #[test]
    fn signed_wide_pushes_decode_to_decimal() {
        for (opcode, bytes) in [
            (OpCode::Pushint128, vec![0xFF; 16]),
            (OpCode::Pushint256, vec![0xFF; 32]),
        ] {
            let instruction = Instruction::new(0, opcode, Some(Operand::Bytes(bytes)));
            assert_eq!(
                literal_for_push(opcode, &instruction),
                Some(Literal::BigInt("-1".to_string())),
                "{opcode:?}"
            );
        }
    }

    #[test]
    fn printable_pushdata_becomes_string_literal() {
        let instruction = Instruction::new(
            0,
            OpCode::Pushdata1,
            Some(Operand::Bytes(b"hello".to_vec())),
        );

        assert_eq!(
            literal_for_push(OpCode::Pushdata1, &instruction),
            Some(Literal::String("hello".to_string()))
        );
    }

    #[test]
    fn nonprintable_pushdata_remains_bytes() {
        let bytes = vec![0x00, 0xFF];
        let instruction =
            Instruction::new(0, OpCode::Pushdata1, Some(Operand::Bytes(bytes.clone())));

        assert_eq!(
            literal_for_push(OpCode::Pushdata1, &instruction),
            Some(Literal::Bytes(bytes))
        );
    }

    #[test]
    fn user_append_call_remains_internal_while_vm_append_is_intrinsic() {
        let internal_instructions = vec![
            instr(0, OpCode::Push2),
            instr(1, OpCode::Push1),
            Instruction::new(2, OpCode::Call, Some(Operand::Jump(6))),
            instr(4, OpCode::Ret),
        ];
        let internal_cfg = CfgBuilder::new(&internal_instructions).build();
        let mut context = MethodContext::default();
        context.calls_by_offset.insert(
            2,
            CallContract::new(
                SemanticCallTarget::Internal {
                    offset: 8,
                    name: "append".to_string(),
                },
                2,
                false,
            ),
        );
        let internal_ssa = SsaBuilder::new(&internal_cfg, &internal_instructions)
            .with_method_context(&context)
            .build();
        let internal_target = internal_ssa
            .blocks_iter()
            .flat_map(|(_, block)| &block.stmts)
            .find_map(|stmt| match stmt {
                SsaStmt::Expr(SsaExpr::Call { target, .. }) => Some(target),
                _ => None,
            })
            .expect("resolved internal append call");

        let intrinsic_instructions = vec![
            instr(0, OpCode::Push2),
            instr(1, OpCode::Push1),
            instr(2, OpCode::Append),
            instr(3, OpCode::Ret),
        ];
        let intrinsic_cfg = CfgBuilder::new(&intrinsic_instructions).build();
        let intrinsic_ssa = SsaBuilder::new(&intrinsic_cfg, &intrinsic_instructions).build();
        let intrinsic_target = intrinsic_ssa
            .blocks_iter()
            .flat_map(|(_, block)| &block.stmts)
            .find_map(|stmt| match stmt {
                SsaStmt::Expr(SsaExpr::Call { target, .. }) => Some(target),
                _ => None,
            })
            .expect("VM APPEND intrinsic call");

        assert!(matches!(
            internal_target,
            SemanticCallTarget::Internal { offset: 8, name } if name == "append"
        ));
        assert!(matches!(
            intrinsic_target,
            SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Append))
        ));
        assert_eq!(internal_target.display_name(), "append");
        assert_eq!(intrinsic_target.display_name(), "append");
        assert_ne!(internal_target, intrinsic_target);
    }

    #[test]
    fn context_free_calls_preserve_encoded_identity() {
        let cases = [
            (
                Instruction::new(0, OpCode::Call, Some(Operand::Jump(8))),
                SemanticCallTarget::Internal {
                    offset: 8,
                    name: "call_0x0008".to_string(),
                },
            ),
            (
                Instruction::new(0, OpCode::Call_L, Some(Operand::Jump32(12))),
                SemanticCallTarget::Internal {
                    offset: 12,
                    name: "call_0x000C".to_string(),
                },
            ),
            (
                Instruction::new(0, OpCode::CallT, Some(Operand::U16(7))),
                SemanticCallTarget::MethodToken {
                    index: 7,
                    name: "callt_0x0007".to_string(),
                    hash_le: None,
                    call_flags: None,
                },
            ),
        ];

        for (call, expected) in cases {
            let instructions = vec![call, instr(2, OpCode::Ret)];
            let cfg = CfgBuilder::new(&instructions).build();
            let ssa = SsaBuilder::new(&cfg, &instructions).build();
            let target = ssa
                .blocks_iter()
                .flat_map(|(_, block)| &block.stmts)
                .find_map(|stmt| match stmt {
                    SsaStmt::Assign {
                        value: SsaExpr::Call { target, .. },
                        ..
                    } => Some(target),
                    _ => None,
                })
                .expect("context-free call target");

            assert_eq!(target, &expected);
        }
    }

    #[test]
    fn reported_build_marks_clean_method_exact() {
        let instructions = vec![instr(0, OpCode::Push1), instr(1, OpCode::Ret)];
        let cfg = CfgBuilder::new(&instructions).build();

        let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

        assert_eq!(built.fidelity.status, Fidelity::Exact);
        assert!(built.fidelity.issues.is_empty());
        assert_eq!(built.fidelity.covered_offsets, BTreeSet::from([0, 1]));
        assert_eq!(built.fidelity.instruction_count, 2);
    }

    #[test]
    fn reported_build_marks_literal_pack_exact() {
        let instructions = vec![
            instr(0, OpCode::PushF),
            instr(1, OpCode::Assert),
            instr(2, OpCode::Push1),
            instr(3, OpCode::Push1),
            instr(4, OpCode::Push2),
            instr(5, OpCode::Pack),
            instr(6, OpCode::Drop),
            instr(7, OpCode::Push1),
            instr(8, OpCode::Ret),
        ];
        let cfg = CfgBuilder::new(&instructions).build();

        let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

        assert_eq!(built.fidelity.status, Fidelity::Exact);
        assert_eq!(built.fidelity.instruction_count, instructions.len());
        assert_eq!(
            built.fidelity.covered_offsets,
            BTreeSet::from([0, 1, 2, 3, 4, 5, 6, 7, 8])
        );
        assert!(built.fidelity.issues.is_empty());
    }

    #[test]
    fn literal_dynamic_stack_operations_apply_exact_vm_order() {
        let cases = [
            (
                OpCode::Pick,
                vec![
                    instr(0, OpCode::Push1),
                    instr(1, OpCode::Push2),
                    instr(2, OpCode::Push3),
                    instr(3, OpCode::Push1),
                    instr(4, OpCode::Pick),
                    instr(5, OpCode::Ret),
                ],
                2,
            ),
            (
                OpCode::Roll,
                vec![
                    instr(0, OpCode::Push1),
                    instr(1, OpCode::Push2),
                    instr(2, OpCode::Push3),
                    instr(3, OpCode::Push2),
                    instr(4, OpCode::Roll),
                    instr(5, OpCode::Ret),
                ],
                1,
            ),
            (
                OpCode::Xdrop,
                vec![
                    instr(0, OpCode::Push1),
                    instr(1, OpCode::Push2),
                    instr(2, OpCode::Push3),
                    instr(3, OpCode::Push1),
                    instr(4, OpCode::Xdrop),
                    instr(5, OpCode::Drop),
                    instr(6, OpCode::Ret),
                ],
                1,
            ),
            (
                OpCode::Reversen,
                vec![
                    instr(0, OpCode::Push1),
                    instr(1, OpCode::Push2),
                    instr(2, OpCode::Push3),
                    instr(3, OpCode::Push3),
                    instr(4, OpCode::Reversen),
                    instr(5, OpCode::Ret),
                ],
                1,
            ),
        ];

        for (opcode, instructions, expected) in cases {
            let cfg = CfgBuilder::new(&instructions).build();
            let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

            assert_eq!(
                built.fidelity.status,
                Fidelity::Exact,
                "{opcode:?}: {:#?}",
                built.fidelity.issues
            );
            assert_eq!(
                optimized_return_expression(&instructions),
                SsaExpr::lit(Literal::Int(expected)),
                "{opcode:?} must preserve Neo's deep-to-top stack order"
            );
        }
    }

    #[test]
    fn literal_pick_creates_a_fresh_ssa_copy() {
        let instructions = vec![
            instr(0, OpCode::Push1),
            instr(1, OpCode::Push2),
            instr(2, OpCode::Push1),
            instr(3, OpCode::Pick),
            instr(4, OpCode::Ret),
        ];
        let cfg = CfgBuilder::new(&instructions).build();
        let ssa = SsaBuilder::new(&cfg, &instructions).build();
        let block = ssa.blocks_iter().next().expect("entry block").1;
        let (copy_target, copy_source) = block
            .stmts
            .iter()
            .find_map(|statement| match statement {
                SsaStmt::Assign {
                    target,
                    value: SsaExpr::Variable(source),
                } => Some((target, source)),
                _ => None,
            })
            .expect("PICK must emit an SSA copy assignment");

        assert_ne!(copy_target, copy_source);
        assert!(matches!(
            block.stmts.last(),
            Some(SsaStmt::Return(Some(SsaExpr::Variable(returned))))
                if returned == copy_target
        ));
    }

    #[test]
    fn literal_dynamic_stack_operand_resolves_through_an_ssa_copy() {
        let instructions = vec![
            instr(0, OpCode::Push1),
            instr(1, OpCode::Push2),
            instr(2, OpCode::Push1),
            instr(3, OpCode::Dup),
            instr(4, OpCode::Pick),
            instr(5, OpCode::Ret),
        ];
        let cfg = CfgBuilder::new(&instructions).build();

        let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

        assert_eq!(
            built.fidelity.status,
            Fidelity::Exact,
            "{:#?}",
            built.fidelity.issues
        );
        assert_eq!(
            optimized_return_expression(&instructions),
            SsaExpr::lit(Literal::Int(2))
        );
    }

    #[test]
    fn literal_dynamic_stack_operations_accept_zero_and_depth_boundary() {
        let cases = [
            (
                OpCode::Pick,
                0,
                vec![
                    instr(0, OpCode::Push1),
                    instr(1, OpCode::Push2),
                    instr(2, OpCode::Push0),
                    instr(3, OpCode::Pick),
                    instr(4, OpCode::Ret),
                ],
                2,
            ),
            (
                OpCode::Roll,
                0,
                vec![
                    instr(0, OpCode::Push1),
                    instr(1, OpCode::Push2),
                    instr(2, OpCode::Push0),
                    instr(3, OpCode::Roll),
                    instr(4, OpCode::Ret),
                ],
                2,
            ),
            (
                OpCode::Xdrop,
                0,
                vec![
                    instr(0, OpCode::Push1),
                    instr(1, OpCode::Push2),
                    instr(2, OpCode::Push0),
                    instr(3, OpCode::Xdrop),
                    instr(4, OpCode::Ret),
                ],
                1,
            ),
            (
                OpCode::Reversen,
                0,
                vec![
                    instr(0, OpCode::Push1),
                    instr(1, OpCode::Push2),
                    instr(2, OpCode::Push0),
                    instr(3, OpCode::Reversen),
                    instr(4, OpCode::Ret),
                ],
                2,
            ),
            (
                OpCode::Pick,
                1,
                vec![
                    instr(0, OpCode::Push1),
                    instr(1, OpCode::Push2),
                    instr(2, OpCode::Push1),
                    instr(3, OpCode::Pick),
                    instr(4, OpCode::Ret),
                ],
                1,
            ),
            (
                OpCode::Roll,
                1,
                vec![
                    instr(0, OpCode::Push1),
                    instr(1, OpCode::Push2),
                    instr(2, OpCode::Push1),
                    instr(3, OpCode::Roll),
                    instr(4, OpCode::Ret),
                ],
                1,
            ),
            (
                OpCode::Xdrop,
                1,
                vec![
                    instr(0, OpCode::Push1),
                    instr(1, OpCode::Push2),
                    instr(2, OpCode::Push1),
                    instr(3, OpCode::Xdrop),
                    instr(4, OpCode::Ret),
                ],
                2,
            ),
            (
                OpCode::Reversen,
                2,
                vec![
                    instr(0, OpCode::Push1),
                    instr(1, OpCode::Push2),
                    instr(2, OpCode::Push2),
                    instr(3, OpCode::Reversen),
                    instr(4, OpCode::Ret),
                ],
                1,
            ),
        ];

        for (opcode, operand, instructions, expected) in cases {
            let cfg = CfgBuilder::new(&instructions).build();
            let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

            assert_eq!(
                built.fidelity.status,
                Fidelity::Exact,
                "{opcode:?}({operand}): {:#?}",
                built.fidelity.issues
            );
            assert_eq!(
                optimized_return_expression(&instructions),
                SsaExpr::lit(Literal::Int(expected)),
                "{opcode:?}({operand})"
            );
        }
    }

    #[test]
    fn literal_dynamic_stack_operations_reject_positions_beyond_depth() {
        for opcode in [OpCode::Pick, OpCode::Roll, OpCode::Xdrop, OpCode::Reversen] {
            let count = if opcode == OpCode::Reversen {
                OpCode::Push2
            } else {
                OpCode::Push1
            };
            let instructions = vec![
                instr(0, OpCode::Push1),
                instr(1, count),
                instr(2, opcode),
                instr(3, OpCode::Ret),
            ];
            let cfg = CfgBuilder::new(&instructions).build();

            let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

            assert_eq!(built.fidelity.status, Fidelity::Incomplete, "{opcode:?}");
            assert!(
                built.fidelity.issues.iter().any(|issue| {
                    issue.offset == 2
                        && issue.opcode == opcode
                        && issue.kind == LoweringIssueKind::LostStackValue
                        && issue.detail.contains("requires 2 stack values")
                }),
                "{opcode:?}: {:#?}",
                built.fidelity.issues
            );
        }
    }

    #[test]
    fn dynamic_stack_literals_must_be_nonnegative_i32_integers() {
        for opcode in [OpCode::Pick, OpCode::Roll, OpCode::Xdrop, OpCode::Reversen] {
            let invalid_cases = [
                (
                    "negative",
                    vec![
                        instr(0, OpCode::Push1),
                        instr(1, OpCode::PushM1),
                        instr(2, opcode),
                        instr(3, OpCode::Ret),
                    ],
                    2,
                ),
                (
                    "non-integer",
                    vec![
                        instr(0, OpCode::Push1),
                        instr(1, OpCode::PushT),
                        instr(2, opcode),
                        instr(3, OpCode::Ret),
                    ],
                    2,
                ),
                (
                    "pointer",
                    vec![
                        instr(0, OpCode::Push1),
                        Instruction::new(1, OpCode::PushA, Some(Operand::I32(0))),
                        instr(6, opcode),
                        instr(7, OpCode::Ret),
                    ],
                    6,
                ),
                (
                    "larger than i32::MAX",
                    vec![
                        instr(0, OpCode::Push1),
                        Instruction::new(
                            1,
                            OpCode::Pushint64,
                            Some(Operand::I64(i64::from(i32::MAX) + 1)),
                        ),
                        instr(10, opcode),
                        instr(11, OpCode::Ret),
                    ],
                    10,
                ),
            ];

            for (label, instructions, operation_offset) in invalid_cases {
                let cfg = CfgBuilder::new(&instructions).build();
                let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

                assert_eq!(
                    built.fidelity.status,
                    Fidelity::Incomplete,
                    "{opcode:?} {label}"
                );
                assert!(
                    built.fidelity.issues.iter().any(|issue| {
                        issue.offset == operation_offset
                            && issue.opcode == opcode
                            && issue.kind == LoweringIssueKind::MissingProvenance
                            && issue.detail.contains("32-bit integer literal")
                    }),
                    "{opcode:?} {label}: {:#?}",
                    built.fidelity.issues
                );
            }
        }
    }

    #[test]
    fn dynamic_stack_i32_max_resolves_before_stack_bounds_check() {
        for opcode in [OpCode::Pick, OpCode::Roll, OpCode::Xdrop, OpCode::Reversen] {
            let instructions = vec![
                instr(0, OpCode::Push1),
                Instruction::new(
                    1,
                    OpCode::Pushint64,
                    Some(Operand::I64(i64::from(i32::MAX))),
                ),
                instr(10, opcode),
                instr(11, OpCode::Ret),
            ];
            let cfg = CfgBuilder::new(&instructions).build();

            let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

            assert!(
                built.fidelity.issues.iter().any(|issue| {
                    issue.offset == 10
                        && issue.opcode == opcode
                        && issue.kind == LoweringIssueKind::LostStackValue
                }),
                "{opcode:?}: {:#?}",
                built.fidelity.issues
            );
            assert!(
                !built.fidelity.issues.iter().any(|issue| {
                    issue.offset == 10
                        && issue.opcode == opcode
                        && issue.kind == LoweringIssueKind::MissingProvenance
                }),
                "{opcode:?}: {:#?}",
                built.fidelity.issues
            );
        }
    }

    #[test]
    fn runtime_variable_dynamic_stack_operands_remain_incomplete() {
        for opcode in [OpCode::Pick, OpCode::Roll, OpCode::Xdrop, OpCode::Reversen] {
            let instructions = vec![
                instr(0, OpCode::Push1),
                instr(1, OpCode::Ldarg0),
                instr(2, opcode),
                instr(3, OpCode::Ret),
            ];
            let cfg = CfgBuilder::new(&instructions).build();
            let context = MethodContext {
                argument_names: vec!["index".to_string()],
                ..MethodContext::default()
            };

            let built = SsaBuilder::new(&cfg, &instructions)
                .with_method_context(&context)
                .build_with_report();

            assert_eq!(built.fidelity.status, Fidelity::Incomplete, "{opcode:?}");
            assert!(
                built.fidelity.issues.iter().any(|issue| {
                    issue.offset == 2
                        && issue.opcode == opcode
                        && issue.kind == LoweringIssueKind::MissingProvenance
                        && issue.detail.contains("32-bit integer literal")
                }),
                "{opcode:?}: {:#?}",
                built.fidelity.issues
            );
        }
    }

    #[test]
    fn literal_dynamic_stack_operations_report_unknown_selected_values() {
        for opcode in [OpCode::Pick, OpCode::Roll, OpCode::Xdrop, OpCode::Reversen] {
            let operand = if opcode == OpCode::Reversen {
                OpCode::Push1
            } else {
                OpCode::Push0
            };
            let (instructions, cfg) = uneven_stack_merge(vec![
                instr(3, operand),
                instr(4, opcode),
                instr(5, OpCode::Clear),
                instr(6, OpCode::Ret),
            ]);
            let context = MethodContext {
                returns_value: Some(false),
                ..MethodContext::default()
            };

            let built = SsaBuilder::new(&cfg, &instructions)
                .with_method_context(&context)
                .build_with_report();

            assert!(
                built.fidelity.issues.iter().any(|issue| {
                    issue.offset == 4
                        && issue.opcode == opcode
                        && issue.kind == LoweringIssueKind::LostStackValue
                        && issue.detail.contains("unknown stack value")
                }),
                "{opcode:?}: {:#?}",
                built.fidelity.issues
            );
        }
    }

    #[test]
    fn reported_build_keeps_dynamic_pack_incomplete() {
        let instructions = vec![
            instr(0, OpCode::Ldarg0),
            instr(1, OpCode::Pack),
            instr(2, OpCode::Drop),
            instr(3, OpCode::Ret),
        ];
        let cfg = CfgBuilder::new(&instructions).build();
        let context = MethodContext {
            argument_names: vec!["count".to_string()],
            returns_value: Some(false),
            ..MethodContext::default()
        };

        let built = SsaBuilder::new(&cfg, &instructions)
            .with_method_context(&context)
            .build_with_report();

        assert_eq!(built.fidelity.status, Fidelity::Incomplete);
        assert!(built.fidelity.issues.iter().any(|issue| {
            issue.offset == 1
                && issue.opcode == OpCode::Pack
                && issue.kind == LoweringIssueKind::MissingProvenance
                && issue.fidelity == Fidelity::Incomplete
        }));
    }

    #[test]
    fn aliased_unpack_preserves_unmodified_collection_provenance() {
        let instructions = vec![
            instr(0, OpCode::Push1),
            instr(1, OpCode::Push1),
            instr(2, OpCode::Pack),
            instr(3, OpCode::Dup),
            instr(4, OpCode::Unpack),
            instr(5, OpCode::Drop),
            instr(6, OpCode::Ret),
        ];
        let cfg = CfgBuilder::new(&instructions).build();
        let context = MethodContext {
            returns_value: Some(false),
            ..MethodContext::default()
        };

        let built = SsaBuilder::new(&cfg, &instructions)
            .with_method_context(&context)
            .build_with_report();

        assert_eq!(built.fidelity.status, Fidelity::Exact);
    }

    #[test]
    fn slot_round_trip_preserves_unmodified_collection_provenance() {
        let instructions = vec![
            instr(0, OpCode::Push1),
            instr(1, OpCode::Push1),
            instr(2, OpCode::Pack),
            instr(3, OpCode::Stloc0),
            instr(4, OpCode::Ldloc0),
            instr(5, OpCode::Unpack),
            instr(6, OpCode::Drop),
            instr(7, OpCode::Ret),
        ];
        let cfg = CfgBuilder::new(&instructions).build();
        let context = MethodContext {
            returns_value: Some(false),
            ..MethodContext::default()
        };

        let built = SsaBuilder::new(&cfg, &instructions)
            .with_method_context(&context)
            .build_with_report();

        assert_eq!(built.fidelity.status, Fidelity::Exact);
    }

    #[test]
    fn collection_mutation_invalidates_all_alias_provenance() {
        let instructions = vec![
            instr(0, OpCode::Push1),
            instr(1, OpCode::Push1),
            instr(2, OpCode::Pack),
            instr(3, OpCode::Dup),
            instr(4, OpCode::Push2),
            instr(5, OpCode::Append),
            instr(6, OpCode::Unpack),
            instr(7, OpCode::Drop),
            instr(8, OpCode::Ret),
        ];
        let cfg = CfgBuilder::new(&instructions).build();
        let context = MethodContext {
            returns_value: Some(false),
            ..MethodContext::default()
        };

        let built = SsaBuilder::new(&cfg, &instructions)
            .with_method_context(&context)
            .build_with_report();

        assert_eq!(built.fidelity.status, Fidelity::Incomplete);
        assert!(built.fidelity.issues.iter().any(|issue| {
            issue.offset == 6
                && issue.opcode == OpCode::Unpack
                && issue.kind == LoweringIssueKind::MissingProvenance
                && issue.fidelity == Fidelity::Incomplete
        }));
    }

    #[test]
    fn value_returning_collection_mutation_invalidates_all_alias_provenance() {
        let instructions = vec![
            instr(0, OpCode::Push1),
            instr(1, OpCode::Push1),
            instr(2, OpCode::Pack),
            instr(3, OpCode::Dup),
            instr(4, OpCode::Popitem),
            instr(5, OpCode::Drop),
            instr(6, OpCode::Unpack),
            instr(7, OpCode::Drop),
            instr(8, OpCode::Ret),
        ];
        let cfg = CfgBuilder::new(&instructions).build();
        let context = MethodContext {
            returns_value: Some(false),
            ..MethodContext::default()
        };

        let built = SsaBuilder::new(&cfg, &instructions)
            .with_method_context(&context)
            .build_with_report();

        assert!(built.fidelity.issues.iter().any(|issue| {
            issue.offset == 6
                && issue.opcode == OpCode::Unpack
                && issue.kind == LoweringIssueKind::MissingProvenance
                && issue.fidelity == Fidelity::Incomplete
        }));
    }

    #[test]
    fn known_call_invalidates_collection_argument_provenance() {
        let instructions = vec![
            instr(0, OpCode::Push1),
            instr(1, OpCode::Push1),
            instr(2, OpCode::Pack),
            instr(3, OpCode::Dup),
            Instruction::new(4, OpCode::CallT, Some(Operand::U16(0))),
            instr(7, OpCode::Unpack),
            instr(8, OpCode::Drop),
            instr(9, OpCode::Ret),
        ];
        let cfg = CfgBuilder::new(&instructions).build();
        let mut context = MethodContext {
            returns_value: Some(false),
            ..MethodContext::default()
        };
        context.calls_by_offset.insert(
            4,
            CallContract::new(
                SemanticCallTarget::MethodToken {
                    index: 0,
                    name: "mutate".to_string(),
                    hash_le: None,
                    call_flags: None,
                },
                1,
                false,
            ),
        );

        let built = SsaBuilder::new(&cfg, &instructions)
            .with_method_context(&context)
            .build_with_report();

        assert!(built.fidelity.issues.iter().any(|issue| {
            issue.offset == 7
                && issue.opcode == OpCode::Unpack
                && issue.kind == LoweringIssueKind::MissingProvenance
        }));
    }

    #[test]
    fn syscall_invalidates_collection_argument_provenance() {
        let instructions = vec![
            instr(0, OpCode::Push1),
            instr(1, OpCode::Push1),
            instr(2, OpCode::Pack),
            instr(3, OpCode::Dup),
            Instruction::new(4, OpCode::Syscall, Some(Operand::Syscall(0x9647_E7CF))),
            instr(9, OpCode::Unpack),
            instr(10, OpCode::Drop),
            instr(11, OpCode::Ret),
        ];
        let cfg = CfgBuilder::new(&instructions).build();
        let context = MethodContext {
            returns_value: Some(false),
            ..MethodContext::default()
        };

        let built = SsaBuilder::new(&cfg, &instructions)
            .with_method_context(&context)
            .build_with_report();

        assert!(built.fidelity.issues.iter().any(|issue| {
            issue.offset == 9
                && issue.opcode == OpCode::Unpack
                && issue.kind == LoweringIssueKind::MissingProvenance
        }));
    }

    #[test]
    fn opaque_call_invalidates_all_collection_provenance() {
        let instructions = vec![
            instr(0, OpCode::Push1),
            instr(1, OpCode::Push1),
            instr(2, OpCode::Pack),
            instr(3, OpCode::Stloc0),
            Instruction::new(4, OpCode::Call, Some(Operand::Jump(2))),
            instr(6, OpCode::Drop),
            instr(7, OpCode::Ldloc0),
            instr(8, OpCode::Unpack),
            instr(9, OpCode::Drop),
            instr(10, OpCode::Ret),
        ];
        let cfg = CfgBuilder::new(&instructions).build();
        let context = MethodContext {
            returns_value: Some(false),
            ..MethodContext::default()
        };

        let built = SsaBuilder::new(&cfg, &instructions)
            .with_method_context(&context)
            .build_with_report();

        assert!(built.fidelity.issues.iter().any(|issue| {
            issue.offset == 8
                && issue.opcode == OpCode::Unpack
                && issue.kind == LoweringIssueKind::MissingProvenance
        }));
    }

    #[test]
    fn internal_call_invalidates_static_collection_provenance() {
        let instructions = vec![
            instr(0, OpCode::Push1),
            instr(1, OpCode::Push1),
            instr(2, OpCode::Pack),
            instr(3, OpCode::Stsfld0),
            Instruction::new(4, OpCode::Call, Some(Operand::Jump(3))),
            instr(6, OpCode::Ldsfld0),
            instr(7, OpCode::Unpack),
            instr(8, OpCode::Drop),
            instr(9, OpCode::Ret),
        ];
        let cfg = CfgBuilder::new(&instructions).build();
        let mut context = MethodContext {
            returns_value: Some(false),
            ..MethodContext::default()
        };
        context.calls_by_offset.insert(
            4,
            CallContract::new(
                SemanticCallTarget::Internal {
                    offset: 7,
                    name: "mutate_static".to_string(),
                },
                0,
                false,
            ),
        );

        let built = SsaBuilder::new(&cfg, &instructions)
            .with_method_context(&context)
            .build_with_report();

        assert!(built.fidelity.issues.iter().any(|issue| {
            issue.offset == 7
                && issue.opcode == OpCode::Unpack
                && issue.kind == LoweringIssueKind::MissingProvenance
        }));
    }

    #[test]
    fn loop_backedge_mutation_invalidates_header_collection_provenance() {
        let instructions = vec![
            instr(0, OpCode::Push1),
            instr(1, OpCode::Push1),
            instr(2, OpCode::Pack),
            instr(3, OpCode::Stloc0),
            instr(4, OpCode::Ldloc0),
            instr(5, OpCode::Unpack),
            instr(6, OpCode::Drop),
            instr(7, OpCode::Drop),
            instr(8, OpCode::Ldloc0),
            instr(9, OpCode::Push2),
            instr(10, OpCode::Append),
            Instruction::new(11, OpCode::Jmp, Some(Operand::Jump(-7))),
        ];
        let cfg = CfgBuilder::new(&instructions).build();
        let context = MethodContext {
            returns_value: Some(false),
            ..MethodContext::default()
        };

        let built = SsaBuilder::new(&cfg, &instructions)
            .with_method_context(&context)
            .build_with_report();

        assert!(built.fidelity.issues.iter().any(|issue| {
            issue.offset == 5
                && issue.opcode == OpCode::Unpack
                && issue.kind == LoweringIssueKind::MissingProvenance
        }));
    }

    #[test]
    fn reported_build_records_unresolved_call_at_the_call_site() {
        let instructions = vec![
            Instruction::new(0, OpCode::Call, Some(Operand::Jump(2))),
            instr(2, OpCode::Ret),
        ];
        let cfg = CfgBuilder::new(&instructions).build();
        let context = MethodContext::default();

        let built = SsaBuilder::new(&cfg, &instructions)
            .with_method_context(&context)
            .build_with_report();

        assert_eq!(built.fidelity.status, Fidelity::Incomplete);
        assert!(built.fidelity.issues.iter().any(|issue| {
            issue.offset == 0
                && issue.opcode == OpCode::Call
                && issue.kind == LoweringIssueKind::UnresolvedCall
                && issue.fidelity == Fidelity::Incomplete
        }));
    }

    #[test]
    fn reported_build_records_explicitly_unresolved_call_target() {
        let instructions = vec![
            Instruction::new(0, OpCode::CallT, Some(Operand::U16(3))),
            instr(3, OpCode::Ret),
        ];
        let cfg = CfgBuilder::new(&instructions).build();
        let mut context = MethodContext::default();
        context.calls_by_offset.insert(
            0,
            CallContract::new(
                SemanticCallTarget::Unresolved {
                    display_name: "ambiguous_call".to_string(),
                },
                0,
                true,
            ),
        );

        let built = SsaBuilder::new(&cfg, &instructions)
            .with_method_context(&context)
            .build_with_report();

        assert!(built.fidelity.issues.iter().any(|issue| {
            issue.offset == 0
                && issue.opcode == OpCode::CallT
                && issue.kind == LoweringIssueKind::UnresolvedCall
                && issue.fidelity == Fidelity::Incomplete
        }));
    }

    #[test]
    fn reported_build_records_missing_operand_metadata_at_the_instruction() {
        let instructions = vec![instr(4, OpCode::Pushint8), instr(5, OpCode::Ret)];
        let cfg = CfgBuilder::new(&instructions).build();

        let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

        assert!(built.fidelity.issues.iter().any(|issue| {
            issue.offset == 4
                && issue.opcode == OpCode::Pushint8
                && issue.kind == LoweringIssueKind::MissingOperandMetadata
                && issue.fidelity == Fidelity::Incomplete
        }));
    }

    #[test]
    fn reported_build_records_unknown_value_reaching_return() {
        let instructions = vec![
            instr(10, OpCode::Push1),
            instr(11, OpCode::Pack),
            instr(12, OpCode::Ret),
        ];
        let cfg = CfgBuilder::new(&instructions).build();

        let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

        assert!(built.fidelity.issues.iter().any(|issue| {
            issue.offset == 11
                && issue.opcode == OpCode::Pack
                && issue.kind == LoweringIssueKind::MissingProvenance
                && issue.fidelity == Fidelity::Incomplete
        }));
    }

    #[test]
    fn reported_build_records_unknown_phi_reaching_return() {
        let (instructions, cfg) = uneven_stack_merge(vec![instr(3, OpCode::Ret)]);
        let context = MethodContext {
            returns_value: Some(true),
            ..MethodContext::default()
        };

        let built = SsaBuilder::new(&cfg, &instructions)
            .with_method_context(&context)
            .build_with_report();

        assert!(built.fidelity.issues.iter().any(|issue| {
            issue.offset == 3
                && issue.opcode == OpCode::Ret
                && issue.kind == LoweringIssueKind::LostStackValue
                && issue.fidelity == Fidelity::Incomplete
        }));
    }

    #[test]
    fn reported_build_records_unknown_phi_consumed_by_resolved_call() {
        let (instructions, cfg) = uneven_stack_merge(vec![
            Instruction::new(3, OpCode::CallT, Some(Operand::U16(0))),
            instr(6, OpCode::Ret),
        ]);
        let mut context = MethodContext {
            returns_value: Some(false),
            ..MethodContext::default()
        };
        context.calls_by_offset.insert(
            3,
            CallContract::new(
                SemanticCallTarget::MethodToken {
                    index: 0,
                    name: "helper".to_string(),
                    hash_le: None,
                    call_flags: None,
                },
                1,
                false,
            ),
        );

        let built = SsaBuilder::new(&cfg, &instructions)
            .with_method_context(&context)
            .build_with_report();

        assert!(built.fidelity.issues.iter().any(|issue| {
            issue.offset == 3
                && issue.opcode == OpCode::CallT
                && issue.kind == LoweringIssueKind::LostStackValue
                && issue.fidelity == Fidelity::Incomplete
        }));
    }

    #[test]
    fn reported_build_records_unknown_phi_consumed_by_drop() {
        let (instructions, cfg) =
            uneven_stack_merge(vec![instr(3, OpCode::Drop), instr(4, OpCode::Ret)]);
        let context = MethodContext {
            returns_value: Some(false),
            ..MethodContext::default()
        };

        let built = SsaBuilder::new(&cfg, &instructions)
            .with_method_context(&context)
            .build_with_report();

        assert!(built.fidelity.issues.iter().any(|issue| {
            issue.offset == 3
                && issue.opcode == OpCode::Drop
                && issue.kind == LoweringIssueKind::LostStackValue
                && issue.fidelity == Fidelity::Incomplete
        }));
    }

    #[test]
    fn reported_build_records_fixed_reorder_underflow_at_the_instruction() {
        let instructions = vec![instr(20, OpCode::Dup), instr(21, OpCode::Ret)];
        let cfg = CfgBuilder::new(&instructions).build();

        let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

        assert!(built.fidelity.issues.iter().any(|issue| {
            issue.offset == 20
                && issue.opcode == OpCode::Dup
                && issue.kind == LoweringIssueKind::LostStackValue
                && issue.fidelity == Fidelity::Incomplete
        }));
    }

    #[test]
    fn reported_build_marks_known_syscall_conservative() {
        let instructions = vec![
            instr(30, OpCode::Push1),
            Instruction::new(31, OpCode::Syscall, Some(Operand::Syscall(0x8CEC_27F8))),
            instr(36, OpCode::Ret),
        ];
        let cfg = CfgBuilder::new(&instructions).build();

        let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

        assert_eq!(built.fidelity.status, Fidelity::Conservative);
        assert!(built.fidelity.issues.iter().any(|issue| {
            issue.offset == 31
                && issue.opcode == OpCode::Syscall
                && issue.fidelity == Fidelity::Conservative
        }));
    }

    #[test]
    fn reported_build_records_unsupported_control_at_the_instruction() {
        let instructions = vec![instr(40, OpCode::Endfinally), instr(41, OpCode::Ret)];
        let cfg = CfgBuilder::new(&instructions).build();

        let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

        assert!(built.fidelity.issues.iter().any(|issue| {
            issue.offset == 40
                && issue.opcode == OpCode::Endfinally
                && issue.kind == LoweringIssueKind::UnsupportedControl
                && issue.fidelity == Fidelity::Incomplete
        }));
    }

    #[test]
    fn reported_build_records_stack_underflow_at_the_instruction() {
        let instructions = vec![instr(0, OpCode::Add), instr(1, OpCode::Ret)];
        let cfg = CfgBuilder::new(&instructions).build();

        let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

        assert!(built.fidelity.issues.iter().any(|issue| {
            issue.offset == 0
                && issue.opcode == OpCode::Add
                && issue.kind == LoweringIssueKind::LostStackValue
                && issue.fidelity == Fidelity::Incomplete
        }));
    }

    #[test]
    fn reported_build_records_slot_load_without_reaching_definition() {
        let instructions = vec![instr(0, OpCode::Ldloc0), instr(1, OpCode::Ret)];
        let cfg = CfgBuilder::new(&instructions).build();

        let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

        assert!(built.fidelity.issues.iter().any(|issue| {
            issue.offset == 0
                && issue.opcode == OpCode::Ldloc0
                && issue.kind == LoweringIssueKind::LostStackValue
                && issue.fidelity == Fidelity::Incomplete
        }));
    }

    #[test]
    fn linear_compute_produces_real_binary_expr() {
        // PUSH1, PUSH2, ADD, RET
        let ins = vec![
            instr(0, OpCode::Push1),
            instr(1, OpCode::Push2),
            instr(2, OpCode::Add),
            Instruction::new(3, OpCode::Ret, None),
        ];
        let (ins, cfg) = linear(ins);
        let ssa = SsaBuilder::new(&cfg, &ins).build();

        // At least one block exists; find a block with assignments.
        let (_id, block) = ssa
            .blocks_iter()
            .find(|(_, b)| b.stmt_count() >= 3)
            .expect("a block with >= 3 assignments should exist");

        // v0 = 1, v1 = 2, v2 = (v0 + v2)
        let assignment_count = block
            .stmts
            .iter()
            .filter(|stmt| matches!(stmt, SsaStmt::Assign { .. }))
            .count();
        assert_eq!(assignment_count, 3, "expected 3 push/compute assignments");
        let add = &block.stmts[2];
        let SsaStmt::Assign { value, .. } = add else {
            panic!("third stmt should be the ADD assignment: {add:?}");
        };
        let SsaExpr::Binary { op, left, right } = value else {
            panic!("ADD should lower to a binary expr, got {value:?}");
        };
        assert_eq!(*op, BinOp::Add);
        // Operands reference the two push defs (deep on the left, top right).
        assert!(
            matches!(left.as_ref(), SsaExpr::Variable(_)),
            "left operand"
        );
        assert!(
            matches!(right.as_ref(), SsaExpr::Variable(_)),
            "right operand"
        );
        assert!(matches!(block.stmts.last(), Some(SsaStmt::Return(Some(_)))));
    }

    #[test]
    fn dup_creates_a_copy_definition() {
        // PUSH1, DUP, RET
        let ins = vec![
            instr(0, OpCode::Push1),
            instr(1, OpCode::Dup),
            Instruction::new(2, OpCode::Ret, None),
        ];
        let (ins, cfg) = linear(ins);
        let ssa = SsaBuilder::new(&cfg, &ins).build();
        let block = ssa.blocks_iter().next().expect("a block exists").1;

        // Two assignments: v0 = 1, v1 = v0 (the DUP copy).
        let assignment_count = block
            .stmts
            .iter()
            .filter(|stmt| matches!(stmt, SsaStmt::Assign { .. }))
            .count();
        assert_eq!(assignment_count, 2);
        let copy = &block.stmts[1];
        let SsaStmt::Assign { value, .. } = copy else {
            panic!("DUP should produce an assignment: {copy:?}");
        };
        assert!(
            matches!(value, SsaExpr::Variable(_)),
            "DUP copy should reference its source var, got {value:?}"
        );
        assert!(matches!(block.stmts.last(), Some(SsaStmt::Return(Some(_)))));
    }

    #[test]
    fn call_results_replace_pre_call_stack_values_at_ret() {
        let cases = [
            (
                "call_0x0005",
                vec![
                    instr(0, OpCode::Push1),
                    Instruction::new(1, OpCode::Call, Some(Operand::Jump(4))),
                    instr(3, OpCode::Ret),
                ],
            ),
            (
                "callt_0x0002",
                vec![
                    instr(0, OpCode::Push1),
                    Instruction::new(1, OpCode::CallT, Some(Operand::U16(2))),
                    instr(4, OpCode::Ret),
                ],
            ),
            (
                "calla",
                vec![
                    Instruction::new(0, OpCode::PushA, Some(Operand::I32(6))),
                    instr(5, OpCode::CallA),
                    instr(6, OpCode::Ret),
                ],
            ),
        ];

        for (expected_name, instructions) in cases {
            let cfg = CfgBuilder::new(&instructions).build();
            let ssa = SsaBuilder::new(&cfg, &instructions).build();
            let block = ssa.blocks_iter().next().expect("a block exists").1;
            let Some(SsaStmt::Return(Some(SsaExpr::Variable(returned)))) = block.stmts.last()
            else {
                panic!("{expected_name} must produce the value consumed by RET: {block:?}");
            };
            assert!(
                block.stmts.iter().any(|stmt| matches!(
                    stmt,
                    SsaStmt::Assign {
                        target,
                        value: SsaExpr::Call { target: call_target, .. }
                    } if target == returned && call_target.display_name() == expected_name
                )),
                "{expected_name} must define RET's value: {block:?}"
            );
        }
    }

    #[test]
    fn dropping_opaque_call_result_does_not_expose_pre_call_values() {
        let cases = [
            vec![
                instr(0, OpCode::Push1),
                Instruction::new(1, OpCode::Call, Some(Operand::Jump(5))),
                instr(3, OpCode::Drop),
                instr(4, OpCode::Ret),
            ],
            vec![
                instr(0, OpCode::Push1),
                Instruction::new(1, OpCode::CallT, Some(Operand::U16(2))),
                instr(4, OpCode::Drop),
                instr(5, OpCode::Ret),
            ],
            vec![
                instr(0, OpCode::Push1),
                Instruction::new(1, OpCode::PushA, Some(Operand::I32(7))),
                instr(6, OpCode::CallA),
                instr(7, OpCode::Drop),
                instr(8, OpCode::Ret),
            ],
        ];

        for instructions in cases {
            let cfg = CfgBuilder::new(&instructions).build();
            let ssa = SsaBuilder::new(&cfg, &instructions).build();
            let block = ssa.blocks_iter().next().expect("a block exists").1;
            assert!(
                matches!(block.stmts.last(), Some(SsaStmt::Return(None))),
                "opaque call arguments must not survive a dropped result: {block:?}"
            );
        }
    }

    #[test]
    fn known_call_contract_preserves_stack_and_uses_source_argument_order() {
        // Ambient value 9 stays below two call arguments. Neo pushes call
        // arguments right-to-left, so popping top-first must render (1, 2).
        let instructions = vec![
            instr(0, OpCode::Push9),
            instr(1, OpCode::Push2),
            instr(2, OpCode::Push1),
            Instruction::new(3, OpCode::Call, Some(Operand::Jump(4))),
            instr(5, OpCode::Drop),
            instr(6, OpCode::Ret),
        ];
        let cfg = CfgBuilder::new(&instructions).build();
        let mut context = MethodContext::default();
        context.calls_by_offset.insert(
            3,
            CallContract::new(
                SemanticCallTarget::Internal {
                    offset: 7,
                    name: "helper".to_string(),
                },
                2,
                true,
            ),
        );

        let mut ssa = SsaBuilder::new(&cfg, &instructions)
            .with_method_context(&context)
            .build();
        super::super::optimize_ssa(&mut ssa);
        let block = ssa.blocks_iter().next().expect("a block exists").1;

        assert!(
            block.stmts.iter().any(|stmt| matches!(
                stmt,
                SsaStmt::Assign {
                    value: SsaExpr::Call { target, args },
                    ..
                } if target.display_name() == "helper"
                    && args.as_slice() == [
                        SsaExpr::lit(Literal::Int(1)),
                        SsaExpr::lit(Literal::Int(2)),
                    ]
            )),
            "known value call must retain its ordered arguments: {block:?}"
        );
        assert!(
            matches!(
                block.stmts.last(),
                Some(SsaStmt::Return(Some(SsaExpr::Literal(Literal::Int(9)))))
            ),
            "dropping a known call result must reveal the preserved ambient value: {block:?}"
        );
    }

    #[test]
    fn known_tail_jump_returns_resolved_call_with_source_argument_order() {
        let instructions = vec![
            instr(0, OpCode::Push2),
            instr(1, OpCode::Push1),
            Instruction::new(2, OpCode::Jmp, Some(Operand::Jump(18))),
        ];
        let mut cfg = Cfg::new();
        cfg.add_block(BasicBlock::new(BlockId(0), 0, 3, 0..3, Terminator::Return));
        let mut context = MethodContext {
            returns_value: Some(true),
            ..MethodContext::default()
        };
        context.calls_by_offset.insert(
            2,
            CallContract::new(
                SemanticCallTarget::Internal {
                    offset: 20,
                    name: "helper".to_string(),
                },
                2,
                true,
            ),
        );

        let output = SsaBuilder::new(&cfg, &instructions)
            .with_method_context(&context)
            .build_with_report();
        assert_eq!(output.fidelity.status, Fidelity::Exact);
        let mut ssa = output.ssa;
        super::super::optimize_ssa(&mut ssa);
        let block = ssa.blocks_iter().next().expect("a block exists").1;

        assert!(matches!(
            block.stmts.last(),
            Some(SsaStmt::Return(Some(SsaExpr::Call { target, args })))
                if target.display_name() == "helper"
                    && args.as_slice() == [
                        SsaExpr::lit(Literal::Int(1)),
                        SsaExpr::lit(Literal::Int(2)),
                    ]
        ));
    }

    #[test]
    fn known_call_contract_emits_void_call_without_phantom_result() {
        let instructions = vec![
            instr(0, OpCode::Push9),
            instr(1, OpCode::Push1),
            Instruction::new(2, OpCode::CallT, Some(Operand::U16(0))),
            instr(5, OpCode::Ret),
        ];
        let cfg = CfgBuilder::new(&instructions).build();
        let mut context = MethodContext::default();
        context.calls_by_offset.insert(
            2,
            CallContract::new(
                SemanticCallTarget::MethodToken {
                    index: 0,
                    name: "notify".to_string(),
                    hash_le: None,
                    call_flags: None,
                },
                1,
                false,
            ),
        );

        let mut ssa = SsaBuilder::new(&cfg, &instructions)
            .with_method_context(&context)
            .build();
        super::super::optimize_ssa(&mut ssa);
        let block = ssa.blocks_iter().next().expect("a block exists").1;

        assert!(
            block.stmts.iter().any(|stmt| matches!(
                stmt,
                SsaStmt::Expr(SsaExpr::Call { target, args })
                    if target.display_name() == "notify"
                        && args.as_slice() == [SsaExpr::lit(Literal::Int(1))]
            )),
            "known void call must survive as a side-effect statement: {block:?}"
        );
        assert!(
            matches!(
                block.stmts.last(),
                Some(SsaStmt::Return(Some(SsaExpr::Literal(Literal::Int(9)))))
            ),
            "known void call must not replace the ambient return value: {block:?}"
        );
    }

    #[test]
    fn known_calla_contract_consumes_pointer_without_rendering_it_as_an_argument() {
        let instructions = vec![
            instr(0, OpCode::Push9),
            instr(1, OpCode::Push1),
            Instruction::new(2, OpCode::PushA, Some(Operand::I32(8))),
            instr(7, OpCode::CallA),
            instr(8, OpCode::Ret),
        ];
        let cfg = CfgBuilder::new(&instructions).build();
        let mut context = MethodContext::default();
        context.calls_by_offset.insert(
            7,
            CallContract::new(
                SemanticCallTarget::Internal {
                    offset: 10,
                    name: "delegate".to_string(),
                },
                1,
                false,
            ),
        );

        let output = SsaBuilder::new(&cfg, &instructions)
            .with_method_context(&context)
            .build_with_report();
        assert_eq!(output.fidelity.status, Fidelity::Exact);
        let mut ssa = output.ssa;
        super::super::optimize_ssa(&mut ssa);
        let block = ssa.blocks_iter().next().expect("a block exists").1;

        assert!(
            block.stmts.iter().any(|stmt| matches!(
                stmt,
                SsaStmt::Expr(SsaExpr::Call { target, args })
                    if target.display_name() == "delegate"
                        && args.as_slice() == [SsaExpr::lit(Literal::Int(1))]
            )),
            "CALLA pointer must be consumed separately from source arguments: {block:?}"
        );
        assert!(
            matches!(
                block.stmts.last(),
                Some(SsaStmt::Return(Some(SsaExpr::Literal(Literal::Int(9)))))
            ),
            "resolved void CALLA must preserve deeper caller stack state: {block:?}"
        );
    }

    #[test]
    fn collection_mutations_emit_ordered_effect_calls() {
        let cases = [
            (
                OpCode::Setitem,
                "set_item",
                vec![OpCode::Push1, OpCode::Push2, OpCode::Push3],
                vec![1, 2, 3],
            ),
            (
                OpCode::Append,
                "append",
                vec![OpCode::Push1, OpCode::Push2],
                vec![1, 2],
            ),
            (
                OpCode::Remove,
                "remove_item",
                vec![OpCode::Push1, OpCode::Push2],
                vec![1, 2],
            ),
            (
                OpCode::Clearitems,
                "clear_items",
                vec![OpCode::Push1],
                vec![1],
            ),
            (
                OpCode::Reverseitems,
                "reverse_items",
                vec![OpCode::Push1],
                vec![1],
            ),
            (
                OpCode::Memcpy,
                "memcpy",
                vec![
                    OpCode::Push1,
                    OpCode::Push2,
                    OpCode::Push3,
                    OpCode::Push4,
                    OpCode::Push5,
                ],
                vec![1, 2, 3, 4, 5],
            ),
        ];

        for (opcode, expected_name, pushes, expected_values) in cases {
            let mut instructions = vec![instr(0, OpCode::Push9)];
            instructions.extend(
                pushes
                    .into_iter()
                    .enumerate()
                    .map(|(offset, push)| instr(offset + 1, push)),
            );
            instructions.push(instr(instructions.len(), opcode));
            instructions.push(instr(instructions.len(), OpCode::Ret));

            let cfg = CfgBuilder::new(&instructions).build();
            let mut ssa = SsaBuilder::new(&cfg, &instructions).build();
            super::super::optimize_ssa(&mut ssa);
            let block = ssa.blocks_iter().next().expect("a block exists").1;
            let matching_calls: Vec<_> = block
                .stmts
                .iter()
                .filter_map(|stmt| match stmt {
                    SsaStmt::Expr(SsaExpr::Call { target, args })
                        if target.display_name() == expected_name =>
                    {
                        Some(args)
                    }
                    _ => None,
                })
                .collect();
            let expected_args: Vec<_> = expected_values
                .into_iter()
                .map(|value| SsaExpr::lit(Literal::Int(value)))
                .collect();

            assert_eq!(
                matching_calls.len(),
                1,
                "{opcode:?} must emit one {expected_name} effect call: {block:?}"
            );
            assert_eq!(
                matching_calls[0], &expected_args,
                "{opcode:?} must preserve deep-to-top operand order"
            );
            assert!(
                matches!(
                    block.stmts.last(),
                    Some(SsaStmt::Return(Some(SsaExpr::Literal(Literal::Int(9)))))
                ),
                "{opcode:?} must consume exactly its operands and preserve ambient 9: {block:?}"
            );
        }
    }

    #[test]
    fn collection_mutation_underflow_preserves_declared_arity() {
        let cases = [
            (OpCode::Setitem, "set_item", 3, true),
            (OpCode::Append, "append", 2, true),
            (OpCode::Remove, "remove_item", 2, true),
            (OpCode::Clearitems, "clear_items", 1, false),
            (OpCode::Reverseitems, "reverse_items", 1, false),
            (OpCode::Memcpy, "memcpy", 5, true),
        ];

        for (opcode, expected_name, arity, has_available_top) in cases {
            let mut instructions = Vec::new();
            if has_available_top {
                instructions.push(instr(0, OpCode::Push1));
            }
            instructions.push(instr(instructions.len(), opcode));
            instructions.push(instr(instructions.len(), OpCode::Ret));

            let cfg = CfgBuilder::new(&instructions).build();
            let mut ssa = SsaBuilder::new(&cfg, &instructions).build();
            super::super::optimize_ssa(&mut ssa);
            let block = ssa.blocks_iter().next().expect("a block exists").1;
            let args = block.stmts.iter().find_map(|stmt| match stmt {
                SsaStmt::Expr(SsaExpr::Call { target, args })
                    if target.display_name() == expected_name =>
                {
                    Some(args)
                }
                _ => None,
            });
            let args = args
                .unwrap_or_else(|| panic!("{opcode:?} underflow must remain visible: {block:?}"));

            assert_eq!(args.len(), arity, "{opcode:?} must retain declared arity");
            let unknown_count = args
                .iter()
                .filter(|arg| **arg == SsaExpr::var(unknown_var()))
                .count();
            assert_eq!(
                unknown_count,
                arity - usize::from(has_available_top),
                "{opcode:?} must preserve each missing operand position"
            );
            if has_available_top {
                assert_eq!(
                    args.last(),
                    Some(&SsaExpr::lit(Literal::Int(1))),
                    "{opcode:?} must keep the available top value in the final operand position"
                );
            }
        }
    }

    #[test]
    fn structured_known_syscall_value_uses_catalog_contract() {
        let instructions = vec![
            instr(0, OpCode::Push9),
            instr(1, OpCode::Push1),
            Instruction::new(2, OpCode::Syscall, Some(Operand::Syscall(0x8CEC_27F8))),
            instr(7, OpCode::Drop),
            instr(8, OpCode::Ret),
        ];
        let cfg = CfgBuilder::new(&instructions).build();

        let mut ssa = SsaBuilder::new(&cfg, &instructions).build();
        super::super::optimize_ssa(&mut ssa);
        let block = ssa.blocks_iter().next().expect("a block exists").1;

        assert!(block.stmts.iter().any(|stmt| matches!(
            stmt,
            SsaStmt::Assign {
                value: SsaExpr::Call { target, args },
                ..
            } if matches!(target, SemanticCallTarget::Syscall {
                    hash: 0x8CEC_27F8,
                    name: Some(name),
                } if name == "System.Runtime.CheckWitness")
                && args.as_slice() == [
                    SsaExpr::lit(Literal::String(
                        "System.Runtime.CheckWitness".to_string()
                    )),
                    SsaExpr::lit(Literal::Int(1)),
                ]
        )));
        assert!(matches!(
            block.stmts.last(),
            Some(SsaStmt::Return(Some(SsaExpr::Literal(Literal::Int(9)))))
        ));
    }

    #[test]
    fn structured_known_syscall_void_preserves_ambient_stack_value() {
        let instructions = vec![
            instr(0, OpCode::Push9),
            instr(1, OpCode::Push1),
            Instruction::new(2, OpCode::Syscall, Some(Operand::Syscall(0x9647_E7CF))),
            instr(7, OpCode::Ret),
        ];
        let cfg = CfgBuilder::new(&instructions).build();

        let mut ssa = SsaBuilder::new(&cfg, &instructions).build();
        super::super::optimize_ssa(&mut ssa);
        let block = ssa.blocks_iter().next().expect("a block exists").1;

        assert!(block.stmts.iter().any(|stmt| matches!(
            stmt,
            SsaStmt::Expr(SsaExpr::Call { target, args })
                if matches!(target, SemanticCallTarget::Syscall {
                        hash: 0x9647_E7CF,
                        name: Some(name),
                    } if name == "System.Runtime.Log")
                    && args.as_slice() == [
                        SsaExpr::lit(Literal::String("System.Runtime.Log".to_string())),
                        SsaExpr::lit(Literal::Int(1)),
                    ]
        )));
        assert!(matches!(
            block.stmts.last(),
            Some(SsaStmt::Return(Some(SsaExpr::Literal(Literal::Int(9)))))
        ));
    }

    #[test]
    fn structured_known_syscall_preserves_declaration_order() {
        let instructions = vec![
            instr(0, OpCode::Push9),
            instr(1, OpCode::Push1),
            instr(2, OpCode::Push2),
            instr(3, OpCode::Push3),
            Instruction::new(4, OpCode::Syscall, Some(Operand::Syscall(0x8418_3FE6))),
            instr(9, OpCode::Ret),
        ];
        let cfg = CfgBuilder::new(&instructions).build();

        let mut ssa = SsaBuilder::new(&cfg, &instructions).build();
        super::super::optimize_ssa(&mut ssa);
        let block = ssa.blocks_iter().next().expect("a block exists").1;

        assert!(block.stmts.iter().any(|stmt| matches!(
            stmt,
            SsaStmt::Expr(SsaExpr::Call { target, args })
                if matches!(target, SemanticCallTarget::Syscall {
                        hash: 0x8418_3FE6,
                        name: Some(name),
                    } if name == "System.Storage.Put")
                    && args.as_slice() == [
                        SsaExpr::lit(Literal::String("System.Storage.Put".to_string())),
                        SsaExpr::lit(Literal::Int(3)),
                        SsaExpr::lit(Literal::Int(2)),
                        SsaExpr::lit(Literal::Int(1)),
                    ]
        )));
        assert!(matches!(
            block.stmts.last(),
            Some(SsaStmt::Return(Some(SsaExpr::Literal(Literal::Int(9)))))
        ));
    }

    #[test]
    fn structured_syscall_fallback_keeps_missing_known_argument_visible() {
        let instructions = vec![
            Instruction::new(0, OpCode::Syscall, Some(Operand::Syscall(0x9647_E7CF))),
            instr(5, OpCode::Ret),
        ];
        let cfg = CfgBuilder::new(&instructions).build();
        let ssa = SsaBuilder::new(&cfg, &instructions).build();
        let block = ssa.blocks_iter().next().expect("a block exists").1;

        assert!(block.stmts.iter().any(|stmt| matches!(
            stmt,
            SsaStmt::Expr(SsaExpr::Call { target, args })
                if matches!(target, SemanticCallTarget::Syscall {
                        hash: 0x9647_E7CF,
                        name: Some(name),
                    } if name == "System.Runtime.Log")
                    && args.as_slice() == [
                        SsaExpr::lit(Literal::String("System.Runtime.Log".to_string())),
                        SsaExpr::var(unknown_var()),
                    ]
        )));
    }

    #[test]
    fn structured_syscall_fallback_unknown_hash_uses_opaque_barrier() {
        let instructions = vec![
            instr(0, OpCode::Push9),
            Instruction::new(1, OpCode::Syscall, Some(Operand::Syscall(0xDEAD_BEEF))),
            instr(6, OpCode::Drop),
            instr(7, OpCode::Ret),
        ];
        let cfg = CfgBuilder::new(&instructions).build();
        let ssa = SsaBuilder::new(&cfg, &instructions).build();
        let block = ssa.blocks_iter().next().expect("a block exists").1;

        assert!(block.stmts.iter().any(|stmt| matches!(
            stmt,
            SsaStmt::Assign {
                value: SsaExpr::Call { target, args },
                ..
            } if matches!(target, SemanticCallTarget::Syscall {
                    hash: 0xDEAD_BEEF,
                    name: None,
                })
                && args.as_slice() == [SsaExpr::lit(Literal::String(
                    "0xDEADBEEF".to_string()
                ))]
        )));
        assert!(matches!(block.stmts.last(), Some(SsaStmt::Return(None))));
    }

    #[test]
    fn entry_loop_keeps_manifest_arguments_as_incoming_slots() {
        let instructions = vec![instr(0, OpCode::Ldarg0), instr(1, OpCode::Drop)];
        let mut cfg = Cfg::new();
        cfg.add_block(BasicBlock::new(
            BlockId(0),
            0,
            2,
            0..2,
            Terminator::Jump { target: BlockId(0) },
        ));
        cfg.add_edge(BlockId(0), BlockId(0), EdgeKind::Unconditional);
        let context = MethodContext {
            argument_names: vec!["value".to_string()],
            ..MethodContext::default()
        };

        let ssa = SsaBuilder::new(&cfg, &instructions)
            .with_method_context(&context)
            .build();
        let block = ssa.block(BlockId(0)).expect("entry loop block");

        assert!(
            block.stmts.iter().any(|stmt| matches!(
                stmt,
                SsaStmt::Assign {
                    value: SsaExpr::Variable(source),
                    ..
                } if source == &SsaVariable::initial("arg0".to_string())
            )),
            "entry-loop LDARG0 must read the incoming manifest argument: {block:?}"
        );
    }

    #[test]
    fn inferred_entry_stack_arguments_follow_vm_order() {
        let instructions = vec![instr(0, OpCode::Sub), instr(1, OpCode::Ret)];
        let cfg = CfgBuilder::new(&instructions).build();
        let context = MethodContext {
            argument_names: vec!["left".to_string(), "right".to_string()],
            arguments_on_entry_stack: true,
            returns_value: Some(true),
            ..MethodContext::default()
        };

        let ssa = SsaBuilder::new(&cfg, &instructions)
            .with_method_context(&context)
            .build();
        let block = ssa.block(BlockId(0)).expect("entry block");

        assert!(matches!(
            block.stmts.first(),
            Some(SsaStmt::Assign {
                value: SsaExpr::Binary { left, right, .. },
                ..
            }) if matches!(left.as_ref(), SsaExpr::Variable(value) if value.base == "arg1")
                && matches!(right.as_ref(), SsaExpr::Variable(value) if value.base == "arg0")
        ));
    }

    #[test]
    fn entry_loop_keeps_inferred_arguments_as_incoming_stack_values() {
        let instructions = vec![instr(0, OpCode::Drop)];
        let mut cfg = Cfg::new();
        cfg.add_block(BasicBlock::new(
            BlockId(0),
            0,
            1,
            0..1,
            Terminator::Jump { target: BlockId(0) },
        ));
        cfg.add_edge(BlockId(0), BlockId(0), EdgeKind::Unconditional);
        let context = MethodContext {
            argument_names: vec!["value".to_string()],
            arguments_on_entry_stack: true,
            ..MethodContext::default()
        };

        let ssa = SsaBuilder::new(&cfg, &instructions)
            .with_method_context(&context)
            .build();
        let block = ssa.block(BlockId(0)).expect("entry loop block");

        assert!(block.phi_nodes.iter().any(|phi| {
            phi.operands
                .values()
                .any(|value| value == &SsaVariable::initial("arg0".to_string()))
        }));
    }

    #[test]
    fn store_local_emits_a_slot_assignment() {
        // PUSH10 ; STLOC0 ; RET  →  the store must define a loc0 SSA var.
        let ins = vec![
            Instruction::new(0, OpCode::Push10, None),
            Instruction::new(1, OpCode::Stloc0, None),
            Instruction::new(2, OpCode::Ret, None),
        ];
        let (ins, cfg) = linear(ins);
        let ssa = SsaBuilder::new(&cfg, &ins).build();
        let block = ssa.blocks_iter().next().expect("a block exists").1;

        let has_loc0_assign = block.stmts.iter().any(|s| match s {
            SsaStmt::Assign { target, .. } => target.base == "loc0",
            _ => false,
        });
        assert!(
            has_loc0_assign,
            "STLOC0 should define a loc0 SSA variable; got {:?}",
            block.stmts
        );
    }

    #[test]
    fn store_then_load_connects_within_a_block() {
        // PUSH10 ; STLOC0 ; LDLOC0 ; RET
        //   store defines a loc0 var; the load must read that var, not stay opaque.
        let ins = vec![
            Instruction::new(0, OpCode::Push10, None),
            Instruction::new(1, OpCode::Stloc0, None),
            Instruction::new(2, OpCode::Ldloc0, None),
            Instruction::new(3, OpCode::Ret, None),
        ];
        let (ins, cfg) = linear(ins);
        let ssa = SsaBuilder::new(&cfg, &ins).build();
        let block = ssa.blocks_iter().next().expect("a block exists").1;

        // loc0 defs in order: [store, load].
        let loc0_defs: Vec<&SsaStmt> = block
            .stmts
            .iter()
            .filter(|s| matches!(s, SsaStmt::Assign { target, .. } if target.base == "loc0"))
            .collect();
        assert!(
            loc0_defs.len() >= 2,
            "expected a store def and a load def for loc0; got {:?}",
            block.stmts
        );
        // The last loc0 def is the load: it must reference the stored var, NOT
        // be an opaque ldloc0() Call.
        let load_def = loc0_defs.last().copied().unwrap();
        let SsaStmt::Assign { value, .. } = load_def else {
            panic!("load def should be an Assign: {load_def:?}");
        };
        assert!(
            matches!(value, SsaExpr::Variable(_)),
            "LDLOC0 after STLOC0 should read the stored var; got {value:?}"
        );
        assert!(
            !matches!(value, SsaExpr::Call { .. }),
            "LDLOC0 should not stay an opaque ldloc0() call once a store exists; got {value:?}"
        );
    }

    #[test]
    fn diamond_places_a_phi_at_the_merge() {
        // Build a diamond by hand so we control predecessor exit stacks:
        //   BB0 (entry) pushes 1, branches to BB1 / BB2
        //   BB1 pushes 10  -> jmp BB3
        //   BB2 pushes 20  -> jmp BB3
        //   BB3 (merge): the incoming slot should be a φ(BB1: 10, BB2: 20).
        let ins = vec![
            Instruction::new(0, OpCode::Push1, None), // BB0: push 1 (condition-ish)
            Instruction::new(0, OpCode::Pushint8, Some(Operand::I8(10))), // BB1
            Instruction::new(0, OpCode::Pushint8, Some(Operand::I8(20))), // BB2
            Instruction::new(0, OpCode::Ret, None),   // BB3
        ];

        let mut cfg = Cfg::new();
        cfg.add_block(BasicBlock::new(
            BlockId(0),
            0,
            1,
            0..1,
            Terminator::Branch {
                then_target: BlockId(1),
                else_target: BlockId(2),
            },
        ));
        cfg.add_block(BasicBlock::new(
            BlockId(1),
            1,
            2,
            1..2,
            Terminator::Jump { target: BlockId(3) },
        ));
        cfg.add_block(BasicBlock::new(
            BlockId(2),
            2,
            3,
            2..3,
            Terminator::Jump { target: BlockId(3) },
        ));
        cfg.add_block(BasicBlock::new(BlockId(3), 3, 4, 3..4, Terminator::Return));
        cfg.add_edge(BlockId(0), BlockId(1), EdgeKind::ConditionalTrue);
        cfg.add_edge(BlockId(0), BlockId(2), EdgeKind::ConditionalFalse);
        cfg.add_edge(BlockId(1), BlockId(3), EdgeKind::Unconditional);
        cfg.add_edge(BlockId(2), BlockId(3), EdgeKind::Unconditional);

        let ssa = SsaBuilder::new(&cfg, &ins).build();
        let merge = ssa.block(BlockId(3)).expect("merge block exists");
        assert!(
            merge.phi_count() >= 1,
            "merge block should have a phi node for the incoming value slot"
        );
        let phi = &merge.phi_nodes[0];
        assert_eq!(
            phi.operands.len(),
            2,
            "phi should have one operand per predecessor"
        );
    }

    #[test]
    fn diamond_places_a_phi_for_a_slot() {
        // Two arms store different values to the same local; the merge loads it
        // and so needs a slot φ(loc0) over BB1 / BB2.
        //   BB0: Push1, STLOC0, Branch -> BB1 / BB2
        //   BB1: PUSH11, STLOC0 -> jmp BB3
        //   BB2: PUSH12, STLOC0 -> jmp BB3
        //   BB3: LDLOC0, RET
        let ins = vec![
            Instruction::new(0, OpCode::Push1, None),
            Instruction::new(1, OpCode::Stloc0, None),
            Instruction::new(0, OpCode::Push11, None),
            Instruction::new(1, OpCode::Stloc0, None),
            Instruction::new(0, OpCode::Push12, None),
            Instruction::new(1, OpCode::Stloc0, None),
            Instruction::new(0, OpCode::Ldloc0, None),
            Instruction::new(0, OpCode::Ret, None),
        ];

        let mut cfg = Cfg::new();
        cfg.add_block(BasicBlock::new(
            BlockId(0),
            0,
            1,
            0..2,
            Terminator::Branch {
                then_target: BlockId(1),
                else_target: BlockId(2),
            },
        ));
        cfg.add_block(BasicBlock::new(
            BlockId(1),
            1,
            2,
            2..4,
            Terminator::Jump { target: BlockId(3) },
        ));
        cfg.add_block(BasicBlock::new(
            BlockId(2),
            2,
            3,
            4..6,
            Terminator::Jump { target: BlockId(3) },
        ));
        cfg.add_block(BasicBlock::new(BlockId(3), 3, 4, 6..8, Terminator::Return));
        cfg.add_edge(BlockId(0), BlockId(1), EdgeKind::ConditionalTrue);
        cfg.add_edge(BlockId(0), BlockId(2), EdgeKind::ConditionalFalse);
        cfg.add_edge(BlockId(1), BlockId(3), EdgeKind::Unconditional);
        cfg.add_edge(BlockId(2), BlockId(3), EdgeKind::Unconditional);

        let ssa = SsaBuilder::new(&cfg, &ins).build();
        let merge = ssa.block(BlockId(3)).expect("merge block exists");

        let has_slot_phi = merge.phi_nodes.iter().any(|phi| phi.target.base == "loc0");
        assert!(
            has_slot_phi,
            "merge of two STLOC0 arms should place a loc0 φ; got {:?}",
            merge.phi_nodes
        );
    }

    #[test]
    fn partially_initialized_slot_merge_is_incomplete() {
        let instructions = vec![
            instr(0, OpCode::Push1),
            instr(1, OpCode::Push1),
            instr(2, OpCode::Stloc0),
            instr(3, OpCode::Nop),
            instr(4, OpCode::Ldloc0),
            instr(5, OpCode::Ret),
        ];
        let mut cfg = Cfg::new();
        cfg.add_block(BasicBlock::new(
            BlockId(0),
            0,
            1,
            0..1,
            Terminator::Branch {
                then_target: BlockId(1),
                else_target: BlockId(2),
            },
        ));
        cfg.add_block(BasicBlock::new(
            BlockId(1),
            1,
            3,
            1..3,
            Terminator::Jump { target: BlockId(3) },
        ));
        cfg.add_block(BasicBlock::new(
            BlockId(2),
            3,
            4,
            3..4,
            Terminator::Jump { target: BlockId(3) },
        ));
        cfg.add_block(BasicBlock::new(BlockId(3), 4, 6, 4..6, Terminator::Return));
        cfg.add_edge(BlockId(0), BlockId(1), EdgeKind::ConditionalTrue);
        cfg.add_edge(BlockId(0), BlockId(2), EdgeKind::ConditionalFalse);
        cfg.add_edge(BlockId(1), BlockId(3), EdgeKind::Unconditional);
        cfg.add_edge(BlockId(2), BlockId(3), EdgeKind::Unconditional);
        let context = MethodContext {
            returns_value: Some(true),
            ..MethodContext::default()
        };

        let built = SsaBuilder::new(&cfg, &instructions)
            .with_method_context(&context)
            .build_with_report();

        assert!(built.fidelity.issues.iter().any(|issue| {
            issue.offset == 4
                && issue.opcode == OpCode::Ldloc0
                && issue.kind == LoweringIssueKind::LostStackValue
                && issue.fidelity == Fidelity::Incomplete
        }));
        let merge = built.ssa.block(BlockId(3)).expect("merge block");
        assert!(merge
            .phi_nodes
            .iter()
            .any(|phi| { phi.target.base == "loc0" && phi.operands.values().any(is_unknown) }));
    }

    #[test]
    fn initslot_seeds_locals_with_null_before_partial_assignment() {
        let instructions = vec![
            Instruction::new(0, OpCode::Initslot, Some(Operand::Bytes(vec![1, 0]))),
            instr(3, OpCode::PushT),
            instr(4, OpCode::Push1),
            instr(5, OpCode::Stloc0),
            instr(6, OpCode::Nop),
            instr(7, OpCode::Ldloc0),
            instr(8, OpCode::Ret),
        ];
        let mut cfg = Cfg::new();
        cfg.add_block(BasicBlock::new(
            BlockId(0),
            0,
            4,
            0..2,
            Terminator::Branch {
                then_target: BlockId(1),
                else_target: BlockId(2),
            },
        ));
        cfg.add_block(BasicBlock::new(
            BlockId(1),
            4,
            6,
            2..4,
            Terminator::Jump { target: BlockId(3) },
        ));
        cfg.add_block(BasicBlock::new(
            BlockId(2),
            6,
            7,
            4..5,
            Terminator::Jump { target: BlockId(3) },
        ));
        cfg.add_block(BasicBlock::new(BlockId(3), 7, 9, 5..7, Terminator::Return));
        cfg.add_edge(BlockId(0), BlockId(1), EdgeKind::ConditionalTrue);
        cfg.add_edge(BlockId(0), BlockId(2), EdgeKind::ConditionalFalse);
        cfg.add_edge(BlockId(1), BlockId(3), EdgeKind::Unconditional);
        cfg.add_edge(BlockId(2), BlockId(3), EdgeKind::Unconditional);
        let context = MethodContext {
            returns_value: Some(true),
            ..MethodContext::default()
        };

        let built = SsaBuilder::new(&cfg, &instructions)
            .with_method_context(&context)
            .build_with_report();

        assert!(!built.fidelity.issues.iter().any(|issue| {
            issue.offset == 7
                && issue.opcode == OpCode::Ldloc0
                && issue.kind == LoweringIssueKind::LostStackValue
        }));
        assert!(built.ssa.block(BlockId(0)).is_some_and(|block| {
            block.stmts.iter().all(|statement| {
                !matches!(
                    statement,
                    SsaStmt::Assign {
                        target,
                        value: SsaExpr::Literal(Literal::Null),
                    } if target.base == "loc0"
                )
            })
        }));
        let merge = built.ssa.block(BlockId(3)).expect("merge block");
        assert!(merge.phi_nodes.iter().any(|phi| {
            phi.target.base == "loc0"
                && phi.operands.len() == 2
                && phi.operands.values().all(|operand| !is_unknown(operand))
                && phi.operands.values().any(SsaVariable::is_vm_null)
        }));
    }

    #[test]
    fn first_static_load_establishes_snapshot_for_non_writing_branch() {
        let instructions = vec![
            instr(0, OpCode::Ldsfld0),
            instr(1, OpCode::PushNull),
            instr(2, OpCode::Stsfld0),
            instr(3, OpCode::Nop),
            instr(4, OpCode::Ldsfld0),
            instr(5, OpCode::Ret),
        ];
        let mut cfg = Cfg::new();
        cfg.add_block(BasicBlock::new(
            BlockId(0),
            0,
            1,
            0..1,
            Terminator::Branch {
                then_target: BlockId(1),
                else_target: BlockId(2),
            },
        ));
        cfg.add_block(BasicBlock::new(
            BlockId(1),
            1,
            3,
            1..3,
            Terminator::Jump { target: BlockId(3) },
        ));
        cfg.add_block(BasicBlock::new(
            BlockId(2),
            3,
            4,
            3..4,
            Terminator::Jump { target: BlockId(3) },
        ));
        cfg.add_block(BasicBlock::new(BlockId(3), 4, 6, 4..6, Terminator::Return));
        cfg.add_edge(BlockId(0), BlockId(1), EdgeKind::ConditionalTrue);
        cfg.add_edge(BlockId(0), BlockId(2), EdgeKind::ConditionalFalse);
        cfg.add_edge(BlockId(1), BlockId(3), EdgeKind::Unconditional);
        cfg.add_edge(BlockId(2), BlockId(3), EdgeKind::Unconditional);
        let context = MethodContext {
            returns_value: Some(true),
            ..MethodContext::default()
        };

        let built = SsaBuilder::new(&cfg, &instructions)
            .with_method_context(&context)
            .build_with_report();
        let merge = built.ssa.block(BlockId(3)).expect("merge block");

        assert!(merge.phi_nodes.iter().any(|phi| {
            phi.target.base == "static0"
                && phi.operands.len() == 2
                && phi.operands.values().all(|operand| !is_unknown(operand))
        }));
        assert!(!built.fidelity.issues.iter().any(|issue| {
            issue.offset == 4
                && issue.opcode == OpCode::Ldsfld0
                && issue.kind == LoweringIssueKind::LostStackValue
        }));
    }

    #[test]
    fn loop_phi_uses_ambient_static_value_on_preheader() {
        let instructions = vec![
            instr(0, OpCode::Nop),
            instr(1, OpCode::Nop),
            instr(2, OpCode::Push1),
            instr(3, OpCode::Stsfld0),
            instr(4, OpCode::Ldsfld0),
            instr(5, OpCode::Ret),
        ];
        let preheader = BlockId(0);
        let header = BlockId(1);
        let body = BlockId(2);
        let exit = BlockId(3);
        let mut cfg = Cfg::new();
        cfg.add_block(BasicBlock::new(
            preheader,
            0,
            1,
            0..1,
            Terminator::Jump { target: header },
        ));
        cfg.add_block(BasicBlock::new(
            header,
            1,
            2,
            1..2,
            Terminator::Branch {
                then_target: body,
                else_target: exit,
            },
        ));
        cfg.add_block(BasicBlock::new(
            body,
            2,
            4,
            2..4,
            Terminator::Jump { target: header },
        ));
        cfg.add_block(BasicBlock::new(exit, 4, 6, 4..6, Terminator::Return));
        cfg.add_edge(preheader, header, EdgeKind::Unconditional);
        cfg.add_edge(header, body, EdgeKind::ConditionalTrue);
        cfg.add_edge(header, exit, EdgeKind::ConditionalFalse);
        cfg.add_edge(body, header, EdgeKind::Unconditional);
        let context = MethodContext {
            returns_value: Some(true),
            ..MethodContext::default()
        };

        let built = SsaBuilder::new(&cfg, &instructions)
            .with_method_context(&context)
            .build_with_report();
        let header = built.ssa.block(header).expect("loop header");
        let phi = header
            .phi_nodes
            .iter()
            .find(|phi| phi.target.base == "static0")
            .expect("loop-carried static phi");

        assert_eq!(
            phi.operands.get(&preheader),
            Some(&SsaVariable::initial("static0".to_string()))
        );
        assert!(phi.operands.values().all(|operand| !is_unknown(operand)));
    }

    #[test]
    fn exception_edges_supply_their_payload_at_mixed_joins() {
        let instructions = vec![
            instr(0, OpCode::Push9),
            instr(1, OpCode::Throw),
            instr(2, OpCode::Stloc0),
            instr(3, OpCode::Ret),
        ];
        let normal = BlockId(0);
        let exceptional = BlockId(1);
        let handler = BlockId(2);
        let mut cfg = Cfg::new();
        cfg.add_block(BasicBlock::new(
            normal,
            0,
            1,
            0..1,
            Terminator::Jump { target: handler },
        ));
        cfg.add_block(BasicBlock::new(exceptional, 1, 2, 1..2, Terminator::Throw));
        cfg.add_block(BasicBlock::new(handler, 2, 4, 2..4, Terminator::Return));
        cfg.add_edge(normal, handler, EdgeKind::Unconditional);
        cfg.add_edge(exceptional, handler, EdgeKind::Exception);

        let ssa = SsaBuilder::new(&cfg, &instructions).build();
        let handler_block = ssa.block(handler).expect("handler block");

        assert!(handler_block.phi_nodes.iter().any(|phi| {
            phi.operands
                .values()
                .any(|operand| operand == &SsaVariable::exception_payload(handler))
        }));
    }

    #[test]
    fn exceptional_finally_entry_does_not_taint_normal_return_stack() {
        let instructions = vec![
            Instruction::new(0, OpCode::Try, Some(Operand::Bytes(vec![0, 5]))),
            instr(3, OpCode::Push1),
            Instruction::new(4, OpCode::Endtry, Some(Operand::Jump(3))),
            instr(5, OpCode::Endfinally),
            instr(7, OpCode::Ret),
        ];
        let cfg = CfgBuilder::new(&instructions).build();
        let context = MethodContext {
            returns_value: Some(true),
            ..MethodContext::default()
        };

        let built = SsaBuilder::new(&cfg, &instructions)
            .with_method_context(&context)
            .build_with_report();

        assert!(!built.fidelity.issues.iter().any(|issue| {
            issue.offset == 7
                && issue.opcode == OpCode::Ret
                && issue.kind == LoweringIssueKind::LostStackValue
        }));
        let return_block = cfg.block_at_offset(7).expect("return block").id;
        assert!(matches!(
            built
                .ssa
                .block(return_block)
                .and_then(|block| block.stmts.last()),
            Some(SsaStmt::Return(Some(SsaExpr::Variable(value)))) if value.base != "?"
        ));
    }

    #[test]
    fn known_non_returning_call_does_not_produce_a_stack_value() {
        let instructions = vec![
            Instruction::new(0, OpCode::Call, Some(Operand::Jump(3))),
            instr(2, OpCode::Ret),
            instr(3, OpCode::Abort),
        ];
        let cfg = CfgBuilder::new(&instructions)
            .with_non_returning_calls([0])
            .build();
        let context = MethodContext {
            calls_by_offset: BTreeMap::from([(
                0,
                CallContract::new(
                    SemanticCallTarget::Internal {
                        offset: 3,
                        name: "abort_leaf".to_string(),
                    },
                    0,
                    true,
                )
                .with_may_return(false),
            )]),
            ..MethodContext::default()
        };

        let ssa = SsaBuilder::new(&cfg, &instructions)
            .with_method_context(&context)
            .build();
        let entry = ssa.block(BlockId::ENTRY).expect("entry block");

        assert!(entry.stmts.iter().any(|stmt| matches!(
            stmt,
            SsaStmt::Expr(SsaExpr::Call { target, .. })
                if matches!(target, SemanticCallTarget::Internal { offset: 3, .. })
        )));
        assert!(!entry.stmts.iter().any(|stmt| matches!(
            stmt,
            SsaStmt::Assign {
                value: SsaExpr::Call { .. },
                ..
            }
        )));
    }

    #[test]
    fn entry_loop_slot_without_virtual_initial_value_is_incomplete() {
        let instructions = vec![
            instr(0, OpCode::Ldloc0),
            instr(1, OpCode::Drop),
            instr(2, OpCode::Push1),
            instr(3, OpCode::Stloc0),
        ];
        let mut cfg = Cfg::new();
        cfg.add_block(BasicBlock::new(
            BlockId(0),
            0,
            4,
            0..4,
            Terminator::Jump { target: BlockId(0) },
        ));
        cfg.add_edge(BlockId(0), BlockId(0), EdgeKind::Unconditional);

        let built = SsaBuilder::new(&cfg, &instructions).build_with_report();

        assert!(built.fidelity.issues.iter().any(|issue| {
            issue.offset == 0
                && issue.opcode == OpCode::Ldloc0
                && issue.kind == LoweringIssueKind::LostStackValue
                && issue.fidelity == Fidelity::Incomplete
        }));
        let entry = built.ssa.block(BlockId(0)).expect("entry loop block");
        assert!(entry
            .phi_nodes
            .iter()
            .any(|phi| { phi.target.base == "loc0" && phi.operands.values().any(is_unknown) }));
    }
}
