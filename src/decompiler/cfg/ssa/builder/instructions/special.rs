//! Operand-dependent collection and stack-special instruction handling.

use super::*;

impl<'a> SsaBuilder<'a> {
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
