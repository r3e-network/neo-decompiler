# Architecture & Code Simplification — Quick Wins

**Date:** 2026-06-27
**Status:** Approved
**Scope:** Pure deletions, zero behavior change. Five atomic commits.

## Goal

Reduce over-complex code and architecture in `neo-decompiler` without changing any
public API or any output. Target: dead code, stale annotations, and speculative
exports left over from prior phases. Bigger structural changes (legacy emitter
retirement, method-table consolidation, MethodTable caching) are explicitly
out of scope — each is its own design doc.

## Non-goals

- **No public API change.** `OutputFormat`, `NefParser`, CLI flags, renderers,
  test files — all stay identical.
- **No output byte change.** Every deletion must produce byte-identical output
  to the pre-change code on the full test + corpus suite.
- **No legacy emitter retirement.** `HighLevelEmitter` and `postprocess/*`
  (~8.3 K LOC) are gated on the corpus-parity milestone per the
  `2026-06-24-codebase-review.md` roadmap. Out of scope here.
- **No method-table consolidation.** The IR path's `MethodTable` and the legacy
  `helpers::methods::inferred_method_starts` will be merged in a later doc.
- **No data-driven refactors.** `csharpize_statement_untyped`'s if-let cascade,
  `slot_name_for`'s 36-arm match, etc. are each their own refactor.
- **No `MethodTable` caching across the pipeline.** Out of scope.

## The five quick wins

Each is an atomic commit. Order chosen so that later commits don't depend on
earlier ones (no compile breaks during the sequence).

### W1 — Strip stale `#[allow(dead_code)]` from `method_view.rs`

**File:** `src/decompiler/cfg/method_view.rs`

**What:** Remove the six `#[allow(dead_code)]` annotations on lines 14, 29, 49,
117, 133, 193 plus their `// wired up by Decompilation::render_structured_ir
(Task 5/6)` comments.

**Why:** Every annotated item is wired up. `extract_method_cfgs` is called from
`decompilation.rs:169`; `render_envelope` from `decompilation.rs:175`;
`render_method_body` from `render_envelope:203`; `extract_one` from
`extract_method_cfgs:42`; `rewrite_terminator` from `extract_one:78`; the
`MethodView` fields are read by `render_method_body:141` and `:136`. The
annotations were defensive scaffolding during Task 5/6 staging, never removed.

**LOC:** -12 (annotations + comments).
**Risk:** None — removing them is safe. If a compile error appears, it surfaces
an actual dead-code bug worth fixing.
**Verification:** `cargo build --tests` compiles; all 521 tests pass.

### W2 — Delete dead SSA-convert module

**Files:** `src/decompiler/cfg/ssa/convert.rs` (entire file),
`src/decompiler/cfg/ssa/mod.rs:17` (the two-line `pub use convert::{...};`).

**What:** Delete `convert.rs` and remove its re-export.

**Why:** `expr_to_ssa` and `stmt_to_ssa` have no callers in the source tree.
The SSA form is built from `cfg + instructions` via `SsaBuilder` (not from IR
`Expr`/`Stmt`). The reverse direction (`SsaExpr` → `Expr`) lives in
`ssa/to_ir.rs` and is wired into `structure.rs`. The export is speculative
dead code from a planned "lift IR → SSA" that never landed.

**LOC:** -110 (whole file) + -2 (re-export line).
**Risk:** None — rustc proves no callers.
**Verification:** Full gate.

### W3 — Delete speculative `inferred_type_to_pseudo`

**Files:** `src/decompiler/helpers/types.rs:12-28` (function),
`src/decompiler/helpers/types.rs:100-111` (its tests), `src/decompiler/helpers.rs:21`
(its line in the re-export).

**What:** Delete the function, its two tests, and its re-export line.

**Why:** The function's `#[allow(dead_code)]` comment says "wired up in the
Phase-4 AST-based high-level renderer". That renderer does not exist. The IR
renderer renders typed declarations through
`csharp::helpers::inferred_type_to_csharp`, not this function. Same mapping is
recoverable from the C# variant when needed.

**LOC:** -30 (function + tests + re-export line).
**Risk:** None — rustc proves no callers. Dropping it also unblocks W5
(removes the `unused_imports` allow's justification).
**Verification:** Full gate.

### W4 — Delete dead `Stmt` variants

**Files:** `src/decompiler/ir/statement.rs:19-30` (the `VarDecl`, `Throw`,
`Break`, `Continue` variants), `src/decompiler/ir/render/stmt/mod.rs` (their
render arms).

**What:** Remove the four variants from the `Stmt` enum and the render arms
that consume them. Construction audit: only `Assign`, `Return`, `ExprStmt`,
`Comment`, `ControlFlow` are constructed anywhere in `src/`. The structurer
(`cfg/structure.rs`) doesn't emit any of `VarDecl`/`Throw`/`Break`/`Continue`.

**Why:** Speculative IR nodes from a planned future structurer pass that hasn't
landed. The render arms are untested dead code.

**LOC:** -40 (variants + arms).
**Risk:** None — no construction sites, no test references.
**Verification:** Full gate.

### W5 — Tighten `Decompilation.ssa` visibility + clean `helpers.rs` re-export

**Files:** `src/decompiler/decompilation.rs:38` (`pub ssa: Option<SsaForm>` →
`pub(crate) ssa: Option<SsaForm>`), `src/decompiler/helpers.rs:21` (drop
`#[allow(unused_imports)]` and `inferred_type_to_pseudo` from the re-export —
the latter already removed in W3).

**What:** Make the field `pub(crate)`. Keep `ssa()` as the public read accessor.
Remove the now-unnecessary blanket allow on the re-export.

**Why:** The field was `pub` but every consumer in the tree goes through the
`ssa()` method (e.g., `examples/test_ssa.rs:32`). Having both is a footgun —
direct field access is a wider API surface than intended. After W3 removes
`inferred_type_to_pseudo` from the re-export, the `unused_imports` allow is
no longer needed.

**LOC:** -3 (annotation width).
**Risk:** Low — only direct field access breaks the compile. The audit
confirmed method-based access everywhere.
**Verification:** Full gate.

## Order of execution

W1 → W2 → W3 → W5 → W4.

Rationale: W1 is independent and the smallest. W2 deletes a whole module.
W3 and W5 are paired (W3's deletion enables W5's cleanup). W4 is the biggest
deletion and goes last so any earlier compile breakage surfaces first.

## Verification fence (per commit)

Run, in order, on each commit:

1. `cargo build --tests` — must compile.
2. `cargo test` — all 521 tests must pass (440 + 31 + 3 + 24 + 5 + 6 + 3 + 9).
3. `cargo test --no-default-features` — full suite must pass.
4. `cargo clippy --all-targets --all-features -- -D warnings` — clean.
5. `cargo fmt --all -- --check` — clean.
6. Corpus replay: `find TestingArtifacts -name '*.nef' | xargs -I{} cargo run
   --quiet -- decompile {} --format ir 2>&1 | grep -q '^}'` — every artifact
   must produce balanced braces.

If any step fails, the change is reverted and the design doc is updated with
the new finding.

## Rollout

Five atomic commits on `master`, each independently revertable. No version
bump (no public API change → no SemVer signal). Push after all five are
green and the user gives the word.

## Out-of-scope items captured for future specs

These were identified during the audit and are explicitly deferred:

- **Retire legacy emitter + postprocess/*.** ~8.3 K LOC. Gated on corpus
  parity (#5 in `2026-06-24-codebase-review.md`).
- **Consolidate method discovery** (extend `MethodTable` with post-RET tail
  detection, delete `helpers::methods::inferred_method_starts` + friends).
  Requires parity test pass.
- **Cache `MethodTable`** across `pipeline::decompile_bytes_with_manifest`
  (currently rebuilt 3–4×). Pure performance, deterministic.
- **Replace `csharpize_statement_untyped`'s 121-line if-let cascade** with a
  data-driven table. Pure refactor.
- **Centralize three `stack_effect` implementations.** One canonical in
  `cfg/ssa/effects.rs`, two callers collapse.
- **Dedup `build_method_labels_by_offset`** (cloned between
  `high_level/render.rs` and `csharp/render.rs`). Extract to
  `helpers/methods.rs`.

Each will get its own design doc when prioritized.

## Risks summary

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| Hidden caller for W2/W3/W4 | Very low | Compile fails | Full gate catches it |
| W1 surface a real dead-code bug | Very low | Compile fails | Fix or revert |
| W5 breaks direct field access | Low | Compile fails | Fix or revert |
| Output byte drift | Zero | Tests fail | Full gate |
| Public API drift | Zero | No `pub` items change except W5 (which tightens) | Type-checked |

## Estimated effort

- Each atomic commit: ~5–20 minutes of editing.
- Each full gate run: ~30 seconds (`cargo test` is the slowest).
- Total wall-clock: ~15 minutes of editing + 5 × 30 s of gates ≈ ~17 minutes.
- Review effort: one commit per line of diff, very low cognitive load.