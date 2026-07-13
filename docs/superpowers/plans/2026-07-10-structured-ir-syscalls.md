# Structured IR Syscall Lifting Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Preserve known and unknown Neo syscalls as readable, stack-correct expressions in the structured SSA/IR output.

**Architecture:** Keep syscall metadata resolution in `SsaBuilder`, where VM stack state is available. Lower syscalls to the existing `SsaExpr::Call("syscall", ...)` representation so optimization, typed IR lowering, structuring, and rendering remain shared; known hashes use exact catalog contracts and unknown hashes use an opaque stack barrier.

**Tech Stack:** Rust, existing generated syscall catalog, CFG/SSA/typed IR modules, existing integration tests. No new dependencies or public IR variants.

---

### Task 1: Lock Known Syscall SSA Semantics

**Files:**
- Modify: `src/decompiler/cfg/ssa/builder.rs`
- Modify: `tests/ir_pipeline.rs`

- [x] **Step 1: Add failing value and void syscall tests**

Add builder tests using `Operand::Syscall` for `System.Runtime.CheckWitness` (`0x8CEC27F8`) and `System.Runtime.Log` (`0x9647E7CF`). Assert the value call has this SSA shape after optimization:

```rust
SsaExpr::Call {
    name,
    args,
} if name == "syscall"
    && args.as_slice() == [
        SsaExpr::lit(Literal::String("System.Runtime.CheckWitness".to_string())),
        SsaExpr::lit(Literal::Int(1)),
    ]
```

For the void call, build `PUSH9; PUSH1; SYSCALL Log; RET` and assert an `SsaStmt::Expr` contains the name literal and argument, while the final return remains `9`.

- [x] **Step 2: Verify the known-syscall tests fail for the current invisible special path**

Run:

```bash
cargo test --lib structured_known_syscall -- --nocapture
```

Expected: both tests fail because `apply_special` currently records stack effects but emits no syscall expression or statement.

- [x] **Step 3: Add failing malformed and unknown syscall tests**

Add one known-`Log` test with an empty stack and assert the rendered call retains `SsaExpr::var(unknown_var())` as its missing argument. Add one unknown-hash test using `PUSH9; SYSCALL 0xDEADBEEF; DROP; RET`; assert the syscall call remains visible and the return is bare, proving the opaque barrier prevents `9` from resurfacing.

- [x] **Step 4: Verify the fallback tests fail for the current anonymous unknown value**

Run:

```bash
cargo test --lib structured_syscall_fallback -- --nocapture
```

Expected: failure because known underflow is not represented in a call and the unknown hash only pushes `?`.

- [x] **Step 5: Add failing end-to-end value and void syscall tests**

Build `PUSH1; SYSCALL CheckWitness; RET` with a manifest `main() -> Boolean` and assert structured IR contains `syscall("System.Runtime.CheckWitness", 1)` rather than `return ?;`. Build `PUSH9; PUSH1; SYSCALL Log; RET` with a manifest `main() -> Integer`; assert the syscall appears as a statement, `return 9;` remains, and no assignment targets the void syscall. Name both tests with the `structured_ir_renders_known_syscall` prefix.

- [x] **Step 6: Verify both integration tests fail before implementation**

Run:

```bash
cargo test --test ir_pipeline structured_ir_renders_known_syscall -- --nocapture
```

Expected: both tests fail because the current structured builder drops syscall expressions.

### Task 2: Emit Syscalls Through Existing SSA Calls

**Files:**
- Modify: `src/decompiler/cfg/ssa/builder.rs`

- [x] **Step 1: Thread statements and versions into special handling**

Change `apply_special` to receive `stmts: &mut Vec<SsaStmt>` and `versions: &mut BTreeMap<String, usize>`, then dispatch `OpCode::Syscall` to a dedicated `apply_syscall` method.

- [x] **Step 2: Implement the known-contract path**

Resolve `crate::syscalls::lookup(hash)`, build the selector with:

```rust
SsaExpr::lit(Literal::String(info.name.to_string()))
```

Pop exactly `info.param_count` arguments top-first without reversing, record non-unknown uses at the current statement index, and append them after the selector. Emit an assignment plus stack result when `info.returns_value` is true; otherwise emit `SsaStmt::expr(call)`.

- [x] **Step 3: Implement the opaque fallback**

For an unknown hash, clear the tracked stack and emit a value call with selector `Literal::String(format!("0x{hash:08X}"))`. For a missing operand, use `Literal::String("unknown".to_string())`. In both cases push only the fresh call result.

- [x] **Step 4: Delete the superseded stack-effect helper**

Remove `syscall_effect`; exact known behavior and the unknown barrier now live together in `apply_syscall`.

- [x] **Step 5: Verify all four builder behaviors pass**

Run:

```bash
cargo test --lib structured_known_syscall -- --nocapture
cargo test --lib structured_syscall_fallback -- --nocapture
```

Require exit `0` and no warnings.

### Task 3: Verify End-to-End Structured Output

**Files:**
- Test: `tests/ir_pipeline.rs`

- [x] **Step 1: Verify both integration tests pass after Task 2**

Run:

```bash
cargo test --test ir_pipeline structured_ir_renders_known_syscall -- --nocapture
```

Require both tests to pass with readable named calls, exact argument order, no `return ?;`, and no assignment for the void syscall.

### Task 4: Review and Verification

**Files:**
- Review: `src/decompiler/cfg/ssa/builder.rs`
- Review: `tests/ir_pipeline.rs`

- [x] **Step 1: Focused verification**

```bash
cargo test --lib decompiler::cfg::ssa --all-features
cargo test --test ir_pipeline --all-features
cargo fmt -- --check
git diff --check HEAD
```

- [x] **Step 2: Review attempts and requirement audit**

The local Claude and Gemini advisors were both invoked on the narrow syscall diff, but authentication/client eligibility failed before either reviewed code. Their failure artifacts are retained in `.omx/artifacts/`. A line-by-line requirement audit confirmed that argument order matches Neo Cdecl pop order, void calls preserve ambient stack state, unknown hashes cannot expose pre-call values, and no public API or generated metadata changed. The missing external verdict remains a process limitation, not hidden evidence.

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

The JavaScript suite is a parity fence even though this slice changes only the Rust structured path.

**Workspace constraint:** this plan continues an existing cumulative dirty worktree. Do not create a partial commit that captures unrelated prior changes; any eventual commit must follow the repository Lore commit protocol.
