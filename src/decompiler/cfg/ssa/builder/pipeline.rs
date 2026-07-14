//! Fixpoint orchestration and straight-line execution for SSA construction.

use super::*;

impl<'a> SsaBuilder<'a> {
    /// Run the fixpoint that produces per-block phi nodes, exit stacks, and the
    /// assembled [`SsaForm`] pieces.
    pub(super) fn build_ssa_blocks(&self) -> SsaBuildResult {
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
        // names across iterations -> stable exit stacks -> fixpoint convergence.
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
        let mut return_facts = Vec::new();
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
                return_facts.extend(exec.return_facts.iter().cloned());
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
                // Phi operands are uses at the block head (stmt_index 0).
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
            // Fold in uses recorded for non-Assign consumers (stores, jumps, ...).
            for (var, idx) in block_uses.get(&bid).cloned().unwrap_or_default() {
                uses.entry(var).or_default().insert(UseSite::new(bid, idx));
            }
            ssa_blocks.insert(bid, sb);
        }

        let return_shape = unanimous_collection_shape(&return_shapes);
        let return_facts = unanimous_collection_facts(&return_facts);
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
            return_facts,
            collection_analysis,
        )
    }

    /// Symbolically execute one block straight-line from `entry`, producing the
    /// exit stack, the SSA statements, and the use list
    /// (vars consumed by non-assignment opcodes such as stores / conditions).
    pub(super) fn execute_block(
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
                    self.method_context
                        .and_then(|context| context.calls_by_offset.get(&instr.offset))
                        .and_then(|contract| contract.return_facts.as_ref()),
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
        let return_facts = stmts
            .iter()
            .filter_map(|statement| match statement {
                SsaStmt::Return(Some(SsaExpr::Variable(variable))) => {
                    Some(Some(collection_shape_facts_for_variable(
                        variable,
                        &facts.definitions,
                        &collection_invalidations,
                    )))
                }
                SsaStmt::Return(_) => Some(None),
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
            return_facts,
            argument_field_writes,
            static_collection_writes,
            call_argument_facts,
        }
    }
}
