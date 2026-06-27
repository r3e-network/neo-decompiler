# Per-Method Structured IR + Contract Envelope — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `--format ir` render real per-method bodies (`fn main(){…} fn helper(){…}`) inside a contract envelope (name, hashes, features, trusts, ABI methods).

**Architecture:** At render time, build a `MethodTable` from (instructions, manifest); for each method range, extract a fresh sub-CFG (blocks whose `instruction_range.start` is within the range, plus a synthesised entry), rewrite cross-range `Jump`/`Jmpif*`/`Jmpifnot*` to `Return`, run `SsaBuilder` + `optimize_ssa` + `structure_cfg` per sub-CFG, render `fn name() -> ret { body }`, and wrap everything in the legacy envelope (reused verbatim). Fallback to the existing whole-script render if extraction fails.

**Tech Stack:** Rust, `cfg`, `cfg::ssa`, `ir`, `analysis::methods::MethodTable`, `nef`, `manifest`. TDD; full suite + clippy `-D warnings` + fmt + `--no-default-features` as the fence.

**Spec:** `docs/superpowers/specs/2026-06-24-structured-ir-per-method-design.md`

---

## File Structure

- **Modify** `src/decompiler/analysis/methods.rs` — add `pub fn methods(&self) -> impl Iterator<Item = (usize, usize, &MethodRef)>` so the new IR path can iterate spans (currently `spans()` is `pub(super)`).
- **Modify** `src/decompiler/high_level/render/header.rs` — change `write_contract_header` from `pub(super)` to `pub(crate)` so the IR path can reuse it.
- **Modify** `src/decompiler/helpers/types.rs` — change `format_manifest_type` from `pub(in super::super)` to `pub(crate)` so the IR path can format per-method return types.
- **Create** `src/decompiler/cfg/method_view.rs` — the new module: `extract_method_cfgs`, `MethodView`, sub-CFG construction (synthesised entry + cross-range rewrite), per-method SSA/optimize/structure, per-method body render, return-type formatting, envelope composition.
- **Modify** `src/decompiler/decompilation.rs` — rewrite `render_structured_ir` to use the new method-view render; keep the existing whole-script path as a private `render_structured_ir_single_cfg` for the fallback.
- **Modify** `src/decompiler/cfg/mod.rs` — expose `pub mod method_view;`.
- **Test** extend `tests/ir_pipeline.rs` — e2e: MultiMethod renders both fns + envelope; LoopIf still renders `while`; a single-method artifact renders one fn + envelope.

---

## Conventions for every task

- After code: `cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings`.
- Tests live next to code (`#[cfg(test)] mod tests`) unless noted.
- Commit style: `feat(ir): …`, `test(ir): …`, `refactor(csa): …`.

---

## Task 1: Expose `MethodTable::methods()` publicly

**Files:** Modify `src/decompiler/analysis/methods.rs`.

- [ ] **Step 1: Write the failing test**

In `methods.rs`'s test module (or add one — confirm there's a tests module; if not, add a small `#[cfg(test)] mod tests`), add:

```rust
    #[test]
    fn methods_iterates_spans_in_order() {
        use crate::instruction::{Instruction, OpCode};
        let ins = vec![
            Instruction::new(0, OpCode::Push1, None),
            Instruction::new(1, OpCode::Ret, None),
            Instruction::new(10, OpCode::Push0, None),
            Instruction::new(11, OpCode::Ret, None),
        ];
        let table = MethodTable::new(&ins, None);
        let spans: Vec<_> = table.methods().collect();
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].0, 0);
        assert_eq!(spans[1].0, 10);
    }
```

- [ ] **Step 2: Run, verify it fails**

Run: `cargo test --lib methods_iterates_spans_in_order`
Expected: FAIL — `methods` doesn't exist.

- [ ] **Step 3: Add `methods()`**

Replace the `pub(super) fn spans(&self) -> &[MethodSpan]` (line ~262) — keep `spans()` but make it `pub(crate)` and add a new public `methods()` returning an iterator over `(start, end, &MethodRef)` triples:

```rust
    pub(crate) fn spans(&self) -> &[MethodSpan] {
        &self.spans
    }

    /// Iterate known method spans as `(start, end, method_ref)` triples
    /// ordered by `start`. Used by the structured-IR per-method view to
    /// extract a sub-CFG per method.
    pub fn methods(&self) -> impl Iterator<Item = (usize, usize, &MethodRef)> {
        self.spans.iter().map(|s| (s.start, s.end, &s.method))
    }
```

- [ ] **Step 4: Run, verify it passes**

Run: `cargo test --lib methods_iterates_spans_in_order`
Expected: PASS.

- [ ] **Step 5: fmt + clippy + commit**

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings
git add src/decompiler/analysis/methods.rs
git commit -m "feat(methods): expose MethodTable::methods() for the per-method IR view"
```

---

## Task 2: Expose `write_contract_header` and `format_manifest_type`

**Files:** Modify `src/decompiler/high_level/render/header.rs`, `src/decompiler/helpers/types.rs`.

- [ ] **Step 1: Change `write_contract_header` visibility**

In `src/decompiler/high_level/render/header.rs` line 10, change:

```rust
pub(super) fn write_contract_header(
```

to:

```rust
pub(crate) fn write_contract_header(
```

- [ ] **Step 2: Change `format_manifest_type` visibility**

In `src/decompiler/helpers/types.rs` line 56, change:

```rust
pub(in super::super) fn format_manifest_type(kind: &str) -> String {
```

to:

```rust
pub(crate) fn format_manifest_type(kind: &str) -> String {
```

- [ ] **Step 3: fmt + clippy + build**

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings
```
Expected: clean (these are visibility widenings).

- [ ] **Step 4: commit**

```bash
git add src/decompiler/high_level/render/header.rs src/decompiler/helpers/types.rs
git commit -m "refactor: widen envelope helpers for the per-method IR renderer"
```

---

## Task 3: `extract_method_cfgs` — sub-CFG construction with synthesised entry + cross-range rewrite

**Files:** Create `src/decompiler/cfg/method_view.rs` (skeleton with the type + this function + tests); modify `src/decompiler/cfg/mod.rs` to expose `pub mod method_view;`.

- [ ] **Step 1: Create the module skeleton + `MethodView` type**

Create `src/decompiler/cfg/method_view.rs`:

```rust
//! Per-method view of a contract for the structured-IR renderer.

use crate::decompiler::analysis::methods::{MethodRef, MethodTable};
use crate::decompiler::cfg::{
    BasicBlock, BlockId, Cfg, EdgeKind, Terminator,
};

/// A per-method view: the method's instruction slice and a self-contained
/// sub-CFG whose terminators do not leave the method (cross-range jumps are
/// rewritten to `Return`; the sub-CFG has a synthesised entry block).
#[derive(Debug, Clone)]
pub(crate) struct MethodView {
    pub method: MethodRef,
    pub cfg: Cfg,
    pub instructions: Vec<crate::instruction::Instruction>,
}
```

- [ ] **Step 2: Expose the module**

In `src/decompiler/cfg/mod.rs` after the existing `pub use …` lines, add:

```rust
pub mod method_view;
```

- [ ] **Step 3: Add `extract_method_cfgs` (failing test first)**

In `method_view.rs`, add the tests module and the failing test:

```rust
    /// Build a sub-CFG for each method: select blocks whose first instruction
    /// lies within the method's range, prepend a synthesised entry block, and
    /// rewrite cross-range `Jump`/`Jmpif*`/`Jmpifnot*` terminators to `Return`
    /// so the sub-CFG is self-contained. `instructions` is the whole-script
    /// instruction stream; each `MethodView` receives the slice whose offsets
    /// fall within the method's range so `SsaBuilder` only sees the method's
    /// instructions.
pub(crate) fn extract_method_cfgs(
    whole: &Cfg,
    table: &MethodTable,
    instructions: &[crate::instruction::Instruction],
) -> Vec<MethodView> {
    let mut out = Vec::new();
    for (start, end, method) in table.methods() {
        let method_instructions: Vec<_> = instructions
            .iter()
            .filter(|i| i.offset >= start && i.offset < end)
            .cloned()
            .collect();
        if let Some(view) = extract_one(whole, start, end, method.clone(), method_instructions) {
            out.push(view);
        }
    }
    out
}

fn extract_one(
    whole: &Cfg,
    start: usize,
    end: usize,
    method: MethodRef,
    instructions: Vec<crate::instruction::Instruction>,
) -> Option<MethodView> {
    let mut selected: Vec<&BasicBlock> = whole
        .blocks()
        .filter(|b| b.instruction_range.start < end && b.start_offset >= start)
        .collect();
    if selected.is_empty() {
        return None;
    }
    // Find the block that starts at the method's entry offset.
    let entry_existing = selected
        .iter()
        .find(|b| b.start_offset == start)
        .copied();
    // Sort selected blocks by id for stable iteration.
    selected.sort_by_key(|b| b.id.0);
    let entry_id = BlockId(start);
    let mut sub = Cfg::new();
    // Insert selected blocks first (with cross-range rewrites), then the
    // synthesised entry.
    for b in &selected {
        let mut nb = (*b).clone();
        if let Some(eid) = entry_existing.map(|e| e.id) {
            if nb.id == eid {
                sub.add_block(nb);
                continue;
            }
        }
        // Rewrite cross-range jumps.
        nb.terminator = rewrite_terminator(&nb.terminator, &selected);
        sub.add_block(nb);
    }
    // Add the synthesised entry that falls through / jumps to the entry block.
    let entry_terminator = match entry_existing {
        Some(e) if matches!(e.terminator, Terminator::Fallthrough { .. }) => Terminator::Fallthrough { target: e.id },
        Some(_) => Terminator::Jump { target: entry_existing.unwrap().id },
        None => Terminator::Return,
    };
    sub.add_block(BasicBlock::new(
        entry_id,
        start,
        start,
        start..start,
        entry_terminator,
    ));
    if let Some(eid) = entry_existing.map(|e| e.id) {
        sub.add_edge(entry_id, eid, EdgeKind::Unconditional);
    }
    // Copy intra-method edges from the whole CFG.
    for b in &selected {
        for s in b.terminator.successors() {
            if sub.block(s).is_some() && s != b.id {
                let kind = edge_kind_for(whole, b.id, s);
                sub.add_edge(b.id, s, kind);
            }
        }
    }
    Some(MethodView { method, cfg: sub, instructions })
}

fn rewrite_terminator(
    term: &Terminator,
    selected: &[&BasicBlock],
) -> Terminator {
    let in_range = |bid: BlockId| selected.iter().any(|b| b.id == bid);
    match term {
        Terminator::Jump { target } if !in_range(*target) => Terminator::Return,
        Terminator::Branch { then_target, else_target }
            if !in_range(*then_target) || !in_range(*else_target) =>
        {
            Terminator::Return
        }
        _ => term.clone(),
    }
}

fn edge_kind_for(_whole: &Cfg, _from: BlockId, _to: BlockId) -> EdgeKind {
    EdgeKind::Unconditional
}
```

Add a `#[cfg(test)] mod tests` at the bottom of `method_view.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::decompiler::analysis::methods::{MethodRef, MethodTable};
    use crate::decompiler::cfg::{BasicBlock, BlockId, Cfg, EdgeKind, Terminator};
    use crate::instruction::{Instruction, OpCode};

    fn ins(offset: usize, op: OpCode) -> Instruction {
        Instruction::new(offset, op, None)
    }

    fn two_method_whole_cfg() -> (Cfg, Vec<Instruction>) {
        // Method A: offsets 0..2 (push; ret) — block(0).
        // Method B: offsets 10..12 (push; ret) — block(10).
        // Plus a synthetic cross-method jump block(2) whose JMP target 10 is
        // in method B (cross-range jump from A to B).
        let mut cfg = Cfg::new();
        cfg.add_block(BasicBlock::new(
            BlockId(0),
            0,
            2,
            0..2,
            Terminator::Jump { target: BlockId(10) },
        ));
        cfg.add_block(BasicBlock::new(
            BlockId(2),
            2,
            2,
            2..2,
            Terminator::Jump { target: BlockId(10) },
        ));
        cfg.add_edge(BlockId(0), BlockId(2), EdgeKind::Unconditional);
        cfg.add_edge(BlockId(2), BlockId(10), EdgeKind::Unconditional);
        cfg.add_block(BasicBlock::new(
            BlockId(10),
            10,
            12,
            10..12,
            Terminator::Return,
        ));
        let instructions = vec![
            ins(0, OpCode::Push1),
            ins(1, OpCode::Ret),
            ins(10, OpCode::Push0),
            ins(11, OpCode::Ret),
        ];
        (cfg, instructions)
    }

    #[test]
    fn extract_produces_two_sub_cfgs_with_cross_range_jump_rewritten() {
        let (cfg, instructions) = two_method_whole_cfg();
        let table = MethodTable::new(&instructions, None);
        let views = extract_method_cfgs(&cfg, &table, &instructions);
        assert_eq!(views.len(), 2, "expected two methods");
        // Method A's sub-CFG: block(0)'s terminator was Jump→10 (cross-range)
        // and must be rewritten to Return.
        let a = &views[0];
        let a0 = a.cfg.block(BlockId(0)).expect("block 0 in A");
        assert!(matches!(a0.terminator, Terminator::Return));
        // Method B's sub-CFG: its Return block (10) is kept.
        let b = &views[1];
        let b10 = b.cfg.block(BlockId(10)).expect("block 10 in B");
        assert!(matches!(b10.terminator, Terminator::Return));
        // Synthesised entry block exists (id = start offset of the method).
        assert!(a.cfg.block(BlockId(0)).is_some());
    }
}
```

- [ ] **Step 4: Run, verify pass**

Run: `cargo test --lib extract_method_cfgs`
Expected: PASS — the helper compiles (no TODO), the test asserts two sub-CFGs with the cross-range jump rewritten to `Return` and the synthesised entry present.

- [ ] **Step 6: fmt + clippy + commit**

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings
git add src/decompiler/cfg/method_view.rs src/decompiler/cfg/mod.rs
git commit -m "feat(csa): extract per-method sub-CFGs with synthesised entry"
```

---

## Task 4: `render_method_body` — SSA + optimize + structure + render `fn name() -> ret { body }`

**Files:** Modify `src/decompiler/cfg/method_view.rs`.

- [ ] **Step 1: Add `render_method_body`**

Add to `method_view.rs`:

```rust
use crate::decompiler::cfg::ssa::{optimize_ssa, structure_cfg, SsaBuilder};
use crate::decompiler::helpers::types::format_manifest_type;
use crate::decompiler::ir::render_block;
use crate::manifest::ContractManifest;

/// Format a method body as `fn name() -> ret { body }`. The `manifest`
/// provides the return type (looked up by method name); falls back to
/// `void` if the manifest is missing or the method is unknown.
pub(crate) fn render_method_body(
    view: &MethodView,
    manifest: Option<&ContractManifest>,
) -> String {
    let mut ssa = SsaBuilder::new(&view.cfg, &view.instructions).build();
    optimize_ssa(&mut ssa);
    let block = structure_cfg(&ssa);
    let body = render_block(&block, 0);
    let ret = method_return_type(view, manifest);
    let name = sanitize_name(&view.method.name);
    if body.trim().is_empty() {
        format!("    fn {name}() -> {ret} {{\n        // empty body\n    }}\n")
    } else {
        let indented = body
            .lines()
            .map(|l| if l.is_empty() { String::new() } else { format!("        {l}") })
            .collect::<Vec<_>>()
            .join("\n");
        format!("    fn {name}() -> {ret} {{\n{indented}\n    }}\n")
    }
}

fn method_return_type(view: &MethodView, manifest: Option<&ContractManifest>) -> String {
    let Some(manifest) = manifest else {
        return "void".to_string();
    };
    manifest
        .abi
        .methods
        .iter()
        .find(|m| m.name == view.method.name)
        .map(|m| format_manifest_type(&m.return_type))
        .unwrap_or_else(|| "void".to_string())
}

fn sanitize_name(raw: &str) -> String {
    let s: String = raw
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
        .collect();
    if s.is_empty() { "sub".to_string() } else { s }
}
```

- [ ] **Step 2: Add a unit test (small hand-built method)**

In `method_view.rs`'s tests module, add:

```rust
    #[test]
    fn render_method_body_emits_fn_with_return_type() {
        // A trivial method: PUSH1; RET → `fn name() -> int { return 1; }`-ish.
        let instructions = vec![ins(0, OpCode::Push1), ins(1, OpCode::Ret)];
        let mut cfg = Cfg::new();
        cfg.add_block(BasicBlock::new(
            BlockId(0),
            0,
            2,
            0..2,
            Terminator::Return,
        ));
        let view = MethodView {
            method: MethodRef::synthetic(0),
            cfg,
            instructions,
        };
        let manifest_json =
            r#"{"abi":{"methods":[{"name":"main","parameters":[],"returntype":"Integer"}]}}"#;
        let manifest: ContractManifest = serde_json::from_str(manifest_json).unwrap();
        let out = render_method_body(&view, Some(&manifest));
        assert!(out.contains("fn main() -> Integer"), "got:\n{out}");
        assert!(out.contains("return"), "got:\n{out}");
    }
```

- [ ] **Step 3: Run, verify pass**

Run: `cargo test --lib render_method_body`
Expected: PASS.

- [ ] **Step 4: fmt + clippy + commit**

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings
git add src/decompiler/cfg/method_view.rs
git commit -m "feat(ir): render per-method body via SSA structurer"
```

---

## Task 5: `render_envelope` — compose header + per-method bodies + closing `}`

**Files:** Modify `src/decompiler/cfg/method_view.rs`.

- [ ] **Step 1: Add `render_envelope`**

```rust
use crate::decompiler::high_level::render::header::write_contract_header;
use std::fmt::Write;

/// Compose the full contract view: legacy envelope header + per-method
/// bodies + closing `}`. Used by `Decompilation::render_structured_ir`.
pub(crate) fn render_envelope(
    nef: &crate::nef::NefFile,
    manifest: Option<&ContractManifest>,
    methods: &[MethodView],
) -> String {
    let mut out = String::new();
    write_contract_header(&mut out, nef, manifest);
    for view in methods {
        out.push_str(&render_method_body(view, manifest));
        out.push('\n');
    }
    out.push_str("}\n");
    out
}
```

- [ ] **Step 2: fmt + clippy + build**

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings
```
Expected: clean.

- [ ] **Step 3: commit**

```bash
git add src/decompiler/cfg/method_view.rs
git commit -m "feat(ir): compose contract envelope around per-method bodies"
```

---

## Task 6: Wire `Decompilation::render_structured_ir` to the new path + fallback

**Files:** Modify `src/decompiler/decompilation.rs`.

- [ ] **Step 1: Rewrite `render_structured_ir` + keep the single-CFG fallback**

Replace the body of `render_structured_ir` (currently lines ~161-170) with:

```rust
    #[must_use]
    pub fn render_structured_ir(&mut self) -> String {
        // Per-method + envelope render. Fall back to the single-CFG render if
        // extraction fails or yields no methods, so the view never regresses.
        let table = crate::decompiler::analysis::methods::MethodTable::new(
            &self.instructions,
            self.manifest.as_ref(),
        );
        let views = crate::decompiler::cfg::method_view::extract_method_cfgs(
            &self.cfg,
            &table,
            &self.instructions,
        );
        if !views.is_empty() {
            return crate::decompiler::cfg::method_view::render_envelope(
                &self.nef,
                self.manifest.as_ref(),
                &views,
            );
        }
        self.render_structured_ir_single_cfg()
    }

    /// Fallback: render the whole-script CFG as a single structured block.
    /// Used when per-method extraction yields no methods.
    fn render_structured_ir_single_cfg(&mut self) -> String {
        self.optimize_ssa();
        match &self.ssa {
            Some(ssa) => {
                let block = crate::decompiler::cfg::structure_cfg(ssa);
                crate::decompiler::ir::render_block(&block, 0)
            }
            None => String::new(),
        }
    }
}

- [ ] **Step 2: fmt + clippy + build**

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings
```
Expected: clean.

- [ ] **Step 3: commit**

```bash
git add src/decompiler/decompilation.rs
git commit -m "feat(decompilation): render_structured_ir → per-method + envelope"
```

---

## Task 7: E2E tests — MultiMethod renders both fns + envelope; LoopIf while preserved; single-method envelope

**Files:** Modify `tests/ir_pipeline.rs`.

- [ ] **Step 1: Add the MultiMethod e2e test**

```rust
#[test]
fn ir_pipeline_renders_per_method_bodies_inside_envelope_for_multimethod() {
    let root = repo_root();
    let nef = fs::read(root.join("TestingArtifacts/edgecases/multi/MultiMethod.nef")).unwrap();
    let mut dec = Decompiler::new()
        .decompile_bytes_with_manifest(&nef, None, OutputFormat::All)
        .unwrap();
    let ir = dec.render_structured_ir();
    assert!(ir.starts_with("contract MultiMethod {"), "expected contract envelope; got:\n{ir}");
    assert!(ir.contains("fn main()"), "expected `fn main` body; got:\n{ir}");
    assert!(ir.contains("fn helper()"), "expected `fn helper` body; got:\n{ir}");
    assert!(ir.contains("ABI methods"), "expected ABI methods section; got:\n{ir}");
    assert!(ir.trim_end().ends_with("}"), "expected closing `}`; got:\n{ir}");
}
```

- [ ] **Step 2: Run, verify pass**

Run: `cargo test --test ir_pipeline ir_pipeline_renders_per_method_bodies_inside_envelope_for_multimethod`
Expected: PASS.

If it fails, debug: dump `dec.cfg.blocks()` and `dec.instructions` ranges vs the method offsets in the manifest to confirm the slicing is correct.

- [ ] **Step 3: Add a single-method e2e test (LoopIf)**

```rust
#[test]
fn ir_pipeline_loopif_envelope_preserves_while_loop() {
    let root = repo_root();
    let nef = fs::read(root.join("TestingArtifacts/edgecases/LoopIf.nef")).unwrap();
    let mut dec = Decompiler::new()
        .decompile_bytes_with_manifest(&nef, None, OutputFormat::All)
        .unwrap();
    let ir = dec.render_structured_ir();
    assert!(ir.starts_with("contract LoopIf {"), "got:\n{ir}");
    assert!(ir.contains("while"), "expected `while` body; got:\n{ir}");
    assert!(ir.trim_end().ends_with("}"), "got:\n{ir}");
}
```

- [ ] **Step 4: Run, verify pass**

Run: `cargo test --test ir_pipeline ir_pipeline_loopif_envelope_preserves_while_loop`
Expected: PASS.

- [ ] **Step 5: commit**

```bash
git add tests/ir_pipeline.rs
git commit -m "test(ir): e2e per-method rendering for MultiMethod and LoopIf"
```

---

## Task 8: Full verification gate

- [ ] **Step 1: Run all gates**

```bash
cargo test
cargo test --no-default-features
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```
Expected: all green; `parity.rs` output unchanged (legacy path untouched); corpus replay panic-free.

- [ ] **Step 2: Spot-check the MultiMethod IR output (manual)**

Run:
```bash
cargo run --quiet -- decompile TestingArtifacts/edgecases/multi/MultiMethod.nef --format ir
```
Expected: a clean `contract MultiMethod { ... fn main() -> ... { ... } fn helper() -> ... { ... } }`.

- [ ] **Step 3: Update the review doc to mark #4 shipped**

In `docs/superpowers/reviews/2026-06-24-codebase-review.md`, replace the #4 bullet with:

```
4. **Contract envelope + method splitting for the IR view.** ✅ Shipped —
   `Decompilation::render_structured_ir` builds a `MethodTable` and extracts
   per-method sub-CFGs (blocks within the method's offset range, cross-range
   jumps rewritten to `Return`, synthesised entry). Each method gets its own
   SSA/optimize/structure pass and renders `fn name() -> ret { body }`. The
   legacy envelope (name, hashes, features, trusts, ABI methods) is reused
   verbatim. MultiMethod now renders two methods inside a contract;
   LoopIf preserves the `while` loop.
```

And leave #5 as the remaining gate.

- [ ] **Step 4: Commit the review update and push**

```bash
git add docs/superpowers/reviews/2026-06-24-codebase-review.md
git commit -m "docs(review): mark per-method IR + envelope (#4) shipped"
git push origin master
```

---

## Definition of Done

- `MultiMethod.nef --format ir` renders `contract MultiMethod { ... fn main(){...} fn helper(){...} }`.
- `LoopIf.nef --format ir` renders `contract LoopIf { ... fn ...() { while(...) {...} } }`.
- Single-method contracts render one `fn` inside the envelope.
- Full suite (519+ tests) green; clippy `-D warnings`, fmt, `--no-default-features`, corpus replay panic-free.
- Legacy high-level / C# / pseudocode / json / web outputs unchanged.