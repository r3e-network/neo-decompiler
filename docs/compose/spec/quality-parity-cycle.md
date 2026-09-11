---
feature: quality-parity-cycle
status: complete
updated: 2026-09-09
branch: master
commits: d504f50b..HEAD
---

# Quality & Parity Cycle

## Report

## [S1] Problem

The product is mature (Rust 0.14.0 / JS 2.1.0; pinned 103-contract corpus;
recent security audit), but several user-visible quality and consistency gaps
remain:

1. **Stale roadmap claims.** README still lists IR switch/for recovery as
   future work and implies C# does not yet use the IR spine. Production C#
   bodies are already structured-IR-only, and switch/for recovery ships on
   that path. Operators and contributors are misled about remaining debt.
2. **C# type hop noise.** Buffer/ByteString `SETITEM` always renders
   `(byte)(dynamic)(value)` even when the value is a statically exact
   `BigInteger`, forcing an unnecessary dynamic binder hop that the sibling
   `NZ` lowering already avoids.
3. **JS analysis differential is incomplete.** Rust `decompile --format json`
   and the JSON schema both expose `analysis.method_contracts`, and the JS
   port implements `methodContracts`, but `js/test/differential.test.mjs`
   never compares them. Silent contract-inference drift is invisible to CI.
4. **Dual-pipeline debt is real but poorly scoped.** The remaining debt is
   the high-level string emitter + postprocess (`--format high-level` /
   JSON `high_level`) and the JS text→C# twin — not a second Rust C# path.
   Docs must say this clearly so future work targets the actual spine.

## [S2] Design

### [S2.1] Documentation truth

- README "Shipped Features" moves IR `switch`/`for` recovery out of Planned.
- Planned "IR-Spine Default Path" is rewritten to describe the real remaining
  work: retire/replace the high-level string postprocess and align the JS
  text→C# twin, not "promote C# to IR".
- Scope/limitations and Development sections state that production C# bodies
  use the structured IR pipeline only; high-level remains an analysis view.

### [S2.2] C# exact-type SETITEM byte store

In `structured/expr_intrinsics.rs` SETITEM for Buffer/ByteString receivers:

- When the stored value `is_statically_exact_csharp_type(..., "BigInteger")`,
  emit `(byte)(value)` (relying on BigInteger's explicit byte conversion).
- Otherwise keep `(byte)(dynamic)(value)` — fail-closed, unchanged.

Mirrors the existing `NZ` exact-BigInteger gate. No type invention beyond a
statically proven C# type.

### [S2.3] JS methodContracts differential

In `js/test/differential.test.mjs` analysis comparison, when the Rust binary
is available and `analysis.method_contracts` is present, assert
`stableJson(js.methodContracts) === stableJson(rust.analysis.method_contracts)`
alongside call_graph / xrefs / types / patterns. When either side is missing
the field, skip that assertion (environment-dependent corpus runs stay green).

### [S2.4] Out of scope this cycle

- Retiring `HighLevelEmitter` + `high_level/emitter/postprocess/*` (public
  JSON/CLI surface; needs a deprecation path and corpus parity).
- Porting SSA/IR structured C# to the independent JS implementation.
- Deleting test-only `legacy_statement_to_csharp` (large test-only suite;
  no production call path; deferred until high-level tests are restructured).
- Changing Integer/ByteString `switch` to constant `case N:` patterns
  (C# cannot traditional-switch on `BigInteger`; the `case var when` form is
  required for compile safety).
- Catch payload `dynamic`→`object` (needs corpus-wide dynamic-binder audit).
- Web API expansion, wasm size reduction, and CI fuzz wiring.

## [S3] Out of Scope

See [S2.4]. This cycle is documentation accuracy, one C# exact-type
lowering, and one missing differential assertion — not a dual-pipeline
retirement project.

## Tasks

- [x] T1: Update README roadmap/scope so IR switch/for and C# IR-spine status match reality — acceptance: Planned table no longer lists shipped switch/for; IR-spine item describes high-level/JS debt; scope text states C# bodies are structured-IR-only (covers: S2.1)
- [x] T2: Emit `(byte)(value)` for SETITEM byte stores when the value is statically exact BigInteger — acceptance: unit test proves exact-type path and non-exact path; existing collection tests still pass (covers: S2.2)
- [x] T3: Assert JS `methodContracts` equals Rust `analysis.method_contracts` in the differential suite — acceptance: analysis differential includes the field; `cd js && npm test` passes with the Rust binary present (covers: S2.3)
- [x] T4: Full verification — acceptance: `cargo test --locked`, `cargo clippy --locked --all-targets --all-features -- -D warnings`, `cd js && npm test` all pass (covers: S2.2; S2.3)

## Follow-up (same cycle, landed with T1–T4)

- Array/Struct collection mutations hop through `dynamic` instead of an
  invalid `List<object>` cast of `object[]`.
- C# structural differential (class name + public ABI methods) between the
  Rust IR path and the JS text→C# twin.
- LDSFLD first-read correctness: SSA pushes initial `staticN`; C# no longer
  renders static getters as `Runtime.LoadScript`.
- Structured IR empty-body rendering no longer inserts a blank line.
