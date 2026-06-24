# Advanced Decompiler Evolution — Phase 0 + 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Lock in a correctness/panic regression fence across the full fuzz + artifact corpus (Phase 0), then wire the existing-but-unused type-inference engine into both renderers so output carries real types instead of `loc0`/`arg0` (Phase 1).

**Architecture:** Phase 0 adds a replay test (`tests/corpus_replay.rs`) that runs every fuzz corpus through `decompile_bytes`/raw-disassemble+SSA under `catch_unwind`, and re-decompiles every `TestingArtifacts/*` NEF across all formats. Phase 1 threads the already-computed `TypeInfo` (`decompiler/analysis/types.rs`) into `render_high_level` and `render_csharp`, mapping each `ValueType` to a pseudo and a C# type string, and emitting typed declarations only when a non-`Unknown` type is known (additive — `loc0` fallback preserved).

**Tech Stack:** Rust 2021 (msrv 1.86), `#![forbid(unsafe_code)]`, existing `cargo test --all-features` (baseline 481 + 3 new = 484).

**Design reference:** `docs/superpowers/specs/2026-06-24-advanced-decompiler-design.md`

---

## Phase 0 — Findings (no code changes required; recorded for posterity)

- **Fuzz corpus (6400+ inputs):** panic-free across `fuzz_decompile`, `fuzz_decompile_raw`, `fuzz_nef_parse`, `fuzz_manifest`. Only 3/188 `fuzz_decompile` NEF inputs parse (rest correctly rejected).
- **Jump-target resolution verified correct** against authoritative neo-vm source (`ExecuteJumpOffset` → `IP + offset` where IP is the opcode offset because `ExecuteNext` skips the `+= Size` advance when `isJumping`). `src/decompiler/cfg/builder/targets.rs:24` is correct. The `LoopIf` fixture output faithfully renders a degenerate hand-crafted ("compiler: edge") bytecode — not a bug.
- **Structural validity:** all artifacts produce balanced braces, no double-`else`, no dangling `goto`/`leave`, no identity temps. The only failing artifact (`CallFlagInvalid.nef`) is in `known_unsupported.txt` by design.

Conclusion: the foundation (parse → disassemble → CFG → render) is robust and correct. Real bugs, if any, live in narrow structural-recovery edge cases to be probed in later phases.

### Task 0.1: Corpus parity / panic regression test  ✅ done

**Files:**
- Create: `tests/corpus_replay.rs`

Replays all fuzz corpora through the pipeline under `catch_unwind` and decompiles every artifact across formats; fails on any panic. Implemented; 3 tests pass.

- [x] Write `tests/corpus_replay.rs` (replay + artifacts + nef-parser smoke)
- [x] `cargo test --test corpus_replay` → 3 passed
- [x] `cargo test --all-features` → 484 passed (481 baseline + 3 new)

---

## Phase 1 — Wire type inference into renderers

`infer_types` (`src/decompiler/analysis/types.rs:133`) already produces `TypeInfo { methods: Vec<MethodTypes{arguments, locals}>, statics }`, indexed identically to the `locN`/`argN`/`staticN` placeholders the emitter emits. It is computed in the pipeline (`pipeline.rs:135`) but never read by either renderer. Phase 1 makes it visible, additively.

### Task 1.1: Type-string maps (pseudo + C#)

**Files:**
- Create: `src/decompiler/helpers/types/render.rs`
- Modify: `src/decompiler/helpers/types.rs` (add `mod render;` + re-export), or create the map in `helpers/types.rs` directly following existing structure.

Add pure functions mapping `ValueType → &'static str` for each dialect. Pure, unit-tested in isolation.

- [ ] **Step 1: Write failing tests** in `src/decompiler/helpers/types.rs` (or a sibling test module) asserting:
  - `pseudo_type(ValueType::Integer) == "int"`
  - `pseudo_type(ValueType::Boolean) == "bool"`
  - `pseudo_type(ValueType::ByteString) == "byte[]"`
  - `pseudo_type(ValueType::Buffer) == "byte[]"`
  - `pseudo_type(ValueType::Array) == "object[]"` (pseudo) / `"object[]"` 
  - `pseudo_type(ValueType::Map) == "map"`
  - `pseudo_type(ValueType::Unknown) == ""` (empty → caller falls back to `loc0`)
  - `csharp_type(ValueType::Integer) == "BigInteger"`
  - `csharp_type(ValueType::Boolean) == "bool"`
  - `csharp_type(ValueType::ByteString) == "ByteString"`
  - `csharp_type(ValueType::Buffer) == "byte[]"`
  - `csharp_type(ValueType::Array) == "object[]"` 
  - `csharp_type(ValueType::Map) == "Map"`
  - `csharp_type(ValueType::Unknown) == ""`
- [ ] **Step 2: Run → FAIL** (`unresolved`).
- [ ] **Step 3: Implement** `pub fn pseudo_type(ValueType) -> &'static str` and `pub fn csharp_type(ValueType) -> &'static str` via `match`.
- [ ] **Step 4: Run → PASS.**
- [ ] **Step 5: Commit** `feat(analysis): add ValueType → pseudo/C# type-string maps`.

### Task 1.2: Index TypeInfo by method offset for renderer lookup

**Files:**
- Modify: `src/decompiler/high_level/render.rs` (and mirror in `src/decompiler/csharp/render.rs`).

The renderers iterate methods by ABI offset. Build a `BTreeMap<method_start_offset, &MethodTypes>` from `TypeInfo.methods` (each `MethodTypes.method` carries `MethodRef` with its offset) once per render, so the per-method emitter can look up `locals[slot]` / `arguments[slot]` in O(log n). Add a small helper struct `SlotTypes { locals: Vec<ValueType>, arguments: Vec<ValueType>, statics: &[ValueType] }` resolving `Unknown` when out of range.

- [ ] **Step 1: Write a failing test** that builds a `TypeInfo` for one method with `locals = [Integer]` and asserts the lookup helper returns `Some(Integer)` for slot 0 and `None`/`Unknown` for slot 99.
- [ ] **Step 2: Run → FAIL.**
- [ ] **Step 3: Implement** the lookup helper.
- [ ] **Step 4: Run → PASS.**
- [ ] **Step 5: Commit** `feat(high-level): add per-method slot-type lookup from TypeInfo`.

### Task 1.3: Emit typed local/arg/static declarations (high-level)

**Files:**
- Modify: `src/decompiler/high_level/render.rs` (thread `&TypeInfo` into `render_high_level`).
- Modify: `src/decompiler/high_level/emitter/mod.rs` / `slots.rs` ONLY where declarations are produced — emit `<type> <name>` when the type is known and non-`Unknown`; leave existing `loc0` fallback untouched when unknown. Gated so existing golden tests that don't assert types stay byte-identical.

Conservative rule: only annotate the *first* declaration of each slot (the `let loc0 = ...` line). Do not re-type later assignments.

- [ ] **Step 1: Write a failing test** that decompiles a synthetic method whose local is seeded `Integer` by the inference engine and asserts the high-level output contains `int loc0` (not just `loc0`).
- [ ] **Step 2: Run → FAIL.**
- [ ] **Step 3: Implement** — thread `TypeInfo`, apply the type-string map at the declaration site.
- [ ] **Step 4: Run → full suite PASS** (existing tests unaffected because their inferred types are `Unknown` → fallback).
- [ ] **Step 5: Commit** `feat(high-level): annotate local/arg declarations with inferred types`.

### Task 1.4: Mirror for C# renderer

**Files:**
- Modify: `src/decompiler/csharp/render.rs`, `src/decompiler/csharp/render/body.rs` as needed.

Same treatment using `csharp_type`. C# already declares types via the manifest-driven signatures; only annotate body-local declarations.

- [ ] **Step 1-5:** TDD as in 1.3; assert `BigInteger loc0` style.
- [ ] **Step 6: Commit** `feat(csharp): annotate locals with inferred types`.

### Task 1.5: Static-field typing

**Files:**
- Modify: same renderers' static-field declaration sites.

`TypeInfo.statics` is a single global vector. Annotate static declarations where known.

- [ ] TDD + commit `feat(analysis): annotate static-field declarations with inferred types`.

---

## Verification (per phase)
- `cargo test --all-features` green (484 after Phase 0).
- `cargo build --all-features && cargo build --release` clean.
- `cargo clippy --all-features -- -D warnings` clean (honor `#![forbid(unsafe_code)]`).
- Differential: existing 481 golden tests must remain byte-identical unless they assert types (additive-only guarantee).

## Self-review
- Spec coverage: design §4 Phase 0 (bug hunt, perf, parity test) and Phase 1 (wire type inference) → Tasks 0.1, 1.1–1.5. Perf wins deferred to a follow-up task batch (not dropped — tracked in todos).
- Placeholders: none; each step shows exact code intent and asserts.
- Type consistency: `ValueType`, `TypeInfo`, `MethodTypes`, `MethodRef` used consistently; `pseudo_type`/`csharp_type` defined in 1.1 and consumed in 1.3/1.4/1.5.
