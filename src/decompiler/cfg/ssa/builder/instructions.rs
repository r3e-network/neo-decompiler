// Stateful opcode application for stack-effect SSA construction.

use super::*;

mod calls;
mod indexed;
mod reorder;
mod syscall;

impl<'a> SsaBuilder<'a> {
    pub(super) fn apply_drop_bare_throw(
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

    pub(super) fn apply_unpack_packstruct(
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
    pub(super) fn apply_instruction(
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

    /// Handle operand-dependent specials: PICK/ROLL/XDROP/REVERSEN (index from
    /// the stack), PACK/PACKMAP/PACKSTRUCT/UNPACK (count from the stack),
    /// CLEAR (empties), and SYSCALL (arity from the syscall table).
    pub(super) fn apply_special(
        &self,
        instr: &Instruction,
        stack: &mut Vec<SsaVariable>,
        stmts: &mut Vec<SsaStmt>,
        uses: &mut Vec<(SsaVariable, usize)>,
        state: &mut BuildPassState<'_>,
    ) {
        match instr.opcode {
            OpCode::Pick | OpCode::Roll | OpCode::Xdrop | OpCode::Reversen => {
                self.apply_indexed_stack_operation(instr, stack, stmts, uses, state);
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
}
