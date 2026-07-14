// Dynamic-index stack operations for SSA construction.

use super::*;

impl<'a> SsaBuilder<'a> {
    pub(super) fn apply_indexed_stack_operation(
        &self,
        instr: &Instruction,
        stack: &mut Vec<SsaVariable>,
        stmts: &mut Vec<SsaStmt>,
        uses: &mut Vec<(SsaVariable, usize)>,
        state: &mut BuildPassState<'_>,
    ) {
        // The index/count comes from the top of the stack. Neo first coerces
        // it to a signed 32-bit integer, then faults on a negative value or a
        // stack position outside the live depth.
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
}
