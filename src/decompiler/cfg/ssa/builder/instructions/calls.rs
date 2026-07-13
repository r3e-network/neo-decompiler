// Call-specific SSA stack effects.
use super::*;

impl<'a> SsaBuilder<'a> {
    pub(super) fn apply_opaque_call(
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

    pub(super) fn apply_known_call(
        &self,
        instruction: &Instruction,
        contract: &crate::decompiler::cfg::ssa::context::CallContract,
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

    pub(super) fn apply_known_tail_call(
        &self,
        instruction: &Instruction,
        contract: &crate::decompiler::cfg::ssa::context::CallContract,
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
}
