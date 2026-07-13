# Structured IR Call-Temp Elision Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Render returned and dropped generated call results without presentation-only SSA assignments while preserving Neo evaluation order.

**Architecture:** Make `StructCtx::emit_body` recognize only an adjacent generated `t` call assignment and its sole return use. Precompute typed SSA references, then let the guarded `emit_ssa_stmt` arm treat a generated call temp as unused only when neither the use index nor the typed graph references it; leave every other SSA shape unchanged.

**Tech Stack:** Rust, existing CFG/SSA/typed IR modules, existing `ir_pipeline` integration tests. No new dependencies or public types.

---

### Task 1: Lock the Safe Structurer Boundary

**Files:**
- Modify: `src/decompiler/cfg/structure.rs`

- [x] **Step 1: Add a single-block test helper with explicit use sites**

Inside `structure.rs`'s test module, add this helper. Keep use-site construction explicit in each test so missing fixture indexes cannot masquerade as real analysis.

```rust
fn single_block_ssa(
    statements: Vec<SsaStmt>,
    uses: BTreeMap<SsaVariable, BTreeSet<UseSite>>,
) -> SsaForm {
    let mut cfg = Cfg::new();
    cfg.add_block(BasicBlock::new(
        BlockId(0),
        0,
        1,
        0..1,
        Terminator::Return,
    ));
    let dominance = crate::decompiler::cfg::ssa::compute(&cfg);
    let blocks = BTreeMap::from([(BlockId(0), block_with(statements))]);
    SsaForm {
        cfg,
        dominance,
        blocks,
        definitions: BTreeMap::new(),
        uses,
    }
}
```

- [x] **Step 2: Add the failing adjacent-return test**

Build:

```rust
let temp = v("t", 0);
let statements = vec![
    SsaStmt::assign(
        temp.clone(),
        SsaExpr::call("read".to_string(), vec![]),
    ),
    SsaStmt::ret(Some(SsaExpr::var(temp.clone()))),
];
let uses = BTreeMap::from([(
    temp,
    BTreeSet::from([UseSite::new(BlockId(0), 1)]),
)]);
```

Assert:

```rust
let structured = structure(&single_block_ssa(statements, uses));
assert_eq!(
    structured.stmts,
    vec![Stmt::Return(Some(Expr::call("read", vec![])))]
);
```

- [x] **Step 3: Verify the direct-return test fails for the assignment-plus-return output**

Run:

```bash
cargo test --lib adjacent_single_use_call_temp_is_returned_directly -- --nocapture
```

Expected: FAIL because the current emitter produces two statements.

- [x] **Step 4: Add the failing unused-call test**

Build and assert:

```rust
let structured = structure(&single_block_ssa(
    vec![
        SsaStmt::assign(
            v("t", 0),
            SsaExpr::call("notify".to_string(), vec![]),
        ),
        SsaStmt::ret(None),
    ],
    BTreeMap::new(),
));
assert_eq!(
    structured.stmts,
    vec![Stmt::ExprStmt(Expr::call("notify", vec![])), Stmt::Return(None)]
);
```

- [x] **Step 5: Verify the unused-call test fails for the unused assignment output**

Run:

```bash
cargo test --lib unused_call_temp_is_an_expression_statement -- --nocapture
```

Expected: FAIL because the current emitter retains `t_0 = notify()`.

- [x] **Step 6: Add conservative-boundary regression tests**

Add four tests with these complete fixtures and assertions:

```rust
#[test]
fn multi_use_call_temp_remains_assigned() {
    let temp = v("t", 0);
    let structured = structure(&single_block_ssa(
        vec![
            SsaStmt::assign(
                temp.clone(),
                SsaExpr::call("read".to_string(), vec![]),
            ),
            SsaStmt::assign(v("loc0", 0), SsaExpr::var(temp.clone())),
            SsaStmt::ret(Some(SsaExpr::var(temp.clone()))),
        ],
        BTreeMap::from([(
            temp,
            BTreeSet::from([
                UseSite::new(BlockId(0), 1),
                UseSite::new(BlockId(0), 2),
            ]),
        )]),
    ));
    assert!(matches!(
        structured.stmts.first(),
        Some(Stmt::Assign { target, .. }) if target == "t_0"
    ));
}

#[test]
fn named_slot_call_remains_assigned_when_unused() {
    let structured = structure(&single_block_ssa(
        vec![
            SsaStmt::assign(
                v("loc0", 0),
                SsaExpr::call("read".to_string(), vec![]),
            ),
            SsaStmt::ret(None),
        ],
        BTreeMap::new(),
    ));
    assert!(matches!(
        structured.stmts.first(),
        Some(Stmt::Assign { target, .. }) if target == "loc0_0"
    ));
}

#[test]
fn unused_non_call_temp_remains_assigned() {
    let structured = structure(&single_block_ssa(
        vec![
            SsaStmt::assign(v("t", 0), SsaExpr::lit(Literal::Int(7))),
            SsaStmt::ret(None),
        ],
        BTreeMap::new(),
    ));
    assert!(matches!(
        structured.stmts.first(),
        Some(Stmt::Assign { target, .. }) if target == "t_0"
    ));
}

#[test]
fn call_temp_used_as_call_argument_remains_assigned() {
    let temp = v("t", 0);
    let structured = structure(&single_block_ssa(
        vec![
            SsaStmt::assign(
                temp.clone(),
                SsaExpr::call("read".to_string(), vec![]),
            ),
            SsaStmt::expr(SsaExpr::call(
                "consume".to_string(),
                vec![SsaExpr::var(temp.clone())],
            )),
            SsaStmt::ret(None),
        ],
        BTreeMap::from([(
            temp,
            BTreeSet::from([UseSite::new(BlockId(0), 1)]),
        )]),
    ));
    assert!(matches!(
        structured.stmts.first(),
        Some(Stmt::Assign { target, .. }) if target == "t_0"
    ));
}
```

- [x] **Step 7: Add missing-index public-API regressions from review**

Build one same-block `t_0 = read(); return t_0;` form and one cross-block assignment/return form with empty `definitions` and `uses` maps. Assert both retain the assignment and return the defined `t_0`; before the review fix, both tests fail because the assignment becomes `read();` while the return still references `t_0`.

### Task 2: Implement Local Call-Temp Elision

**Files:**
- Modify: `src/decompiler/cfg/structure.rs`

- [x] **Step 1: Walk ordinary block statements by index**

Replace `emit_body`'s direct `for` loop with an indexed `while` loop so an exact adjacent assignment/return pair can be consumed together. Keep `emit_body_except_condition` on its existing loop because a CFG block with a branch terminator cannot also have a return terminator.

- [x] **Step 2: Emit an adjacent sole-use call as a direct return**

Before ordinary lowering, match:

```rust
(
    SsaStmt::Assign {
        target,
        value: SsaExpr::Call { .. },
    },
    Some(SsaStmt::Return(Some(SsaExpr::Variable(returned)))),
) if target.base == "t" && returned == target
```

Require:

```rust
self.ssa.uses_of(target).is_some_and(|sites| {
    sites.len() == 1 && sites.contains(&UseSite::new(bid, index + 1))
})
```

Lower the original call with `ssa_expr_to_ir_with_source_names`, push `Stmt::Return(Some(call))`, and advance by two statements. Otherwise call `emit_ssa_stmt` and advance by one.

- [x] **Step 3: Emit an unused generated call as an expression statement**

Precompute a `BTreeSet<SsaVariable>` from all statement-expression and phi-operand references. Add a guarded `SsaStmt::Assign` match arm before the general assignment arm in `emit_ssa_stmt`. For a generated `t` assignment containing `SsaExpr::Call`, require both `self.ssa.uses_of(target).is_none_or(BTreeSet::is_empty)` and absence from the structural reference set before lowering the call in its current position as `Stmt::ExprStmt`.

- [x] **Step 4: Preserve condition suppression and ordinary lowering**

Keep `emit_body_except_condition`'s skip check before its call to `emit_ssa_stmt`. Route all unmatched statements through the existing general assignment arm unchanged.

- [x] **Step 5: Verify all structurer tests pass**

Run:

```bash
cargo test --lib decompiler::cfg::structure --all-features -- --nocapture
```

Require exit `0`, including the new direct-return, unused-call, multi-use, named-slot, non-call, and call-argument cases.

### Task 3: Lock End-to-End Syscall Output

**Files:**
- Modify: `tests/ir_pipeline.rs`

- [x] **Step 1: Strengthen the known value-syscall test before implementation**

Update `structured_ir_renders_known_syscall_value` to require:

```rust
ir.lines().any(|line| {
    line.trim() == "return syscall(\"System.Runtime.CheckWitness\", 1);"
})
```

Also reject any line containing `= syscall("System.Runtime.CheckWitness"`.

- [x] **Step 2: Add a dropped value-syscall regression before implementation**

Build `PUSH9; PUSH1; SYSCALL System.Runtime.CheckWitness; DROP; RET` with a manifest returning `Integer`. Require a standalone `syscall("System.Runtime.CheckWitness", 1);`, require `return 9;`, and reject an assigned syscall result.

- [x] **Step 3: Verify the integration assertions fail before implementation**

Run:

```bash
cargo test --test ir_pipeline structured_ir_elides_known_syscall_temp -- --nocapture
cargo test --test ir_pipeline structured_ir_renders_known_syscall_value -- --nocapture
```

Expected: FAIL because value-returning syscalls currently retain generated assignments.

- [x] **Step 4: Verify both syscall behaviors pass after Task 2**

Run the same commands and require exit `0`. The direct return and dropped result must remain visible without `= syscall`.

### Task 4: Review and Verification

**Files:**
- Review: `src/decompiler/cfg/structure.rs`
- Review: `tests/ir_pipeline.rs`
- Review: `docs/superpowers/specs/2026-07-10-structured-ir-call-temp-elision-design.md`
- Review: `docs/superpowers/plans/2026-07-10-structured-ir-call-temp-elision.md`

- [x] **Step 1: Focused verification**

```bash
cargo test --lib decompiler::cfg::structure --all-features
cargo test --test ir_pipeline structured_ir_renders_known_syscall --all-features
cargo fmt -- --check
git diff --check HEAD
```

- [x] **Step 2: Review the narrow change**

Independent review confirmed the evaluation-order boundary but found that an absent `SsaForm::uses` entry was not proof of an unused value through the public construction API. Two red regressions reproduced undefined `t_0` returns with empty indexes. The final guard also checks a precomputed typed-reference set, preserving same-block, cross-block, and phi references while retaining dropped-call elision for complete builder output.

- [x] **Step 3: Repository gates**

```bash
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt -- --check
npm test --prefix js
npm test --prefix web
cargo deny check
git diff --check HEAD
```

The JavaScript suite remains a parity fence even though this slice changes only Rust structured output.

**Workspace constraint:** this plan continues an existing cumulative dirty worktree. Do not create a partial commit that captures unrelated prior changes; any eventual commit must follow the repository Lore commit protocol.
