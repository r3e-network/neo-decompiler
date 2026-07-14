// Fixed-shape stack reordering for SSA construction.

use super::*;

impl<'a> SsaBuilder<'a> {
    /// Handle fixed-shape stack reorders (DUP/OVER/TUCK/SWAP/ROT/REVERSE3/4/
    /// DEPTH/DROP/NIP). New copies get a fresh SSA definition so the single-
    /// assignment property is preserved.
    pub(super) fn apply_reorder(
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
}
