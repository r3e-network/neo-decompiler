# Neo Decompiler Web NPM Package Implementation Plan

> **For Implementer:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Status:** Completed (shipped in v0.6.0)

**Goal:** Turn the existing `web/` wasm demo into a publishable npm package with a stable TypeScript wrapper and typed browser-facing API.

**Architecture:** Keep the Rust wasm output as the execution engine in `web/pkg/`, then add a small TypeScript wrapper in `web/src/` that normalizes camelCase JS options into the snake_case wasm ABI and re-exports typed reports. Keep the browser demo, but make it consume the wrapper instead of the raw wasm glue so package and demo behavior stay aligned.

**Tech Stack:** Rust wasm output, npm, TypeScript, Node test runner

### Task 1: Add a failing package test

**Files:**
- Modify: `web/package.json`
- Create: `web/test/package.test.mjs`

**Step 1: Write the failing test**

```js
import { createNeoDecompilerClient } from "../dist/index.js";

test("client maps camelCase options to wasm bindings", () => {
  // ...
});
```

**Step 2: Run test to verify it fails**

Run: `cd web && npm test`
Expected: FAIL because `dist/index.js` and the wrapper do not exist yet.

### Task 2: Add the TypeScript wrapper

**Files:**
- Create: `web/src/index.ts`
- Create: `web/tsconfig.json`

**Step 1: Implement typed wrapper and public factory**

```ts
export function createNeoDecompilerClient(bindings) {
  return {
    infoReport(bytes, options) {
      return bindings.infoReport(bytes, normalizeInfoOptions(options));
    },
  };
}
```

**Step 2: Define stable public types**

```ts
export interface InfoOptions { manifestJson?: string; strictManifest?: boolean; }
export interface WebInfoReport { ... }
```

### Task 3: Make the package publishable

**Files:**
- Modify: `web/package.json`
- Modify: `web/README.md`

**Step 1: Add publish metadata**

```json
{
  "name": "neo-decompiler-web",
  "version": "0.6.0",
  "exports": { ".": { "types": "./dist/index.d.ts", "import": "./dist/index.js" } }
}
```

**Step 2: Add build/test scripts**

Run: `cd web && npm test`
Expected: PASS

### Task 4: Move the demo onto the wrapper

**Files:**
- Modify: `web/main.js`
- Modify: `web/index.html`

**Step 1: Switch demo imports from raw wasm glue to the wrapper**

```js
import { init, decompileReport } from "./dist/index.js";
```

### Task 5: Verify the package

**Files:**
- Modify: `README.md`

**Step 1: Run package verification**

Run: `cd web && npm test`
Expected: PASS

Run: `cargo clippy --all-targets --all-features -- -D warnings`
Expected: PASS

Run: `cargo build --target wasm32-unknown-unknown --features web --no-default-features`
Expected: PASS
