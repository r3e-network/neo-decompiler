use std::collections::{BTreeMap, BTreeSet};

use crate::decompiler::cfg::{BlockId, EdgeKind};
use crate::decompiler::ir::SemanticCallTarget;
use crate::instruction::{Instruction, OpCode};

use super::super::context::CollectionShapeFacts;
use super::super::form::SsaExpr;
use super::super::variable::SsaVariable;
use super::slots::*;
use super::{
    absent_slot_value, fresh_var, is_static_slot_name, is_unknown, phi_var, unknown_var,
    BuildFacts, CollectionInvalidations, DefinitionFact, DefinitionFacts, SlotState, SsaBuilder,
};

fn is_slot_load_opcode(opcode: OpCode) -> bool {
    matches!(
        opcode,
        OpCode::Ldloc0
            | OpCode::Ldloc1
            | OpCode::Ldloc2
            | OpCode::Ldloc3
            | OpCode::Ldloc4
            | OpCode::Ldloc5
            | OpCode::Ldloc6
            | OpCode::Ldloc
            | OpCode::Ldarg0
            | OpCode::Ldarg1
            | OpCode::Ldarg2
            | OpCode::Ldarg3
            | OpCode::Ldarg4
            | OpCode::Ldarg5
            | OpCode::Ldarg6
            | OpCode::Ldarg
            | OpCode::Ldsfld0
            | OpCode::Ldsfld1
            | OpCode::Ldsfld2
            | OpCode::Ldsfld3
            | OpCode::Ldsfld4
            | OpCode::Ldsfld5
            | OpCode::Ldsfld6
            | OpCode::Ldsfld
    )
}

impl<'a> SsaBuilder<'a> {
    pub(super) fn tainted_phi_targets(
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
    pub(super) fn compute_join_entry(
        &self,
        bid: BlockId,
        exit_stacks: &BTreeMap<BlockId, Vec<SsaVariable>>,
    ) -> (Vec<SsaVariable>, Vec<super::super::variable::PhiNode>) {
        use super::super::variable::PhiNode;
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
        let recovered_dup_value = self.recover_dup_join_value(bid, &incoming_stacks);
        for depth in 0..max_depth {
            // A predecessor with a known but shorter stack contributes `?` at
            // this depth. Skipping it would fabricate a value on that path.
            let mut operands: Vec<(BlockId, SsaVariable)> = Vec::new();
            for (pred, stack) in &incoming_stacks {
                let leading_underflow = max_depth.saturating_sub(stack.len());
                let variable = recovered_dup_value
                    .as_ref()
                    .filter(|(recovered_pred, _, _)| pred == recovered_pred)
                    .and_then(|(_, recovered_depth, recovered_value)| {
                        if depth < *recovered_depth {
                            stack.get(depth).cloned()
                        } else if depth == *recovered_depth {
                            Some(recovered_value.clone())
                        } else {
                            None
                        }
                    })
                    .or_else(|| {
                        depth
                            .checked_sub(leading_underflow)
                            .and_then(|index| stack.get(index))
                            .cloned()
                    })
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

    /// A compiler-generated conditional value may reach a merge through two
    /// stack shapes: the longer path keeps an ambient value and pushes the
    /// normalized value, while the shorter path leaves the original value on
    /// top. When the merge begins with DUP, the shorter top value is the
    /// proven operand for that missing slot; treating it as an absent bottom
    /// value creates a false cleanup diagnostic without changing semantics.
    fn recover_dup_join_value(
        &self,
        bid: BlockId,
        incoming_stacks: &[(BlockId, Vec<SsaVariable>)],
    ) -> Option<(BlockId, usize, SsaVariable)> {
        let block = self.cfg.block(bid)?;
        let first = self.instructions.get(block.instruction_range.start)?;
        let slot_load = self.instructions.get(block.instruction_range.start + 1)?;
        let conditional = self.instructions.get(block.instruction_range.start + 2)?;
        if first.opcode != OpCode::Dup
            || !is_slot_load_opcode(slot_load.opcode)
            || !matches!(
                conditional.opcode,
                OpCode::Jmpif | OpCode::Jmpif_L | OpCode::Jmpifnot | OpCode::Jmpifnot_L
            )
            || incoming_stacks.len() != 2
        {
            return None;
        }
        let first = &incoming_stacks[0];
        let second = &incoming_stacks[1];
        let (short_pred, short_stack, long_stack) = if first.1.len() < second.1.len() {
            (first.0, &first.1, &second.1)
        } else if second.1.len() < first.1.len() {
            (second.0, &second.1, &first.1)
        } else {
            return None;
        };
        if long_stack.len() != short_stack.len() + 1
            || long_stack[..short_stack.len()] != short_stack[..]
        {
            return None;
        }
        let value = short_stack.last()?.clone();
        (!is_unknown(&value)).then_some((short_pred, short_stack.len(), value))
    }

    /// Compute a block's entry slot state and the φ nodes it needs, from its
    /// predecessors' current exit slot states. For each slot name present across
    /// the predecessors: if they all agree, the reaching version flows through;
    /// if they disagree, a φ is placed. The φ target is named after the slot
    /// (`loc0_N`) so downstream `strip_version` keeps it associated with the
    /// slot. `versions` is the per-pass counter, shared with `execute_block` so
    /// φ targets and defs draw from one deterministic namespace.
    pub(super) fn compute_join_slots(
        &self,
        bid: BlockId,
        exit_slots: &BTreeMap<BlockId, SlotState>,
        versions: &mut BTreeMap<String, usize>,
    ) -> (SlotState, Vec<super::super::variable::PhiNode>) {
        use super::super::variable::PhiNode;
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

    pub(super) fn compute_join_collection_invalidations(
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

    pub(super) fn initial_entry_stack(&self, bid: BlockId) -> Vec<SsaVariable> {
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

    pub(super) fn reserve_argument_versions(&self, versions: &mut BTreeMap<String, usize>) {
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

    pub(super) fn seed_context_collection_facts(&self, facts: &mut DefinitionFacts) {
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

    pub(super) fn static_collection_facts_for_instruction(
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
}
