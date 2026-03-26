# Neo Decompiler Web/WASM Implementation Plan

> **For Implementer:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Status:** Completed (shipped in v0.6.0)

**Goal:** Expose the existing Rust decompiler to browser-side JavaScript via WebAssembly without forking the core analysis logic.

**Architecture:** Keep the Rust parser/disassembler/decompiler as the single source of truth and add an optional `web` feature that builds wasm bindings. Surface JS-friendly `info`, `disasm`, and `decompile` entrypoints backed by serializable report structs, then add a small web demo/package scaffold to show browser usage.

**Tech Stack:** Rust 2021, `wasm-bindgen`, `serde`, `serde_json`, optional browser-facing JS assets

### Task 1: Add a failing web API test

**Files:**
- Create: `tests/web_api.rs`
- Modify: `Cargo.toml`

**Step 1: Write the failing test**

```rust
#[cfg(feature = "web")]
#[test]
fn web_decompile_report_exposes_high_level_output_and_hashes() {
    let report = neo_decompiler::web::decompile_report(...).expect("web report");
    assert!(report.high_level.contains("contract"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --features web --test web_api -v`
Expected: FAIL because the `web` feature / module does not exist yet.

### Task 2: Add shared serializable web report builders

**Files:**
- Create: `src/web.rs`
- Create: `src/web/report.rs`
- Modify: `src/lib.rs`

**Step 1: Define minimal serializable report models**

```rust
pub struct WebInfoReport { ... }
pub struct WebDisasmReport { ... }
pub struct WebDecompileReport { ... }
```

**Step 2: Implement byte-based builder functions**

```rust
pub fn info_report(...) -> Result<WebInfoReport> { ... }
pub fn disasm_report(...) -> Result<WebDisasmReport> { ... }
pub fn decompile_report(...) -> Result<WebDecompileReport> { ... }
```

**Step 3: Re-run targeted tests**

Run: `cargo test --features web --test web_api -v`
Expected: Tests still fail until bindings/configuration are complete, but compile should advance.

### Task 3: Add wasm bindings for JS consumers

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/web.rs`

**Step 1: Add optional wasm dependencies and feature wiring**

```toml
[dependencies]
wasm-bindgen = { version = "...", optional = true }
serde-wasm-bindgen = { version = "...", optional = true }
console_error_panic_hook = { version = "...", optional = true }

[features]
web = ["dep:wasm-bindgen", "dep:serde-wasm-bindgen", "dep:console_error_panic_hook"]
```

**Step 2: Export JS-friendly wasm entrypoints**

```rust
#[wasm_bindgen]
pub fn decompile(nef_bytes: &[u8], options: JsValue) -> Result<JsValue, JsValue> { ... }
```

**Step 3: Re-run targeted tests**

Run: `cargo test --features web --test web_api -v`
Expected: PASS

### Task 4: Add browser-facing scaffold and usage docs

**Files:**
- Create: `web/README.md`
- Create: `web/package.json`
- Create: `web/index.html`
- Create: `web/main.js`

**Step 1: Add a minimal browser demo that imports the wasm package**

```js
import init, { decompile } from "./pkg/neo_decompiler.js";
```

**Step 2: Document the build flow**

Run: `wasm-pack build --target web --out-dir web/pkg --features web --no-default-features`
Expected: JS glue + `.wasm` artifact emitted to `web/pkg/`

### Task 5: Verify the new target

**Files:**
- Modify: `README.md`

**Step 1: Run targeted verification**

Run: `cargo test --features web --test web_api -v`
Expected: PASS

Run: `cargo clippy --all-targets --all-features -- -D warnings`
Expected: PASS

Run: `cargo build --target wasm32-unknown-unknown --features web --no-default-features`
Expected: PASS

**Step 2: Update top-level docs**

```markdown
## Web / JS
Build the wasm bindings with ...
```
