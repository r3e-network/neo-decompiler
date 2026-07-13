# Shared Method Contract Analysis Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Compute one tri-state method contract analysis per decompilation and use it to preserve private void-call semantics in Rust structured IR, Rust high-level output, Rust C#, and JavaScript.

**Architecture:** Rust adds a serializable `analysis::method_contracts` result built from the existing method table, argument-count inference, per-method CFG/SSA builder, and call graph. The pipeline stores that result and every Rust renderer derives call-stack behavior from it. JavaScript mirrors the same monotone `unknown -> void` fixed point over stable analysis method groups and exposes the result through analysis and high-level APIs.

**Tech Stack:** Rust 2021, serde/serde_json, Neo VM CFG/SSA/typed IR, JavaScript ES modules, Node test runner, TypeScript declarations, JSON Schema.

**Workspace note:** This is a cumulative dirty worktree with overlapping user changes. Verification checkpoints are included, but commits are intentionally omitted so pre-existing edits are not bundled. Any later commit must use the repository's Lore protocol.

---

## File Map

- Create `src/decompiler/analysis/method_contracts.rs`: public types, lookup helpers, and Rust fixed-point inference.
- Modify `src/decompiler/analysis/mod.rs`, `src/decompiler/pipeline.rs`, and `src/decompiler/decompilation.rs`: export, compute, and store contracts once.
- Modify `src/decompiler/high_level/render.rs`, `src/decompiler/high_level/render/methods.rs`, `src/decompiler/csharp/render.rs`, and `src/decompiler/csharp/render/methods.rs`: consume shared arity/return behavior.
- Modify `src/decompiler/cfg/method_view.rs`: delete structured-only inference and adapt the shared result to SSA `CallContract`s.
- Modify `src/cli/reports/types.rs`, `src/cli/runner/decompile.rs`, `src/web/report.rs`, `docs/schema/decompile.schema.json`, `docs/schema/README.md`, and `web/src/index.ts`: expose contracts in reports and types.
- Create `js/src/method-contracts.js`: JavaScript fixed-point inference over stable analysis groups.
- Modify `js/src/index.js`, `js/src/high-level-state.js`, `js/src/high-level.js`, and `js/src/index.d.ts`: consume and expose JavaScript contracts.
- Modify focused Rust, CLI/web, JavaScript, and TypeScript tests named in the tasks below.

### Task 1: Lock the Rust analysis contract

**Files:**
- Create: `src/decompiler/analysis/method_contracts.rs`
- Modify: `src/decompiler/analysis/mod.rs`
- Test: `src/decompiler/analysis/method_contracts.rs`

- [ ] **Step 1: Export the module and write failing public-shape tests**

Add `pub mod method_contracts;` to `analysis/mod.rs`. Create the module with this public shape:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReturnBehavior {
    Value,
    Void,
    Unknown,
}

impl ReturnBehavior {
    #[must_use]
    pub const fn returns_value(self) -> bool {
        !matches!(self, Self::Void)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MethodContract {
    pub method: MethodRef,
    pub argument_count: usize,
    pub return_behavior: ReturnBehavior,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct MethodContracts {
    pub methods: Vec<MethodContract>,
}
```

Use the existing NEF/disassembly helpers and the critical script:

```rust
const PRIVATE_VOID: &[u8] = &[
    0x19, 0x11, 0x34, 0x05, 0x40, 0x21, 0x21,
    0x57, 0x00, 0x01, 0x78, 0x45, 0x40,
];
```

Declare only `main@0 -> Integer` and assert offset 7 is `argument_count: 1`, `return_behavior: Void`, and named `sub_0x0007`. Also assert `Unknown` serializes as `"unknown"`, offsets are sorted, and `MethodContracts::get(7)` works.

- [ ] **Step 2: Run the focused test and verify RED**

```bash
cargo test decompiler::analysis::method_contracts --all-features
```

Expected: failure because lookup/inference is not implemented.

- [ ] **Step 3: Implement deterministic seeding and lookup maps**

Implement binary-search lookup plus internal renderer adapters:

```rust
impl MethodContracts {
    #[must_use]
    pub fn get(&self, offset: usize) -> Option<&MethodContract> {
        self.methods
            .binary_search_by_key(&offset, |contract| contract.method.offset)
            .ok()
            .map(|index| &self.methods[index])
    }

    pub(crate) fn argument_counts_by_offset(&self) -> BTreeMap<usize, usize> {
        self.methods
            .iter()
            .map(|contract| (contract.method.offset, contract.argument_count))
            .collect()
    }

    pub(crate) fn returns_value_by_offset(&self) -> BTreeMap<usize, bool> {
        self.methods
            .iter()
            .map(|contract| {
                (
                    contract.method.offset,
                    contract.return_behavior.returns_value(),
                )
            })
            .collect()
    }
}
```

Seed every stable `MethodTable::methods()` entry. Resolve manifest methods by exact offset plus the existing entry fallback. Manifest parameters and return types are authoritative; private methods start `Unknown`. Reuse `build_method_arg_counts_by_offset` and sort by method offset before constructing the public collection.

- [ ] **Step 4: Implement the monotone Rust fixed point**

Extract per-method CFGs with `cfg::method_view::extract_method_cfgs`. Restrict inference candidates to undeclared `CallTarget::Internal` offsets; an undeclared script entry stays `Unknown` and preserves existing no-manifest renderer behavior. For each candidate, create `MethodContext` call-site contracts from the current result, mapping `Unknown` conservatively to `returns_value = true`. Build SSA and collect every `SsaStmt::Return`.

Only transition `Unknown -> Void` when there is at least one observed return and every observed return is bare:

```rust
let proven_void = !returns.is_empty() && returns.iter().all(Option::is_none);
```

Never infer private `Value`. Mixed/no-return methods, unresolved calls, and ungrounded recursive cycles remain `Unknown`; declared offsets are never reconsidered.

- [ ] **Step 5: Add convergence, ambiguity, and precedence tests**

Cover:

```text
main@0 -> wrapper@6 -> leaf@10: wrapper and leaf converge to void
private self-recursive helper: remains unknown
ABI helper declared Integer with a bare body: remains value
ABI helper declared Void with stack data at RET: remains void
```

Assert results contain no duplicate offsets and remain sorted.

- [ ] **Step 6: Verify GREEN**

```bash
cargo test decompiler::analysis::method_contracts --all-features
```

### Task 2: Make the Rust pipeline the single owner

**Files:**
- Modify: `src/decompiler/pipeline.rs`
- Modify: `src/decompiler/decompilation.rs`
- Modify: `src/decompiler/high_level/render.rs`
- Modify: `src/decompiler/high_level/render/methods.rs`
- Modify: `src/decompiler/csharp/render.rs`
- Modify: `src/decompiler/csharp/render/methods.rs`
- Test: `src/decompiler/tests/core/analysis.rs`
- Test: `src/decompiler/tests/csharp.rs`

- [ ] **Step 1: Write failing high-level and C# regressions**

Decompile the critical fixture with only `main@0 -> Integer`. Assert:

```rust
assert_eq!(dec.method_contracts.get(7).unwrap().return_behavior, ReturnBehavior::Void);
assert!(dec.high_level.as_deref().unwrap().contains("sub_0x0007(1);"));
assert!(dec.high_level.as_deref().unwrap().contains("return 9;"));
assert!(!dec.high_level.as_deref().unwrap().contains("return sub_0x0007(1)"));
assert!(dec.csharp.as_deref().unwrap().contains("private static void sub_0x0007"));
```

The C# test must also assert the call statement, `return 9;`, and one inferred helper parameter. Match the existing inferred parameter type rather than introducing source-type inference in this slice.

- [ ] **Step 2: Verify RED**

```bash
cargo test private_void --all-features
```

- [ ] **Step 3: Compute and store contracts once**

Add `pub method_contracts: MethodContracts` to `Decompilation`. Immediately after building the call graph in `decompile_bytes_with_manifest`, compute:

```rust
let method_contracts = infer_method_contracts(&instructions, manifest.as_ref(), &call_graph);
```

Pass `&method_contracts` to high-level and C# renderers, then move it into the result.

- [ ] **Step 4: Replace legacy renderer metadata assembly**

Change both renderer signatures to accept `&MethodContracts`. Replace manifest-only helper calls with:

```rust
let method_arg_counts_by_offset = method_contracts.argument_counts_by_offset();
let method_returns_value_by_offset = method_contracts.returns_value_by_offset();
```

Keep label normalization, CALLA targets, method tokens, warnings, and options unchanged. Render private `Unknown` as `-> any` / C# `dynamic`, and private `Void` without a Rust high-level return annotation / as C# `void`. ABI signatures remain authoritative.

- [ ] **Step 5: Verify focused pipeline/render tests**

```bash
cargo test private_void --all-features
cargo test decompiler::tests::csharp --all-features
cargo test decompiler::tests::core::analysis --all-features
```

### Task 3: Make structured IR consume the shared result

**Files:**
- Modify: `src/decompiler/decompilation.rs`
- Modify: `src/decompiler/cfg/method_view.rs`
- Test: `tests/ir_pipeline.rs`

- [ ] **Step 1: Extend structured tests to inspect stored contracts**

Before rendering in the existing private-void and wrapper-chain tests, assert the relevant offsets are `ReturnBehavior::Void`. Add a recursive/mixed ambiguity test asserting `Unknown` remains conservatively value-producing in structured output.

- [ ] **Step 2: Run the existing behavior lock**

```bash
cargo test --test ir_pipeline structured_ir_infers --all-features
```

- [ ] **Step 3: Delete structured-only inference**

Change `render_envelope` to accept `&MethodContracts`. Remove `build_inferred_method_contracts`, `infer_method_returns_value`, and its manifest-only return map. Build internal SSA `CallContract`s from `MethodContracts::get(offset)` and use `return_behavior.returns_value()`. Keep method-token contracts from NEF/call-graph metadata and indirect/unresolved calls opaque.

Pass each method's shared contract to `render_method_body`, using its argument count and tri-state behavior while preserving manifest parameter names/types.

- [ ] **Step 4: Pass stored contracts into structured rendering**

```rust
method_view::render_envelope(
    &self.nef,
    self.manifest.as_ref(),
    &views,
    &self.call_graph,
    &self.method_contracts,
)
```

- [ ] **Step 5: Verify the complete IR pipeline**

```bash
cargo test --test ir_pipeline --all-features
```

### Task 4: Expose Rust contracts in CLI, web, schema, and browser types

**Files:**
- Modify: `src/cli/reports/types.rs`
- Modify: `src/cli/runner/decompile.rs`
- Modify: `src/web/report.rs`
- Modify: `docs/schema/decompile.schema.json`
- Modify: `docs/schema/README.md`
- Modify: `web/src/index.ts`
- Test: `tests/cli_smoke/decompile.rs`
- Test: `tests/web_api.rs`

- [ ] **Step 1: Write failing CLI and web report assertions**

Extend the current JSON tests with:

```rust
assert!(value["analysis"]["method_contracts"]["methods"].is_array());
assert_eq!(
    value["analysis"]["method_contracts"]["methods"][0]["return_behavior"],
    Value::String("value".into()),
);
```

Add a private-void report case that finds the offset-7 entry and compares it to:

```json
{
  "method": { "offset": 7, "name": "sub_0x0007" },
  "argument_count": 1,
  "return_behavior": "void"
}
```

- [ ] **Step 2: Verify RED**

```bash
cargo test --test cli_smoke decompile_command_supports_json_format --all-features
cargo test --test web_api web_decompile_report_exposes_high_level_and_csharp_outputs --all-features
```

- [ ] **Step 3: Add report fields without recomputing analysis**

Add `method_contracts: MethodContracts` to both private `AnalysisReport` structs. Populate it from `decompilation.method_contracts.clone()`; report code must not run its own inference.

- [ ] **Step 4: Define the serialized schema**

Require `method_contracts` in `analysisReport` and add:

```json
"methodContracts": {
  "type": "object",
  "additionalProperties": false,
  "required": ["methods"],
  "properties": {
    "methods": {
      "type": "array",
      "items": { "$ref": "#/definitions/methodContract" }
    }
  }
},
"methodContract": {
  "type": "object",
  "additionalProperties": false,
  "required": ["method", "argument_count", "return_behavior"],
  "properties": {
    "method": { "$ref": "#/definitions/methodRef" },
    "argument_count": { "type": "integer", "minimum": 0 },
    "return_behavior": { "enum": ["value", "void", "unknown"] }
  }
}
```

Reuse the schema's existing `#/definitions/methodRef` definition for the contract's `method` property.

- [ ] **Step 5: Update browser types and schema docs**

Add to `web/src/index.ts`:

```ts
export type ReturnBehavior = "value" | "void" | "unknown";
export interface MethodContract {
  method: MethodRef;
  argument_count: number;
  return_behavior: ReturnBehavior;
}
export interface MethodContracts { methods: MethodContract[]; }
```

Add `method_contracts: MethodContracts` to web `AnalysisReport`. Update `docs/schema/README.md` to list method contracts beside call graph, xrefs, and types.

- [ ] **Step 6: Verify report, schema, and web surfaces**

```bash
cargo test --test cli_smoke --all-features
cargo test --test web_api --all-features
npm --prefix web test
npm --prefix web run build
```

### Task 5: Add JavaScript fixed-point analysis

**Files:**
- Create: `js/src/method-contracts.js`
- Modify: `js/src/index.js`
- Test: `js/test/decompiler.test.mjs`

- [ ] **Step 1: Write failing JavaScript analysis tests**

Reuse `buildNefFromScript` and the same fixtures as Rust. For the critical fixture, assert the public stable list is exactly offsets 0 and 7, excluding the presentation-only NOP group at offset 5:

```js
assert.deepEqual(result.methodContracts.methods, [
  {
    method: { offset: 0, name: "main" },
    argumentCount: 0,
    returnBehavior: "value",
  },
  {
    method: { offset: 7, name: "sub_0x0007" },
    argumentCount: 1,
    returnBehavior: "void",
  },
]);
```

Also cover wrapper convergence (`main@0 -> wrapper@4 -> leaf@7`), a self-recursive helper that remains `unknown`, and ABI value/void overrides.

- [ ] **Step 2: Verify RED**

```bash
cd js
node --test --test-name-pattern='method contract|private void|wrapper' test/decompiler.test.mjs
```

- [ ] **Step 3: Implement contract seeds and internal maps**

Create `inferMethodContracts(methodGroups, manifest, context)` returning:

```js
{
  methods,                    // sorted public array
  byOffset,                   // internal contract Map
  argumentCountsByOffset,
  returnsValueByOffset,
}
```

Each public entry has only `method`, `argumentCount`, and `returnBehavior`. Manifest methods seed `value`/`void`; private methods seed `unknown`. The boolean map treats `unknown` as value-producing. Use the existing high-level entry-stack argument-count inference, not the call-graph module's narrower arity helper.

- [ ] **Step 4: Implement monotone lifting**

For each unknown stable analysis group, call `liftMethodBody(group.instructions, null, evolvingContext, group.start)`. Classify rendered returns with:

```js
const returns = result.statements
  .flatMap((statement) => statement.split("\n"))
  .map((line) => line.trim())
  .filter((line) => /^return(?:\s+.+)?;$/.test(line));
const provenVoid = returns.length > 0 && returns.every((line) => line === "return;");
```

On each `unknown -> void` transition, update the evolving boolean map and repeat. Never transition a private contract to `value`.

- [ ] **Step 5: Compute once for analysis and high-level calls**

Refactor context construction into stable base metadata followed by contract inference. `analyzeBytes` must infer over `analysisGroups` (`includePostTerminatorTails: false`). High-level APIs may render presentation groups, but must use and expose contracts inferred from the matching stable analysis groups.

- [ ] **Step 6: Verify focused JavaScript analysis**

```bash
cd js
node --test --test-name-pattern='method contract|private void|wrapper' test/decompiler.test.mjs
```

### Task 6: Make JavaScript lifting consume and expose contracts

**Files:**
- Modify: `js/src/high-level-state.js`
- Modify: `js/src/high-level.js`
- Modify: `js/src/index.js`
- Modify: `js/src/index.d.ts`
- Modify: `js/README.md`
- Test: `js/test/decompiler.test.mjs`

- [ ] **Step 1: Write the failing ambient-value regression**

Assert the critical fixture's high-level result contains `sub_0x0007(1);` and `return 9;`, excludes `return sub_0x0007(1)`, and exposes the same `methodContracts` as `analyzeBytes`.

- [ ] **Step 2: Make a method's own `RET` consult its contract**

In `createState`, calculate:

```js
const inferred = context?.methodContractsByOffset?.get(methodOffset)?.returnBehavior;
const returnsVoid =
  manifestMethod?.returnType?.toLowerCase() === "void" || inferred === "void";
```

Preserve the value through `cloneState`, add an empty map in `emptyContext`, and leave `high-level-calls.js` unchanged because it already consumes `methodReturnsValueByOffset` conservatively.

- [ ] **Step 3: Render inferred signatures from tri-state behavior**

When no manifest return type exists, render `unknown` as `fn name(args) -> any` and `void` as `fn name(args)`. This is renderer fallback behavior only; the public classification remains `unknown`.

- [ ] **Step 4: Update JavaScript declarations and README**

Add:

```ts
export type ReturnBehavior = "value" | "void" | "unknown";
export interface MethodContract {
  method: MethodRef;
  argumentCount: number;
  returnBehavior: ReturnBehavior;
}
export interface MethodContracts { methods: MethodContract[]; }
```

Add `methodContracts: MethodContracts` to `HighLevelResult` and `AnalyzeResult`; manifest high-level results inherit it. Document the result field in `js/README.md`.

- [ ] **Step 5: Verify all JavaScript behavior and declarations**

```bash
cd js
npm test
tsc --noEmit --strict --target ES2022 --module NodeNext --moduleResolution NodeNext src/index.d.ts
```

### Task 7: Completion verification and cleanup review

**Files:**
- Review every file listed above.

- [ ] **Step 1: Prove the critical fixture on every surface**

Focused tests must jointly prove: helper call is a statement; caller returns ambient 9; helper contract is void with one argument; C# defines `private static void`; CLI/web/JS APIs expose the deterministic entry.

- [ ] **Step 2: Check formatting and accidental changes**

```bash
cargo fmt --all -- --check
npm --prefix js run format:check --if-present
git diff --check HEAD
```

Review `git diff --stat` and preserve the pre-existing `.gitignore` edit.

- [ ] **Step 3: Run Rust gates**

```bash
cargo test --all-features
cargo clippy --all-features --all-targets -- -D warnings
cargo deny check
```

`cargo deny check` may retain the three known unmatched-license warnings but must exit successfully.

- [ ] **Step 4: Run JavaScript and web gates**

```bash
npm --prefix js test
npm --prefix web test
npm --prefix web run build
```

- [ ] **Step 5: Audit duplicate semantics**

```bash
rg -n 'build_inferred_method_contracts|infer_method_returns_value|build_method_returns_value_by_offset' src js
rg -n 'method_contracts|methodContracts' src tests docs/schema web js
```

The structured renderer-local inference must be gone. Remove the manifest return helper if it has no non-renderer consumer. Every renderer must consume the shared result rather than recompute it.

- [ ] **Step 6: Record residual risk honestly**

The final handoff must state that `Unknown` deliberately remains value-producing, Rust and JavaScript have equivalent native analyses rather than a cross-runtime implementation, and collection mutations plus SSA destruction remain later structured-IR slices. Do not claim that C# has migrated to structured IR.
