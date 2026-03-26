# JavaScript Decompiler Parity Hardening Implementation Plan

> **For Implementer:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Status:** Completed (shipped in v0.6.0)

**Goal:** Close the highest-value Rust-to-JavaScript parity gaps in the standalone `js/` decompiler so it is consistent, production-ready, and self-contained without Rust at runtime.

**Architecture:** Keep the JS port as a separate ESM package under `js/`, but harden its high-level renderer and public API to match Rust behavior where the current implementation is still shallow. Execute the remaining work as small TDD slices: add a focused failing test, verify the failure, implement the narrowest fix, and re-run targeted plus full package tests.

**Tech Stack:** JavaScript (ESM), Node test runner, npm package tooling

### Task 1: Align manifest-driven signatures with Rust output

**Files:**
- Modify: `js/src/manifest.js`
- Modify: `js/src/high-level.js`
- Modify: `js/test/decompiler.test.mjs`

**Step 1: Write the failing tests**

Add tests that prove:
- manifest entry methods render sanitized parameter names and pseudo-types
- manifest helper methods render sanitized names and unique collisions
- manifest return types are normalized to Rust-like pseudo-types such as `int`, `hash160`, and `void`

**Step 2: Run the targeted tests to verify they fail**

Run: `cd /home/neo/git/neo-decompiler/js && npm test -- --test-name-pattern "manifest|sanitize|signature"`
Expected: FAIL because the current JS renderer omits parameter types and does not fully normalize manifest types.

**Step 3: Write the minimal implementation**

Implement shared helpers that:
- sanitize and uniquify parameter names
- format manifest ABI types into the same pseudo-language surface the Rust renderer uses
- reuse those labels consistently in method signatures and lifted bodies

**Step 4: Re-run the targeted tests**

Run: `cd /home/neo/git/neo-decompiler/js && npm test -- --test-name-pattern "manifest|sanitize|signature"`
Expected: PASS

### Task 2: Add structured high-level syscall warnings and context

**Files:**
- Modify: `js/src/high-level.js`
- Modify: `js/src/index.js`
- Modify: `js/test/decompiler.test.mjs`

**Step 1: Write the failing tests**

Add tests that prove:
- missing syscall arguments render as `???`
- the high-level output includes inline warning comments
- `warnings` on the public result include structured high-level warning strings
- prior slot stores add context such as `preceding STLOC0 stored a packed value into loc0`

**Step 2: Run the targeted tests to verify they fail**

Run: `cd /home/neo/git/neo-decompiler/js && npm test -- --test-name-pattern "syscall|warning|packed-store"`
Expected: FAIL because current JS output drops warning metadata and does not annotate missing syscall arguments.

**Step 3: Write the minimal implementation**

Implement high-level warning collection by:
- extending render/lift state with warnings and note emission
- teaching syscall lifting to surface missing-argument warnings with optional store-context hints
- merging high-level warnings into `decompileHighLevelBytes` and `decompileHighLevelBytesWithManifest`

**Step 4: Re-run the targeted tests**

Run: `cd /home/neo/git/neo-decompiler/js && npm test -- --test-name-pattern "syscall|warning|packed-store"`
Expected: PASS

### Task 3: Lift remaining generic control-transfer fallbacks

**Files:**
- Modify: `js/src/high-level.js`
- Modify: `js/test/decompiler.test.mjs`

**Step 1: Write the failing tests**

Add tests that prove:
- unresolved `JMP` and `JMP_L` use `goto label_0x....;`
- unresolved `ENDTRY` and `ENDTRY_L` use `leave label_0x....;`
- target labels are emitted in the method body when those offsets exist

**Step 2: Run the targeted tests to verify they fail**

Run: `cd /home/neo/git/neo-decompiler/js && npm test -- --test-name-pattern "label|goto|leave|endtry"`
Expected: FAIL because the current JS renderer falls back to raw instruction comments.

**Step 3: Write the minimal implementation**

Implement a narrow label-based fallback layer for unstructured control transfer that:
- tracks label targets encountered in a method
- emits stable `label_0x....` markers before matching instructions
- renders `goto` and `leave` statements instead of raw opcode comments

**Step 4: Re-run the targeted tests**

Run: `cd /home/neo/git/neo-decompiler/js && npm test -- --test-name-pattern "label|goto|leave|endtry"`
Expected: PASS

### Task 4: Update package documentation to match the actual JS surface

**Files:**
- Modify: `js/README.md`
- Modify: `README.md`

**Step 1: Document the new parity level**

Describe:
- the standalone JS API
- implemented high-level features
- known non-goals that still remain
- how warning behavior works

**Step 2: Verify docs reference the JS port, not the web/wasm wrapper**

Run: `rg -n "web|wasm" /home/neo/git/neo-decompiler/js/README.md /home/neo/git/neo-decompiler/README.md`
Expected: only intentional references remain.

### Task 5: Verify production readiness before completion

**Files:**
- Modify if needed: `js/src/*`
- Modify if needed: `js/test/*`

**Step 1: Run targeted verification during each slice**

Run the targeted commands from Tasks 1-3 after each change.

**Step 2: Run the full JS suite**

Run: `cd /home/neo/git/neo-decompiler/js && npm test`
Expected: PASS with clean output.

**Step 3: Review for consistency**

Confirm:
- signatures and body labels match
- warnings are preserved in public results
- no unrelated repo changes were reverted

**Step 4: Commit**

Do not commit unless explicitly requested by the user.
