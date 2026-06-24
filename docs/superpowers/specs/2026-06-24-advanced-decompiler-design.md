# Neo N3 Decompiler — Advanced Decompiler Evolution

**Date:** 2026-06-24
**Status:** Proposed
**Risk posture:** Major rewrite permitted; 481-test suite is the regression fence throughout.

## 1. Problem

`neo-decompiler` is mature (v0.8.2, 33k LOC, 481 tests, full opcode coverage,
real type inference, dominance analysis, a typed IR module, and an SSA module).
But the pieces are **disconnected**, so it stops short of being an "advanced"
decompiler:

- The high-level/C# renderers **build strings directly** from a stack-machine
  emitter. Structural recovery (while/for/switch/try/else-if) is done by
  **~24 string-pattern postprocess passes** over rendered lines — fragile and
  the project's own biggest maintenance risk.
- The typed **IR (`decompiler::ir`)** is dormant: `Stmt::Unlifted` is never
  constructed; only the SSA skeleton references it.
- **SSA is skeleton-only**: `SsaBuilder` only versions `PUSH0..16`; no φ nodes
  are placed despite a `PhiNode` data model and dominance frontiers existing.
  (`cfg/ssa/builder.rs:184-292`)
- **Type inference is computed but never consumed** by any renderer — output
  keeps emitting `loc0`/`arg0`, discarding the inferred `ValueType`.
- C# and high-level **duplicate** the method-label/call-target orchestrators
  (`csharp/render.rs:115-211` vs `high_level/render.rs:130-275`), and C# output
  runs through a 1163-line string rewriter (`csharp/helpers.rs:63`).

These four gaps are exactly the README's "Planned Future Work" (SSA
optimizations, data-flow analysis) and the defining traits of advanced
decompilers (Hex-Rays/Ghidra/SmartDec; EVM: Panoramix/EtherSolve): a **typed IR
on SSA is the spine**; analyses operate on the IR; emission is *from* the IR.

## 2. Goal

Make SSA + typed IR the spine of the decompiler, wire the existing analyses
into output, eliminate string-based structural recovery, and fix real bugs —
**incrementally**, so the suite stays green and every phase ships value.

Non-goals (README "Out of Scope" still hold): full source-level type
reconstruction, ML variable naming, source debugging, contract patching.

## 3. Target architecture

```
 NEF bytes
   │  parse + disassemble
   ▼
 Vec<Instruction>
   │  CFG build (exists)
   ▼
 Cfg  ──────────────► dominance (exists) ──► SSA φ-placement (NEW, Cytron)
   │                                                │
   │  lift → ir::Block (NEW lifter, reuses emitter   │
   │  stack-effect tables as the source of truth)    │
   ▼                                                 ▼
 ir::Block/Stmt/Expr  ◄──── SSA renaming ◄──── φ nodes
   │
   │  SSA optimizations (NEW): const fold, copy/const prop, DCE, φ-simplify
   │  type annotation (NEW): stamp ValueType from infer_types onto locals/args/temps
   │  structural recovery (NEW): CFG → ir::ControlFlow (while/for/switch/try)
   ▼
 optimized typed ir::Block
   │  render (NEW, shared)
   ▼
 high-level text   /   C# source
```

Key invariant: **one shared lift → one shared IR → one shared optimizer → one
shared renderer**. The pseudo-code and C# front-ends differ only in a final
dialect pass (type names, attribute syntax), not in lifting or control-flow
recovery. The legacy string emitter survives behind a flag until the IR path
reaches parity, then is removed.

## 4. Phased plan

Each phase: green suite, new tests, the suite as regression fence. Later phases
depend on earlier ones but each is independently useful and mergeable.

### Phase 0 — Correctness baseline & cheap wins
- Decompile every `TestingArtifacts/*` contract; triage and **fix real bugs**
  (malformed output, wrong control flow, panics on edge inputs).
- Fuzz-informed hardening: feed `fuzz/` corpora through the full pipeline;
  patch any panic.
- Perf: profile the postprocess passes; apply safe O(n²)→O(n) wins (e.g. the
  repeated full-`Vec` scans in `simplify.rs` / `while_loops.rs`).
- Add a repo-wide **parity/regression test** that pins all-artifact decompilation
  output, so later phases detect regressions immediately.

### Phase 1 — Wire type inference into renderers (additive)
- Thread `&TypeInfo` into `render_high_level` and `render_csharp` (both already
  receive the analysis bundle's siblings).
- Add `ValueType → pseudo type string` and `ValueType → C# type string` maps
  (e.g. `Integer → "int"`/`"BigInteger"`, `ByteArray/Buffer → "byte[]"`,
  `Map → "Map"`, `Array → "object[]"`, `Boolean → "bool"`).
- Emit typed declarations for locals/args/statics when a non-`Unknown` type is
  known; leave `loc0` fallback for unknown. Manifest parameter names already
  flow in; combine type + name.
- New tests per `ValueType`. Existing tests untouched where they don't assert
  declarations.

### Phase 2 — Real SSA over the instruction stream
- Introduce a **stack-effect model**: for every opcode, (pop N, push M), as
  already encoded in the emitter dispatch (`emitter/dispatch/*`). Make this the
  single source of truth and consume it from SSA.
- Rewrite `SsaBuilder::process_instruction_for_ssa` to pop N SSA value-uses and
  push M SSA value-defs (generalized, not PUSH-only).
- Implement **Cytron φ placement** using the existing `dominance::compute`
  frontiers, then **renaming** (the `versions` map generalizes).
- Populate `definitions` / `uses` / `PhiNode` for real. `SsaForm` becomes a
  genuine data-flow SSA.
- Tests: φ placement on diamond CFGs, def/use correctness on arithmetic
  sequences, dominance-frontier-driven placement.

### Phase 3 — SSA-based optimizations
- Over `SsaForm`: **constant folding/propagation**, **copy propagation**,
  **dead-store/code elimination**, **trivial-φ elimination**.
- Expose via `Decompiler::with_ssa_optimizations(bool)` (default on once stable).
- Feed optimized SSA into Phase 4 lifting so dead temps/computed constants
  don't surface in output.
- Differential tests: before/after on the artifact corpus; assert no spurious
  `goto`/temp regressions and that pure-computation chains collapse.

### Phase 4 — IR spine: lift → optimize → recover → render
- **New lifter:** instruction stream → `ir::Block` (per method), reusing the
  emitter's provenance tables (call labels, method offsets, pointer literals)
  but producing `ir::Stmt`/`ir::Expr` instead of `String`.
- **Structural recovery over CFG → `ir::ControlFlow`**: while/for/switch/
  try/catch/finally/else-if, replacing the string postprocess. Uses the CFG
  (not rendered text), so it is robust to formatting changes.
- **Type annotation:** stamp `ValueType` from Phase 1 onto IR vars.
- **Shared renderer** `ir::Block → String` with a small C# dialect shim. The
  1163-line `csharpize_statement` collapses to a type-name + attribute map.
- **Parallel rollout:** run IR path alongside legacy, gate behind
  `--ir`/`Decompiler::with_ir_pipeline`. Promote to default at parity; remove
  legacy emitter + string postprocess thereafter.

### Phase 5 — Maintainability
- Orchestrator duplication dissolves once both front-ends render from the IR.
- Split the god-object `HighLevelEmitter` (or retire it).
- Delete merged/dead code (skeleton SSA path, dormant `Stmt::Unlifted` sites,
  redundant pseudocode renderer if subsumed).

## 5. Risks & mitigations

| Risk | Mitigation |
|---|---|
| String postprocess has implicit ordering/invariants lost in AST rewrite | Phase 4 runs both paths in parallel; promote only at parity; full corpus diff gate |
| φ placement / renaming bugs corrupt output | Dedicated unit tests on synthetic CFGs; differential tests vs legacy |
| Type annotation changes golden output unexpectedly | Additive only in Phase 1; emit types only when known; keep `loc0` fallback |
| Scope creep / half-finished rewrite | Each phase is independently mergeable; legacy path remains default until Phase 4 parity |

## 6. Verification per phase
- `cargo test --all-features` green (current baseline: 481 passing).
- `cargo build --all-features` + `cargo build --release` clean.
- New unit/differential tests added per phase.
- `cargo clippy --all-features -- -D warnings` (honor `#![forbid(unsafe_code)]`).

## 7. Out of scope for this design
- Struct/class field recovery, deobfuscation, plugin/REPL, interactive mode —
  remain README "future/low priority"; revisit after Phase 4.
