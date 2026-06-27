# Implementation Plan: Architecture & Code Simplification — Quick Wins

**Spec:** `docs/superpowers/specs/2026-06-27-simplification-quick-wins-design.md`
**Date:** 2026-06-27
**Goal:** Pure deletions, zero behavior change. 5 atomic commits on `master`.

---

## Convention

Each task = one atomic commit. Order: W1 → W2 → W3 → W5 → W4 (per spec §"Order
of execution"). Full verification fence after each commit (build + 3 test
suites + clippy + fmt + corpus replay).

The fence for every task:

```bash
cargo build --tests &&
cargo test &&
cargo test --no-default-features &&
cargo clippy --all-targets --all-features -- -D warnings &&
cargo fmt --all -- --check &&
(cargo run --quiet -- decompile TestingArtifacts/edgecases/LoopIf.nef --format ir | grep -q '^}')
```

If any step fails, revert the commit and update the design doc with the
finding.

---

## Task W1 — Strip stale `#[allow(dead_code)]` from `method_view.rs`

**File:** `src/decompiler/cfg/method_view.rs`

**Steps:**

1. Read the file end-to-end (already done in audit). Lines 14, 29, 49, 117,
   133, 193 carry `#[allow(dead_code)]` annotations + comments like
   `// wired up by Decompilation::render_structured_ir (Task 5/6)`.
2. For each of the 6 sites: delete the `#[allow(dead_code)]` attribute line
   and its trailing comment. Keep the doc comment on the item itself.
3. Run the full fence.
4. If a compile error appears, it surfaces a real dead-code bug — fix or
   revert.
5. Commit: `chore(method_view): strip stale dead_code annotations`.

**Acceptance:** `cargo build --tests` succeeds; all 521 tests pass; the file
shrinks by ~12 LOC.

---

## Task W2 — Delete dead SSA-convert module

**Files:** `src/decompiler/cfg/ssa/convert.rs` (entire),
`src/decompiler/cfg/ssa/mod.rs:17` (re-export).

**Steps:**

1. Read `convert.rs` to confirm it has no callers (the audit says it has
   `expr_to_ssa` and `stmt_to_ssa`; verify no `use ssa::convert::` anywhere
   in the tree).
2. Delete `convert.rs`.
3. In `ssa/mod.rs`, delete the `pub use convert::{expr_to_ssa, stmt_to_ssa};`
   line.
4. Run the full fence.
5. Commit: `chore(ssa): delete dead convert module (no callers)`.

**Acceptance:** Full fence green; file count drops by 1; re-export gone.

---

## Task W3 — Delete speculative `inferred_type_to_pseudo`

**Files:** `src/decompiler/helpers/types.rs:12-28` (function),
`src/decompiler/helpers/types.rs:100-111` (tests),
`src/decompiler/helpers.rs:21` (re-export line for this function).

**Steps:**

1. Read `types.rs` to confirm the function and its tests have no callers.
2. Delete the function definition.
3. Delete the two tests referencing it.
4. In `helpers.rs`, remove `inferred_type_to_pseudo` from the re-export list
   on line 21. Keep the `#[allow(unused_imports)]` for now (W5 cleans it up).
5. Run the full fence.
6. Commit: `chore(helpers): delete speculative inferred_type_to_pseudo`.

**Acceptance:** Full fence green; function + tests + re-export entry gone.

---

## Task W5 — Tighten `Decompilation.ssa` visibility + clean helpers.rs

**Files:** `src/decompiler/decompilation.rs:38`,
`src/decompiler/helpers.rs:21`.

**Steps:**

1. Change `pub ssa: Option<SsaForm>` → `pub(crate) ssa: Option<SsaForm>` in
   `decompilation.rs:38`.
2. In `helpers.rs:21`, drop `#[allow(unused_imports)]` (no longer needed
   after W3) and keep only `format_manifest_type` + `inferred_type_to_csharp`
   in the re-export list.
3. Run the full fence. If any consumer reads `dec.ssa` directly (not through
   `dec.ssa()`), the compile fails — fix or revert.
4. Commit: `chore(decompilation): tighten ssa field visibility + clean helpers re-export`.

**Acceptance:** Full fence green; field is `pub(crate)`; the blanket allow
is gone.

---

## Task W4 — Delete dead `Stmt` variants

**Files:** `src/decompiler/ir/statement.rs:19-30`,
`src/decompiler/ir/render/stmt/mod.rs` (render arms).

**Steps:**

1. Read `ir/statement.rs` to confirm the four variants (`VarDecl`, `Throw`,
   `Break`, `Continue`) have no construction sites.
2. Delete the four variant arms from the `Stmt` enum.
3. Find and delete the matching render arms in `ir/render/stmt/mod.rs` (the
   audit says lines 19-26 / 39-42, but verify by grepping for the variant
   names).
4. Run the full fence.
5. Commit: `chore(ir): delete dead Stmt variants (VarDecl, Throw, Break, Continue)`.

**Acceptance:** Full fence green; four variants + their render arms gone.

---

## Final verification

After all 5 commits:

```bash
git log --oneline -8 &&
cargo test &&
cargo test --no-default-features &&
cargo clippy --all-targets --all-features -- -D warnings &&
cargo fmt --all -- --check &&
git push origin master
```

Verify the push succeeds; print the final commit graph.

---

## Risk model (recap)

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| Hidden caller for W2/W3/W4 | Very low | Compile fails | Full gate |
| W1 surfaces real dead-code bug | Very low | Compile fails | Fix or revert |
| W5 breaks direct field access | Low | Compile fails | Fix or revert |
| Output byte drift | Zero | Tests fail | Full gate |

Each revert + doc update is fast (~5 min). No structural risk.

---

## Effort estimate

- W1: ~5 min edit, ~30 s gate.
- W2: ~10 min (file deletion + module cleanup), ~30 s gate.
- W3: ~15 min (find tests, edit three sites), ~30 s gate.
- W5: ~5 min, ~30 s gate.
- W4: ~20 min (find render arms across stmt/mod.rs), ~30 s gate.

Total: ~55 min edit + 5 × 30 s gates ≈ ~60 min wall-clock.

No public API change. No version bump. Push after user confirmation.