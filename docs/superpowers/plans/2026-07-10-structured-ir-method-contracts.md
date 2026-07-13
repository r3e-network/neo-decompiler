# Structured IR Method Contracts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make structured IR use manifest parameter names/types and precise resolved-call arity/return metadata instead of renderer-only signatures and opaque stack barriers.

**Architecture:** Keep manifest and call-graph interpretation in the per-method render boundary. Pass a small SSA method context containing source parameter names, the current return contract, and call-site contracts into the existing `SsaBuilder`; unknown calls continue through the conservative fallback. Seed incoming `argN` slots as real SSA values and carry their source display names into lowering, preserving optimizer slot identity while avoiding rendered-string rewriting. Represent void calls explicitly as SSA expression statements.

**Tech Stack:** Rust, existing Neo VM CFG/SSA/IR modules, `ContractManifest`, `CallGraph`, built-in unit and integration tests. No new dependencies.

---

### Task 1: Lock Source Parameter Behavior

**Files:**
- Modify: `src/decompiler/cfg/method_view.rs`
- Modify: `src/decompiler/cfg/ssa/form.rs`
- Modify: `src/decompiler/cfg/ssa/builder.rs`
- Modify: `src/decompiler/cfg/ssa/optimize.rs`
- Modify: `src/decompiler/cfg/ssa/to_ir.rs`
- Modify: `src/decompiler/cfg/structure.rs`
- Test: `tests/ir_pipeline.rs`

- [x] **Step 1: Add a failing end-to-end parameter test**

Build a real NEF method containing `INITSLOT 0,2; LDARG1; RET` and a manifest declaring `choose(from: Hash160, amount: Integer) -> Integer`. Assert that structured IR contains:

```text
fn choose(from: hash160, amount: int) -> int {
return amount;
```

Also assert that the body contains neither `ldarg1()` nor `arg1_`.

- [x] **Step 2: Verify the test fails for the body/signature mismatch**

Run:

```bash
cargo test --test ir_pipeline structured_ir_uses_manifest_parameter_names_in_signature_and_body -- --nocapture
```

Expected: failure because the body signature currently renders `fn choose()`, and the argument load is not a source parameter expression.

- [x] **Step 3: Seed source parameters as incoming SSA argument slots**

`SsaBuilder` receives sanitized argument names, seeds entry slot `argN` with version zero, and reserves version one for subsequent definitions. `MethodContext` derives the `argN -> source_name` map and passes it only to structured lowering, so every version of a declared argument renders with the source name while locals and temps keep their analysis-facing SSA suffixes. Keeping this metadata out of public `SsaForm` preserves source compatibility for external struct-literal users.

- [x] **Step 4: Render manifest parameters on method definitions**

Reuse `format_manifest_parameters` and `sanitize_parameter_names` in `method_view.rs`. Configure the builder with the same sanitized names used in the signature so declaration and body identifiers cannot diverge.

- [x] **Step 5: Verify the focused parameter test passes**

Run the command from Step 2 and require exit `0`.

### Task 2: Model Resolved Call Contracts in SSA

**Files:**
- Create: `src/decompiler/cfg/ssa/context.rs`
- Modify: `src/decompiler/cfg/ssa/mod.rs`
- Modify: `src/decompiler/cfg/ssa/form.rs`
- Modify: `src/decompiler/cfg/ssa/builder.rs`
- Modify: `src/decompiler/cfg/ssa/optimize.rs`
- Modify: `src/decompiler/cfg/ssa/to_ir.rs`
- Modify: `src/decompiler/cfg/structure.rs`
- Test: `src/decompiler/cfg/ssa/builder.rs`

- [x] **Step 1: Add failing SSA tests for value and void calls**

Configure call-site contracts keyed by instruction offset:

```rust
CallContract::new("helper", 2, true)
CallContract::new("notify", 1, false)
```

Assert a known value call pops arguments in Neo calling order and pushes only its result. Assert a known void call emits a side-effect statement, pushes no result, and leaves a later return value intact.

- [x] **Step 2: Verify both tests fail because all calls use the opaque result path**

Run:

```bash
cargo test --lib known_call_contract -- --nocapture
```

Expected: failure because arguments are discarded, names remain offset placeholders, and void calls still manufacture a result.

- [x] **Step 3: Introduce the bounded SSA context**

Define:

```rust
pub(crate) struct CallContract {
    pub(crate) name: String,
    pub(crate) argument_count: usize,
    pub(crate) returns_value: bool,
}

#[derive(Default)]
pub(crate) struct MethodContext {
    pub(crate) argument_names: Vec<String>,
    pub(crate) returns_value: Option<bool>,
    pub(crate) calls_by_offset: BTreeMap<usize, CallContract>,
}
```

Replace the builder's standalone return flag with an optional borrowed `MethodContext`. Preserve each `MethodView`'s exact `manifest_index_for_start` so overloaded or offsetless ABI methods are not selected by sanitized name alone.

- [x] **Step 4: Add explicit SSA expression statements**

Extend `SsaStmt` with:

```rust
Expr(SsaExpr)
```

Update display, optimization/use indexes, lowering, and structuring so a known void call becomes `helper(args);` without a fake assignment.

- [x] **Step 5: Implement precise known calls and preserve the fallback**

For a call-site contract, pop the `CALLA` pointer when applicable, pop exactly `argument_count` values top-first into source argument order, preserve underflow as `?`, and emit the friendly name. Push an SSA result only when `returns_value` is true. When no contract exists, keep `apply_opaque_call` unchanged.

- [x] **Step 6: Verify the focused SSA tests pass**

Run the command from Step 2 and require exit `0`.

### Task 3: Build Contracts from Existing Analysis

**Files:**
- Modify: `src/decompiler/cfg/method_view.rs`
- Modify: `src/decompiler/decompilation.rs`
- Test: `tests/ir_pipeline.rs`

- [x] **Step 1: Add failing full-pipeline internal-call tests**

Use a two-method synthetic NEF and manifest. Assert:

```text
helper(1, 2)
helper(1);
return 9;
```

The value test must not contain `call_0x`; the void test must not assign or return the void call.

- [x] **Step 2: Verify the integration tests fail on the opaque-call output**

Run:

```bash
cargo test --test ir_pipeline structured_ir_uses_resolved_internal_call_contract -- --nocapture
cargo test --test ir_pipeline structured_ir_keeps_resolved_void_call_as_statement -- --nocapture
```

- [x] **Step 3: Build call-site contracts from `CallGraph` and existing metadata**

Pass `Decompilation::call_graph` into `render_envelope`. For each resolved internal edge, use the `MethodRef` name plus manifest/inferred argument count and manifest or uniform explicit-return behavior. For method-token edges, use the token method, parameter count, and return flag already present on `CallTarget::MethodToken`. Leave indirect and unresolved edges absent so they take the opaque fallback.

- [x] **Step 4: Configure each method builder with one consistent context**

The current method's manifest supplies parameter names and return behavior; the contract-wide call map is shared across method views. Do not parse manifests inside `SsaBuilder`.

- [x] **Step 5: Verify all three new integration behaviors pass**

Run the focused parameter, value-call, and void-call tests together and require exit `0`.

### Task 4: Cleanup and Verification

**Files:**
- Review only the files changed by Tasks 1-3.

- [x] **Step 1: Dead-code and duplication pass**

Remove the superseded `with_method_returns_value` flag path and consolidate manifest method lookup in `method_view.rs`. Do not refactor unrelated legacy renderers.

- [x] **Step 2: Naming/error-handling pass**

Confirm malformed/unknown calls still preserve the conservative barrier and missing arguments render as `?` rather than dropping the call.

- [x] **Step 3: Run focused verification**

```bash
cargo test --lib decompiler::cfg::ssa --all-features
cargo test --test ir_pipeline --all-features
```

- [x] **Step 4: Run repository gates**

```bash
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt -- --check
npm test --prefix js
npm test --prefix web
cargo deny check
git diff --check
```

- [x] **Step 5: Review the final diff**

Require no unrelated edits, no new dependency, unchanged legacy defaults, and explicit reporting of the remaining unknown-call/type-inference limitations.

Review hardening completed in this tranche:

- [x] Preserve terminator-only SSA uses so branch conditions and adjacent void calls survive optimization.
- [x] Associate ABI metadata only by retained manifest index or exact offset, never sanitized name alone.
- [x] Disambiguate generated SSA display names from manifest parameter names.
- [x] Remove pure `PUSHA` materialization from resolved `CALLA` output.
- [x] Emit nontrivial stack phis and preserve `?` on top-aligned unequal-height merges.
- [x] Apply the same inferred arity and uniform return contract to private helper calls and definitions.
- [x] Infer private return contracts to a fixed point with resolved nested-call metadata, including entry-stack arguments that remain live across void calls.
- [x] Seed inferred no-`INITSLOT` parameters on the evaluation stack in VM order and retain a virtual method-entry source for entry loops.
- [x] Use one collision-safe offset-to-label map for structured method definitions and resolved internal calls.
- [x] Sanitize method-token call identifiers and escape control characters in token summary comments without mutating raw analysis metadata.
- [x] Preserve manifest arity for offsetless entry methods instead of overwriting it with bytecode inference.
- [x] Lower `PUSHA` to its checked absolute target offset, with unresolved arithmetic retaining the conservative fallback.

**Release constraint:** `SsaStmt` is publicly re-exported and gains `Expr` and `Return` variants in this tranche. It is now marked `#[non_exhaustive]`; publish these variants in the next breaking-compatible 0.x minor release rather than as a patch release.

**Remaining limitations:** unresolved and indirect calls still use the conservative opaque stack barrier, recursive/ambiguous private return SCCs retain the value-returning fallback, and source-level type inference remains intentionally bounded by available manifest and opcode metadata.
