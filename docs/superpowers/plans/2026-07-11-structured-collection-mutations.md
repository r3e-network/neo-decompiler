# Structured Collection Mutation Lifting Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Preserve six fixed-arity Neo collection and buffer mutations as ordered effect statements throughout the Rust structured SSA/IR pipeline.

**Architecture:** Keep VM stack semantics in `SsaBuilder`: use the canonical stack-effect table, normalize operands to the builder's existing deep-to-top order, and lower recognized mutations to existing `SsaStmt::Expr(SsaExpr::Call)` values. Existing optimization, structuring, typed IR lowering, and rendering remain unchanged; `MEMCPY`'s canonical effect is corrected to five inputs.

**Tech Stack:** Rust, existing Neo opcode model, CFG/SSA/typed IR modules, built-in unit and integration test harnesses. No new dependencies or public IR variants.

---

### Task 1: Lock Fixed-Arity Mutation Semantics

**Files:**
- Modify: `src/decompiler/cfg/ssa/effects.rs`
- Modify: `src/decompiler/cfg/ssa/builder.rs`

- [x] **Step 1: Correct the expected `MEMCPY` effect in the test**

Extend `collection_ops_have_correct_effects` with the complete mutator table and require:

```rust
assert_eq!(stack_effect(OpCode::Append), (2, 0));
assert_eq!(stack_effect(OpCode::Setitem), (3, 0));
assert_eq!(stack_effect(OpCode::Remove), (2, 0));
assert_eq!(stack_effect(OpCode::Clearitems), (1, 0));
assert_eq!(stack_effect(OpCode::Reverseitems), (1, 0));
assert_eq!(stack_effect(OpCode::Memcpy), (5, 0));
```

- [x] **Step 2: Run the effect test and verify the expected failure**

Run:

```bash
cargo test --lib decompiler::cfg::ssa::effects::tests::collection_ops_have_correct_effects -- --exact --nocapture
```

Expected: failure showing the current `MEMCPY` effect is `(3, 0)` rather than `(5, 0)`.

- [x] **Step 3: Add one failing builder table test for all six operations**

Add a `collection_mutations_emit_ordered_effect_calls` test beside the existing builder call/syscall tests. For each case, push distinct integer literals in source order, append the mutation and `RET`, optimize the SSA, and require exactly one matching effect call:

```rust
let cases = [
    (OpCode::Setitem, "set_item", vec![1, 2, 3]),
    (OpCode::Append, "append", vec![1, 2]),
    (OpCode::Remove, "remove_item", vec![1, 2]),
    (OpCode::Clearitems, "clear_items", vec![1]),
    (OpCode::Reverseitems, "reverse_items", vec![1]),
    (OpCode::Memcpy, "memcpy", vec![1, 2, 3, 4, 5]),
];
```

Construct each push with the existing `instr(offset, OpCode::PushN)` helper and assert the final `SsaExpr::Call` arguments are the corresponding `Literal::Int` values in the same order. Assert the block ends in `SsaStmt::Return(None)` so none of the mutations manufactures a stack result.

- [x] **Step 4: Run the builder test and verify the expected failure**

Run:

```bash
cargo test --lib decompiler::cfg::ssa::builder::tests::collection_mutations_emit_ordered_effect_calls -- --exact --nocapture
```

Expected: failure because zero-result non-slot operations currently emit no statement.

- [x] **Step 5: Add a failing underflow-shape regression**

Build `PUSH1; SETITEM; RET` and assert the effect call has the full declared arity with arguments `[?, ?, 1]`. The two unknowns represent the missing deeper container and key operands while the available top value remains the value operand.

Run:

```bash
cargo test --lib decompiler::cfg::ssa::builder::tests::collection_mutation_underflow_preserves_declared_arity -- --exact --nocapture
```

Expected: failure because `SETITEM` currently emits no effect statement.

### Task 2: Align Shared Contract and Type Semantics

**Files:**
- Modify: `src/decompiler/helpers/lifted.rs`
- Modify: `src/decompiler/analysis/types.rs`
- Modify: `src/decompiler/tests/core/analysis.rs`

- [x] **Step 1: Add a failing entry-stack inference test**

Add a unit test around `estimate_required_entry_stack_depth` using `MEMCPY; RET` with no seeded values:

```rust
let instructions = [
    Instruction::new(0, OpCode::Memcpy, None),
    Instruction::new(1, OpCode::Ret, None),
];
assert_eq!(estimate_required_entry_stack_depth(&instructions), Some(5));
```

Run:

```bash
cargo test --lib decompiler::helpers::lifted::tests::memcpy_requires_five_entry_stack_arguments -- --exact --nocapture
```

Expected: failure with `Some(3)` because specialized argument inference currently groups `MEMCPY` with `SETITEM`.

- [x] **Step 2: Add failing typed-stack regressions**

Add two decompilation analysis tests. `INITSLOT 1,0; NEWARRAY0; REVERSEITEMS; STLOC0; RET` must leave local 0 `Unknown`, and `INITSLOT 1,0; PUSH1..PUSH5; MEMCPY; STLOC0; RET` must also leave local 0 `Unknown`. A known local type would prove a stale mutation operand leaked into the following store.

Run:

```bash
cargo test --lib type_inference_consumes_ -- --nocapture
```

Expected: both tests fail, currently inferring `Array` and `Integer` respectively.

- [x] **Step 3: Correct the specialized method-argument effect**

Split the combined branch in `stack_effect_for_arg_inference`:

```rust
Setitem => Some(StackEffect { pops: 3, pushes: vec![] }),
Memcpy => Some(StackEffect { pops: 5, pushes: vec![] }),
```

- [x] **Step 4: Consume typed mutation operands**

Add `Reverseitems` beside the one-pop `Clearitems` branch. Add a `Memcpy` branch that calls `pop_or_unknown` exactly five times and pushes nothing.

- [x] **Step 5: Run all three semantic-analysis regressions**

```bash
cargo test --lib decompiler::helpers::lifted::tests::memcpy_requires_five_entry_stack_arguments -- --exact --nocapture
cargo test --lib type_inference_consumes_ -- --nocapture
```

Expected: all three tests pass with no warnings.

### Task 3: Prove Effect Inputs Survive Optimization

**Files:**
- Modify: `src/decompiler/cfg/ssa/optimize.rs`

- [x] **Step 1: Add a regression around an effect statement's input definition**

Create a block containing a non-substitutable constructor definition, its mutation use, and a bare return:

```rust
block.add_stmt(assign_str(
    v("t", 0),
    SsaExpr::call("newmap".to_string(), vec![]),
));
block.add_stmt(SsaStmt::expr(SsaExpr::call(
    "set_item".to_string(),
    vec![
        SsaExpr::var(v("t", 0)),
        SsaExpr::lit(Literal::Int(1)),
        SsaExpr::lit(Literal::Int(2)),
    ],
)));
block.add_stmt(SsaStmt::ret(None));
```

After `optimize`, assert the `newmap` assignment and `set_item` statement both remain, `ssa.definitions` still contains `t_0`, and `ssa.uses[t_0]` names the effect statement's index.

- [x] **Step 2: Temporarily run the test without the effect statement to prove it catches DCE**

Run the new test once with the `SsaStmt::expr(...)` line omitted.

Expected: failure because `newmap` is converted to a standalone effect and `t_0` no longer has the required definition/use relationship.

Restore the effect statement before continuing. This is the red phase for the optimizer invariant; no optimizer production change is expected.

- [x] **Step 3: Run the restored optimizer regression**

Run:

```bash
cargo test --lib decompiler::cfg::ssa::optimize::tests::effect_statement_keeps_input_definition_live -- --exact --nocapture
```

Expected: pass, proving the existing optimizer already supplies the required preservation behavior.

### Task 4: Lock End-to-End Structured IR Output

**Files:**
- Modify: `tests/ir_pipeline.rs`

- [x] **Step 1: Add a failing `SETITEM` pipeline regression**

Build this script:

```rust
let nef = build_nef(&[0xC8, 0x11, 0x12, 0xD0, 0x40]);
```

It is `NEWMAP; PUSH1; PUSH2; SETITEM; RET`. Require ordered, visible output:

```rust
let constructor = ir
    .find("newmap()")
    .unwrap_or_else(|| panic!("map constructor must remain visible:\n{ir}"));
let mutation = ir
    .find("set_item(t_0, 1, 2);")
    .unwrap_or_else(|| panic!("SETITEM must remain visible:\n{ir}"));
assert!(constructor < mutation, "constructor must precede mutation:\n{ir}");
assert!(ir.contains("return;"), "mutation must not manufacture a return value:\n{ir}");
```

- [x] **Step 2: Run the pipeline regression and verify the expected failure**

Run:

```bash
cargo test --test ir_pipeline structured_ir_preserves_setitem_mutation -- --exact --nocapture
```

Expected: failure because current output contains `newmap();` and `return;` but no `set_item` statement.

### Task 5: Emit Mutations Through Existing SSA Calls

**Files:**
- Modify: `src/decompiler/cfg/ssa/effects.rs`
- Modify: `src/decompiler/cfg/ssa/builder.rs`

- [x] **Step 1: Correct the production `MEMCPY` effect**

Change the canonical table entry to:

```rust
Memcpy => (5, 0), // destination, destination index, source, source index, count
```

- [x] **Step 2: Add the focused opcode-to-helper mapping**

Add this private helper near the builder's existing opcode helpers:

```rust
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
```

- [x] **Step 3: Emit the effect statement after operand normalization**

In `apply_instruction`, after branch/comparison handling and before the `push == 1` branch, add:

```rust
if let Some(name) = effectful_collection_name(op) {
    stmts.push(SsaStmt::expr(SsaExpr::call(
        name.to_string(),
        popped.into_iter().map(SsaExpr::var).collect(),
    )));
    return None;
}
```

This must consume no additional stack values, create no SSA target, and preserve the use records already collected for `popped`.

- [x] **Step 4: Run all focused tests and require green output**

Run:

```bash
cargo test --lib decompiler::cfg::ssa::effects::tests::collection_ops_have_correct_effects -- --exact --nocapture
cargo test --lib decompiler::cfg::ssa::builder::tests::collection_mutations_emit_ordered_effect_calls -- --exact --nocapture
cargo test --lib decompiler::cfg::ssa::builder::tests::collection_mutation_underflow_preserves_declared_arity -- --exact --nocapture
cargo test --lib decompiler::cfg::ssa::optimize::tests::effect_statement_keeps_input_definition_live -- --exact --nocapture
cargo test --test ir_pipeline structured_ir_preserves_setitem_mutation -- --exact --nocapture
```

Expected: all five commands exit `0` with no warnings.

### Task 6: Review and Repository Verification

**Files:**
- Review: `src/decompiler/cfg/ssa/effects.rs`
- Review: `src/decompiler/cfg/ssa/builder.rs`
- Review: `src/decompiler/cfg/ssa/optimize.rs`
- Review: `src/decompiler/helpers/lifted.rs`
- Review: `src/decompiler/analysis/types.rs`
- Review: `tests/ir_pipeline.rs`

- [x] **Step 1: Run focused module and integration suites**

```bash
cargo test --lib decompiler::cfg::ssa --all-features
cargo test --test ir_pipeline --all-features
```

- [x] **Step 2: Request an independent code review**

Review the mutation-only diff against this plan. Require the reviewer to check exact `MEMCPY` order, preservation of unknown underflow arguments, absence of phantom results, optimizer def/use consistency, and non-expansion into assertion or terminator semantics. Fix every critical or important finding before continuing.

- [x] **Step 3: Run repository gates**

```bash
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
npm test --prefix js
npm test --prefix web
npm run build --prefix web
cargo deny check
git diff --check HEAD
```

Parse `docs/schema/decompile.schema.json` as JSON and run the existing TypeScript declaration check used by the shared method-contract slice. Treat the three known unmatched-license warnings from `cargo deny` as existing policy output only if the command exits successfully.

- [x] **Step 4: Audit completion against the design**

Confirm all six mappings, exact arities, deep-to-top argument order, underflow visibility, zero-result behavior, optimizer preservation, and end-to-end `SETITEM` output. Confirm no JavaScript, C#, public API, assertion, exception, terminator, or phi-lowering behavior changed in this slice.

**Workspace constraint:** this plan continues an existing cumulative dirty worktree. Do not create a partial commit that captures unrelated prior changes; any eventual commit must follow the repository Lore commit protocol.

### Task 7: Close Independent Review Coverage Gaps

**Files:**
- Modify: `src/decompiler/cfg/ssa/optimize.rs`
- Modify: `src/decompiler/cfg/ssa/builder.rs`
- Modify: `src/decompiler/analysis/method_contracts.rs`
- Modify: `src/decompiler/tests/core/analysis.rs`

- [x] **Step 1: Prove the original optimizer fixture performed no rewrite**

Require `optimize` to return a positive round count and observe the test fail with the original `newmap`-only fixture.

- [x] **Step 2: Add ambient sentinels and observe the old expectations fail**

Seed integer `9` below each builder and type-analysis mutation fixture. Preserve the previous bare/unknown expectations for the red run and require failures showing `Return(9)` and `ValueType::Integer`.

- [x] **Step 3: Exercise optimizer substitution and reindexing**

Add a constant key definition consumed by `set_item`. Assert at least one rewrite round, removal of the constant definition, literal propagation into the effect call, preservation of the `newmap` definition, and the rebuilt use site at the effect's shifted index.

- [x] **Step 4: Lock exact stack consumption and malformed arity**

Assert every mutation preserves ambient `9`. Expand partial-underflow coverage to all six operations, including five-argument `MEMCPY`, with unknowns in every missing leading operand position.

- [x] **Step 5: Cover the shared method-contract consumer**

Analyze a private `MEMCPY; RET` helper through `infer_method_contracts` and assert `argument_count == 5` plus `ReturnBehavior::Void`. Temporarily restore the old three-pop estimator, require this test to fail, then restore the five-pop fix.

- [x] **Step 6: Run focused and repository verification**

Run the four focused regressions, SSA tests, method-contract tests, core-analysis tests, formatting, Clippy, and the full repository test fence.
