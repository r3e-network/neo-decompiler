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

use crate::decompiler::cfg::{BlockId, Cfg};
use crate::decompiler::ir::{BinOp, Literal, UnaryOp};
use crate::instruction::{Instruction, OpCode, Operand};

use super::context::MethodContext;
use super::dominance::{self, DominanceInfo};
use super::effects;
use super::form::{SsaBlock, SsaExpr, SsaForm, SsaStmt, UseSite};
use super::variable::SsaVariable;

/// `(blocks, definitions, uses)` — the assembled SSA pieces.
type SsaBuildResult = (
    BTreeMap<BlockId, SsaBlock>,
    BTreeMap<SsaVariable, BlockId>,
    BTreeMap<SsaVariable, BTreeSet<UseSite>>,
);

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
        let (blocks, definitions, uses) = self.build_ssa_blocks();
        SsaForm {
            cfg: self.cfg.clone(),
            dominance: self.dominance,
            blocks,
            definitions,
            uses,
        }
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
        let mut versions: BTreeMap<String, usize> = BTreeMap::new();

        // Upper bound on iterations: a couple of passes beyond the block count
        // is plenty for reducible + irreducible graphs given canonical naming.
        let max_iterations = block_ids.len() + 4;
        let mut changed = true;
        let mut iterations = 0usize;
        while changed && iterations <= max_iterations {
            changed = false;
            iterations += 1;
            versions.clear();
            self.reserve_argument_versions(&mut versions);
            for &bid in &block_ids {
                let (new_entry, _new_phis) = self.compute_join_entry(bid, &exit_stacks);
                let (new_slot_entry, _new_slot_phis) =
                    self.compute_join_slots(bid, &exit_slots, &mut versions);
                let exec = self.execute_block(bid, &new_entry, &new_slot_entry, &mut versions);

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

        versions.clear();
        self.reserve_argument_versions(&mut versions);
        for &bid in &block_ids {
            let entry = entry_stacks.get(&bid).cloned().unwrap_or_default();
            let slot_entry = entry_slots.get(&bid).cloned().unwrap_or_default();
            let (_, stack_phis) = self.compute_join_entry(bid, &exit_stacks);
            let (_, slot_phis) = self.compute_join_slots(bid, &exit_slots, &mut versions);
            let exec = self.execute_block(bid, &entry, &slot_entry, &mut versions);

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

        (ssa_blocks, definitions, uses)
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
        let preds = self.cfg.predecessors(bid);
        let initial_arguments = self.initial_entry_stack(bid);
        if preds.is_empty() {
            return (initial_arguments, Vec::new());
        }

        let mut entry = Vec::new();
        let mut phis = Vec::new();
        let predecessor_depth = preds
            .iter()
            .filter_map(|pred| exit_stacks.get(pred))
            .map(Vec::len)
            .max()
            .unwrap_or(0);
        let max_depth = predecessor_depth.max(initial_arguments.len());
        let entry_source = BlockId::from(usize::MAX);
        for depth in 0..max_depth {
            // A predecessor with a known but shorter stack contributes `?` at
            // this depth. Skipping it would fabricate a value on that path.
            let mut operands: Vec<(BlockId, SsaVariable)> = Vec::new();
            for pred in preds {
                if let Some(stack) = exit_stacks.get(pred) {
                    let leading_underflow = max_depth.saturating_sub(stack.len());
                    let variable = depth
                        .checked_sub(leading_underflow)
                        .and_then(|index| stack.get(index))
                        .cloned()
                        .unwrap_or_else(unknown_var);
                    operands.push((*pred, variable));
                }
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
        let preds = self.cfg.predecessors(bid);
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
        for pred in preds {
            if let Some(state) = exit_slots.get(pred) {
                for name in state.keys() {
                    names.insert(name.clone());
                }
            }
        }

        let mut entry = SlotState::new();
        let mut phis = Vec::new();
        for name in names {
            let mut operands: Vec<(BlockId, SsaVariable)> = Vec::new();
            for pred in preds {
                if let Some(var) = exit_slots.get(pred).and_then(|s| s.get(&name)) {
                    operands.push((*pred, var.clone()));
                }
            }
            if let Some(initial) = initial_arguments.get(&name) {
                operands.push((entry_source, initial.clone()));
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
        versions: &mut BTreeMap<String, usize>,
    ) -> BlockExec {
        let Some(block) = self.cfg.block(bid) else {
            return BlockExec::default();
        };
        let mut stack: Vec<SsaVariable> = entry.to_vec();
        let mut slots: SlotState = entry_slots.clone();
        let mut stmts: Vec<SsaStmt> = Vec::new();
        let mut uses: Vec<(SsaVariable, usize)> = Vec::new();
        let mut terminator_condition = None;

        for idx in block.instruction_range.clone() {
            let Some(instr) = self.instructions.get(idx) else {
                continue;
            };
            if let Some(condition) = self.apply_instruction(
                instr, &mut stack, &mut slots, &mut stmts, &mut uses, versions,
            ) {
                terminator_condition = Some(condition);
            }
        }

        BlockExec {
            exit_stack: stack,
            exit_slots: slots,
            stmts,
            uses,
            terminator_condition,
        }
    }

    /// Apply a single instruction's stack effect / transformation.
    fn apply_instruction(
        &self,
        instr: &Instruction,
        stack: &mut Vec<SsaVariable>,
        slots: &mut SlotState,
        stmts: &mut Vec<SsaStmt>,
        uses: &mut Vec<(SsaVariable, usize)>,
        versions: &mut BTreeMap<String, usize>,
    ) -> Option<SsaVariable> {
        let op = instr.opcode;

        if op == OpCode::Ret {
            let returns_value = self
                .method_context
                .and_then(|context| context.returns_value);
            let value = if returns_value == Some(false) {
                None
            } else {
                stack.last().cloned()
            };
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
                self.apply_known_call(instr, contract, stack, stmts, uses, versions);
            } else {
                self.apply_opaque_call(instr, stack, stmts, uses, versions);
            }
            return None;
        }

        if effects::is_stack_reorder(op) {
            self.apply_reorder(op, stack, stmts, versions);
            return None;
        }
        if effects::is_stack_special(op) {
            self.apply_special(instr, stack, stmts, uses, versions);
            return None;
        }

        let (pop, push) = effects::stack_effect(op);

        // Pop consumers (top-first). Reversed afterwards so `popped` is
        // ordered deep-to-top, matching source-language operand order.
        let mut popped: Vec<SsaVariable> = Vec::with_capacity(pop);
        for _ in 0..pop {
            let v = stack.pop().unwrap_or_else(unknown_var);
            popped.push(v);
        }
        popped.reverse();

        // Record uses for the consumed values at the current statement index.
        let use_index = stmts.len();
        for v in &popped {
            if !is_unknown(v) {
                uses.push((v.clone(), use_index));
            }
        }

        if is_boolean_branch(op) {
            return popped.first().cloned();
        }

        if let Some(branch_op) = comparison_branch_op(op) {
            let left = popped.first().cloned().unwrap_or_else(unknown_var);
            let right = popped.get(1).cloned().unwrap_or_else(unknown_var);
            let target = fresh_var(versions, "t");
            stmts.push(SsaStmt::assign(
                target.clone(),
                SsaExpr::binary(branch_op, SsaExpr::var(left), SsaExpr::var(right)),
            ));
            return Some(target);
        }

        if let Some(name) = effectful_collection_name(op) {
            stmts.push(SsaStmt::expr(SsaExpr::call(
                name.to_string(),
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
            let expr = match reaching {
                Some(var) => SsaExpr::var(var),
                None => self.build_expr(op, instr, &popped),
            };
            // Slot loads inherit their slot name (loc0/arg1/static2); everything
            // else gets a temp name. The version counter is per-pass-global and
            // deterministic, so names stay stable across fixpoint iterations.
            let base = slot_name_for(op, &instr.operand).unwrap_or_else(|| "t".to_string());
            let target = fresh_var(versions, &base);
            stmts.push(SsaStmt::assign(target.clone(), expr));
            stack.push(target);
        } else if push == 0 {
            // A store defines a new version of its target slot: `loc0_N = <v>`.
            // Other push==0 opcodes (assert/throw/jump condition) only consumed;
            // their uses were already recorded above.
            if let Some(name) = slot_name_for(op, &instr.operand) {
                if let Some(value) = popped.first().cloned() {
                    let target = fresh_var(versions, &name);
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
        versions: &mut BTreeMap<String, usize>,
    ) {
        let pointer =
            (instruction.opcode == OpCode::CallA).then(|| stack.pop().unwrap_or_else(unknown_var));
        if let Some(pointer) = &pointer {
            if !is_unknown(pointer) {
                uses.push((pointer.clone(), stmts.len()));
            }
        }

        // This call site has no resolved contract metadata. Keeping deeper
        // values would let consumed arguments resurface after a dropped result,
        // so invalidate the unknown pre-call stack conservatively.
        stack.clear();

        let value = SsaExpr::call(
            call_name(instruction.opcode, instruction),
            pointer.into_iter().map(SsaExpr::var).collect(),
        );
        let target = fresh_var(versions, "t");
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
        versions: &mut BTreeMap<String, usize>,
    ) {
        if instruction.opcode == OpCode::CallA {
            let pointer = stack.pop().unwrap_or_else(unknown_var);
            if !is_unknown(&pointer) {
                uses.push((pointer, stmts.len()));
            }
        }

        let mut args = Vec::with_capacity(contract.argument_count);
        for _ in 0..contract.argument_count {
            let argument = stack.pop().unwrap_or_else(unknown_var);
            if !is_unknown(&argument) {
                uses.push((argument.clone(), stmts.len()));
            }
            args.push(SsaExpr::var(argument));
        }

        let call = SsaExpr::call(contract.name.clone(), args);
        if contract.returns_value {
            let target = fresh_var(versions, "t");
            stmts.push(SsaStmt::assign(target.clone(), call));
            stack.push(target);
        } else {
            stmts.push(SsaStmt::expr(call));
        }
    }

    /// Handle fixed-shape stack reorders (DUP/OVER/TUCK/SWAP/ROT/REVERSE3/4/
    /// DEPTH/DROP/NIP). New copies get a fresh SSA definition so the single-
    /// assignment property is preserved.
    fn apply_reorder(
        &self,
        op: OpCode,
        stack: &mut Vec<SsaVariable>,
        stmts: &mut Vec<SsaStmt>,
        versions: &mut BTreeMap<String, usize>,
    ) {
        let mut fresh_copy =
            |src: SsaVariable, stack: &mut Vec<SsaVariable>, stmts: &mut Vec<SsaStmt>| {
                let target = fresh_var(versions, "t");
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
                let target = fresh_var(versions, "t");
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
        versions: &mut BTreeMap<String, usize>,
    ) {
        match instr.opcode {
            OpCode::Pick | OpCode::Roll | OpCode::Xdrop | OpCode::Reversen => {
                // Index comes from the top of the stack as an integer value.
                let n_var = stack.pop();
                if let Some(ref v) = n_var {
                    if !is_unknown(v) {
                        uses.push((v.clone(), 0));
                    }
                }
                // Structural transforms require a proven concrete index, which
                // the SSA layer does not track (the emitter does this at lift
                // time). Leave the reorder conservatively unapplied here.
            }
            OpCode::Clear => {
                stack.clear();
            }
            OpCode::Syscall => self.apply_syscall(instr, stack, stmts, uses, versions),
            // PACK family: count is on the stack and operand-dependent. Model
            // conservatively (drop the count, push one result) so the stack
            // stays consistent without a precise count analysis. Phase 3 can
            // refine this with the literal-count tracking the emitter already
            // performs.
            OpCode::Pack | OpCode::Packmap | OpCode::Packstruct => {
                let count = stack.pop().unwrap_or_else(unknown_var);
                if !is_unknown(&count) {
                    uses.push((count, 0));
                }
                stack.push(unknown_var());
            }
            OpCode::Unpack => {
                let _item = stack.pop();
                stack.push(unknown_var());
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
        versions: &mut BTreeMap<String, usize>,
    ) {
        let info = match &instruction.operand {
            Some(Operand::Syscall(hash)) => crate::syscalls::lookup(*hash),
            _ => None,
        };

        let Some(info) = info else {
            let selector = match &instruction.operand {
                Some(Operand::Syscall(hash)) => format!("0x{hash:08X}"),
                _ => "unknown".to_string(),
            };
            stack.clear();
            let call = SsaExpr::call(
                "syscall".to_string(),
                vec![SsaExpr::lit(Literal::String(selector))],
            );
            let target = fresh_var(versions, "t");
            stmts.push(SsaStmt::assign(target.clone(), call));
            stack.push(target);
            return;
        };

        let use_index = stmts.len();
        let mut args = Vec::with_capacity(usize::from(info.param_count) + 1);
        args.push(SsaExpr::lit(Literal::String(info.name.to_string())));
        for _ in 0..info.param_count {
            let argument = stack.pop().unwrap_or_else(unknown_var);
            if !is_unknown(&argument) {
                uses.push((argument.clone(), use_index));
            }
            args.push(SsaExpr::var(argument));
        }

        let call = SsaExpr::call("syscall".to_string(), args);
        if info.returns_value {
            let target = fresh_var(versions, "t");
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
        // Ternary compute (Within/Substr/Modmul/Modpow/Convert): render as a call.
        if matches!(
            op,
            OpCode::Within | OpCode::Substr | OpCode::Modmul | OpCode::Modpow | OpCode::Convert
        ) {
            return SsaExpr::call(
                mnemonic(op),
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
                | OpCode::Istype
        ) {
            return SsaExpr::call(
                mnemonic(op),
                popped.iter().cloned().map(SsaExpr::var).collect(),
            );
        }
        // Collection constructors / byte ops without a dedicated expr.
        SsaExpr::call(
            mnemonic(op),
            popped.iter().cloned().map(SsaExpr::var).collect(),
        )
    }
}

// ─────────────────────────── helpers ───────────────────────────

/// Straight-line execution result for one block.
#[derive(Default)]
struct BlockExec {
    exit_stack: Vec<SsaVariable>,
    exit_slots: SlotState,
    stmts: Vec<SsaStmt>,
    uses: Vec<(SsaVariable, usize)>,
    terminator_condition: Option<SsaVariable>,
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

/// Placeholder used when the symbolic stack underflows (malformed input).
fn unknown_var() -> SsaVariable {
    SsaVariable::new("?".to_string(), 0)
}

fn is_unknown(v: &SsaVariable) -> bool {
    v.base == "?"
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
            Some(Operand::Bytes(b)) => Some(Literal::BigInt(hex::encode(b))),
            _ => None,
        },
        Pushdata1 | Pushdata2 | Pushdata4 => match &instr.operand {
            Some(Operand::Bytes(b)) => Some(Literal::Bytes(b.clone())),
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

fn effectful_collection_name(op: OpCode) -> Option<&'static str> {
    use OpCode::*;
    match op {
        Setitem => Some("set_item"),
        Append => Some("append"),
        Remove => Some("remove_item"),
        Clearitems => Some("clear_items"),
        Reverseitems => Some("reverse_items"),
        Memcpy => Some("memcpy"),
        _ => None,
    }
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
        SsaExpr::Array(els) => els.iter().for_each(|e| collect_expr_uses_into(e, out)),
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
    use crate::decompiler::cfg::ssa::{CallContract, MethodContext};
    use crate::decompiler::cfg::{BasicBlock, BlockId, CfgBuilder, EdgeKind, Terminator};
    use crate::instruction::{Instruction, OpCode, Operand};

    /// Build instructions + a matching CFG for a straight-line program.
    fn linear(instrs: Vec<Instruction>) -> (Vec<Instruction>, Cfg) {
        let cfg = CfgBuilder::new(&instrs).build();
        (instrs, cfg)
    }

    fn instr(off: usize, op: OpCode) -> Instruction {
        Instruction::new(off, op, None)
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
                        value: SsaExpr::Call { name, .. }
                    } if target == returned && name == expected_name
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
        context
            .calls_by_offset
            .insert(3, CallContract::new("helper", 2, true));

        let mut ssa = SsaBuilder::new(&cfg, &instructions)
            .with_method_context(&context)
            .build();
        super::super::optimize_ssa(&mut ssa);
        let block = ssa.blocks_iter().next().expect("a block exists").1;

        assert!(
            block.stmts.iter().any(|stmt| matches!(
                stmt,
                SsaStmt::Assign {
                    value: SsaExpr::Call { name, args },
                    ..
                } if name == "helper"
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
    fn known_call_contract_emits_void_call_without_phantom_result() {
        let instructions = vec![
            instr(0, OpCode::Push9),
            instr(1, OpCode::Push1),
            Instruction::new(2, OpCode::CallT, Some(Operand::U16(0))),
            instr(5, OpCode::Ret),
        ];
        let cfg = CfgBuilder::new(&instructions).build();
        let mut context = MethodContext::default();
        context
            .calls_by_offset
            .insert(2, CallContract::new("notify", 1, false));

        let mut ssa = SsaBuilder::new(&cfg, &instructions)
            .with_method_context(&context)
            .build();
        super::super::optimize_ssa(&mut ssa);
        let block = ssa.blocks_iter().next().expect("a block exists").1;

        assert!(
            block.stmts.iter().any(|stmt| matches!(
                stmt,
                SsaStmt::Expr(SsaExpr::Call { name, args })
                    if name == "notify"
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
        context
            .calls_by_offset
            .insert(7, CallContract::new("delegate", 1, false));

        let mut ssa = SsaBuilder::new(&cfg, &instructions)
            .with_method_context(&context)
            .build();
        super::super::optimize_ssa(&mut ssa);
        let block = ssa.blocks_iter().next().expect("a block exists").1;

        assert!(
            block.stmts.iter().any(|stmt| matches!(
                stmt,
                SsaStmt::Expr(SsaExpr::Call { name, args })
                    if name == "delegate"
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
                    SsaStmt::Expr(SsaExpr::Call { name, args }) if name == expected_name => {
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
                SsaStmt::Expr(SsaExpr::Call { name, args }) if name == expected_name => Some(args),
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
                value: SsaExpr::Call { name, args },
                ..
            } if name == "syscall"
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
            SsaStmt::Expr(SsaExpr::Call { name, args })
                if name == "syscall"
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
            SsaStmt::Expr(SsaExpr::Call { name, args })
                if name == "syscall"
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
            SsaStmt::Expr(SsaExpr::Call { name, args })
                if name == "syscall"
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
                value: SsaExpr::Call { name, args },
                ..
            } if name == "syscall"
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
}
