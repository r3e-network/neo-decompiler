# NEF Hardening And Consistency Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Harden NEF parsing against malformed overlong varints and clean up core decompiler consistency issues so the crate is stricter, safer, and lint-clean.

**Architecture:** Keep the public API stable and tighten behavior at the low-level parsing boundary where malformed data enters the system. Add regression tests around malformed NEF payloads first, then implement the minimal parser change and small internal refactors that preserve behavior while improving consistency.

**Tech Stack:** Rust 2021, `cargo test`, `cargo clippy`, `cargo fmt`

### Task 1: Reject non-canonical NEF varints

**Files:**
- Modify: `src/error/nef.rs`
- Modify: `src/nef/encoding.rs`
- Test: `src/nef/tests/parse.rs`
- Test: `src/nef/tests/method_tokens/errors.rs`

**Step 1: Write the failing tests**

- Add a parser test that encodes an empty NEF source string using an overlong varint and expects a dedicated NEF parse error.
- Add a method-token parser test that encodes the token count using an overlong varint and expects the same error.

**Step 2: Run test to verify it fails**

Run: `cargo test rejects_non_canonical_varint`
Expected: FAIL because the parser currently accepts overlong varints.

**Step 3: Write minimal implementation**

- Add a dedicated NEF error variant for non-canonical varints.
- Teach `read_varint` to reject any encoding that is longer than `varint_encoded_len(value)` for supported `u32` values.

**Step 4: Run test to verify it passes**

Run: `cargo test rejects_non_canonical_varint`
Expected: PASS

### Task 2: Remove avoidable parser/decompiler sharp edges

**Files:**
- Modify: `src/decompiler/analysis/call_graph.rs`
- Modify: `src/decompiler/high_level/emitter/control_flow/branches.rs`
- Modify: `src/decompiler/high_level/emitter/core.rs`
- Modify: `src/decompiler/high_level/emitter/helpers.rs`
- Modify: `src/decompiler/high_level/emitter/postprocess/overflow_collapse.rs`
- Modify: `src/decompiler/high_level/emitter/postprocess/simplify.rs`

**Step 1: Preserve coverage before refactoring**

- Rely on existing decompiler and high-level tests that already cover these code paths.

**Step 2: Write minimal refactor**

- Replace Clippy-flagged `map_or(false, ...)` patterns with `is_some_and(...)`.
- Prefer slice-based APIs over `&mut Vec<_>` when mutation does not require vector-only behavior.
- Replace needless index-based loops with iterator-based forms where it improves clarity without changing behavior.

**Step 3: Run targeted verification**

Run: `cargo test`
Expected: PASS

**Step 4: Run lint verification**

Run: `cargo clippy --all-targets --all-features`
Expected: PASS without the current warnings.
