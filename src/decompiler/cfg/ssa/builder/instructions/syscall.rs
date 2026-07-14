// Syscall-specific SSA stack effects.

use super::*;

impl<'a> SsaBuilder<'a> {
    pub(super) fn apply_syscall(
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
}
