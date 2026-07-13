# Structured IR Phi Lowering Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace every live structured-output phi pseudo-call with semantics-preserving assignments on the exact incoming CFG edge while retaining real phi nodes in the analysis-facing SSA view.

**Architecture:** Add a private `PhiLowering` component that converts predecessor-keyed SSA phis into edge-keyed parallel-copy groups after source naming. The existing structurer emits those groups only on the traversed edge; optimizer cleanup removes only trivial/dead phis, while nontrivial analysis SSA remains intact.

**Tech Stack:** Rust, existing CFG/SSA/typed IR model, structured control-flow recovery, unit and NEF integration tests. No new dependencies or public IR variants.

---

### Task 1: Make Trivial Phi Optimization Converge

**Files:**
- Modify: `src/decompiler/cfg/ssa/optimize.rs`

- [x] **Step 1: Strengthen the existing trivial-phi regression**

After the current substitution assertions in `eliminates_trivial_phi`, require the obsolete node and indexes to disappear and a second optimization call to converge:

```rust
assert!(
    ssa.block(BlockId(0))
        .expect("merge block")
        .phi_nodes
        .is_empty()
);
assert!(!ssa.definitions.contains_key(&v("p0", 0)));
assert_eq!(optimize(&mut ssa), 0);
```

Add a `removes_dead_phi_and_releases_operand_definition` test with a phi target absent from all expressions. Assert the phi and its operand-only definition disappear after optimization.

- [x] **Step 2: Run the tests and verify the current stale-node failures**

```bash
cargo test --lib decompiler::cfg::ssa::optimize::tests::eliminates_trivial_phi -- --exact --nocapture
cargo test --lib decompiler::cfg::ssa::optimize::tests::removes_dead_phi_and_releases_operand_definition -- --exact --nocapture
```

Expected: the first test retains `p0` and reports another rewrite round; the second retains the dead phi and its incoming definition.

- [x] **Step 3: Remove substituted and dead phi nodes inside `one_round`**

Track removals before the `subst.is_empty()` return:

```rust
let live_targets: BTreeSet<_> = ssa.uses.keys().cloned().collect();
let mut rewrites = 0usize;
for block in ssa.blocks.values_mut() {
    let before = block.phi_nodes.len();
    block
        .phi_nodes
        .retain(|phi| live_targets.contains(&phi.target));
    rewrites += before - block.phi_nodes.len();
}
```

After trivial substitutions are discovered, remove phis whose targets occur in `subst` before collecting used variables:

```rust
for block in ssa.blocks.values_mut() {
    let before = block.phi_nodes.len();
    block
        .phi_nodes
        .retain(|phi| !subst.contains_key(&phi.target));
    rewrites += before - block.phi_nodes.len();
}
```

If `subst` is empty but `rewrites > 0`, rebuild indexes and return the removal count. Reuse the same `rewrites` accumulator for later expression rewrites rather than redeclaring it.

- [x] **Step 4: Run both tests and the optimizer module**

```bash
cargo test --lib decompiler::cfg::ssa::optimize::tests -- --nocapture
```

Expected: all optimizer tests pass, the second call returns zero, and no stale phi operand pins a dead definition.

### Task 2: Build and Schedule Edge Copies

**Files:**
- Create: `src/decompiler/cfg/phi_lowering.rs`
- Modify: `src/decompiler/cfg/mod.rs`

- [x] **Step 1: Add failing copy-plan unit tests**

Create tests in `phi_lowering.rs` for:

```rust
#[test]
fn groups_live_phi_operands_by_incoming_edge() { /* B1/B2 -> B3 */ }

#[test]
fn fills_missing_real_predecessor_with_unknown() { /* missing B2 operand */ }

#[test]
fn separates_virtual_entry_from_real_backedge() { /* usize::MAX and B1 */ }

#[test]
fn schedules_acyclic_parallel_copies_without_clobbering_sources() {
    // a <- b, c <- a must emit c <- a before a <- b.
}

#[test]
fn breaks_parallel_copy_cycle_with_one_unique_temporary() {
    // a <- b, b <- a must save one old value before either destination changes.
}
```

Assert the resulting `Stmt::Assign` sequences, not debug strings.

- [x] **Step 2: Run the module tests and verify the missing-module failure**

```bash
cargo test --lib decompiler::cfg::phi_lowering::tests -- --nocapture
```

Expected: compile failure because the private module and `PhiLowering` API do not exist.

- [x] **Step 3: Add the private module and data model**

Register `mod phi_lowering;` in `cfg/mod.rs`. In the new file define:

```rust
use std::collections::{BTreeMap, BTreeSet};

use crate::decompiler::cfg::ssa::{ssa_var_name, SsaForm, SsaVariable};
use crate::decompiler::cfg::BlockId;
use crate::decompiler::ir::{Expr, Stmt};

const VIRTUAL_ENTRY: BlockId = BlockId(usize::MAX);

#[derive(Clone, Debug, PartialEq, Eq)]
struct Copy {
    target: String,
    source: String,
}

pub(super) struct PhiLowering {
    edges: BTreeMap<(BlockId, BlockId), Vec<Copy>>,
    entries: BTreeMap<BlockId, Vec<Copy>>,
    used_names: BTreeSet<String>,
}
```

`PhiLowering::new(ssa, source_names)` includes only phis whose target has a use. Iterate every real predecessor from `ssa.cfg.predecessors(successor)` and use `SsaVariable::new("?", 0)` for a missing operand. Store a `VIRTUAL_ENTRY` operand in `entries` rather than `edges`.

- [x] **Step 4: Implement deterministic parallel-copy scheduling**

Expose:

```rust
pub(super) fn edge_statements(&self, from: BlockId, to: BlockId) -> Vec<Stmt>;
pub(super) fn entry_statements(&self, entry: BlockId) -> Vec<Stmt>;
```

Remove `target == source` copies. Repeatedly emit a copy whose destination is not a remaining source. On a cycle, emit `_copy_tmp_<edge>_<n> = source`, replace every matching remaining source with that temporary, and continue. Generate a suffix until the temporary is absent from `used_names` and the pending copies.

- [x] **Step 5: Run all copy-plan tests**

```bash
cargo test --lib decompiler::cfg::phi_lowering::tests -- --nocapture
```

Expected: all grouping, unknown, virtual-entry, acyclic, and cyclic-copy tests pass.

### Task 3: Lower Entry and Single-Successor Phis

**Files:**
- Modify: `src/decompiler/cfg/structure.rs`
- Test: `src/decompiler/cfg/structure.rs`

- [x] **Step 1: Add failing entry and jump-edge tests**

Add `structure_initializes_virtual_entry_phi_once` using an entry loop phi with virtual and self-backedge operands. Add `structure_emits_jump_edge_copy_before_merge_body` using `B0 -> B1` with a live phi in `B1`. Require assignments from `PhiLowering` and assert no typed-IR call named `phi` exists recursively.

- [x] **Step 2: Run the focused tests and verify phi calls remain**

```bash
cargo test --lib structure_initializes_virtual_entry_phi_once -- --nocapture
cargo test --lib structure_emits_jump_edge_copy_before_merge_body -- --nocapture
```

Expected: failure because `emit_phi_nodes` still creates `Expr::Call("phi", ...)` and no edge copies exist.

- [x] **Step 3: Attach `PhiLowering` to `StructCtx`**

Build the plan in `structure_with_source_names`, add it to `StructCtx`, prepend `entry_statements(entry)` to the result, and then append the existing `structure_region` result.

Remove `emit_phi_nodes` and its calls from `emit_body` and `emit_body_except_condition`. Keep analysis-only SSA rendering unchanged.

- [x] **Step 4: Emit copies from blocks with one successor**

At the end of `emit_body`, inspect `self.cfg.successors(bid)`. When exactly one successor exists, append `phi_lowering.edge_statements(bid, successor)` after ordinary statements. This covers fallthrough, jump, latch backedges, and `ENDTRY` continuation edges, including a jump to the current region boundary.

- [x] **Step 5: Run entry/jump tests and the straight-line structurer tests**

```bash
cargo test --lib structure_initializes_virtual_entry_phi_once -- --nocapture
cargo test --lib structure_emits_jump_edge_copy_before_merge_body -- --nocapture
cargo test --lib decompiler::cfg::structure::tests::straight_line_cfg_emits_flat_block -- --exact
```

Expected: all pass without a structured phi call.

### Task 4: Lower Branch, Critical-Edge, and Switch Phis

**Files:**
- Modify: `src/decompiler/cfg/structure.rs`
- Modify: `tests/ir_pipeline.rs`

- [x] **Step 1: Rewrite the two phi-output integration tests first**

Change `structured_ir_defines_stack_phi_before_resolved_call` to require:

```rust
assert!(!ir.contains("phi(") && !ir.contains('φ'));
assert!(ir.matches("p4_0 = ").count() == 2);
assert!(ir.contains("check(p4_0);"));
```

Change the uneven-stack test to require `p4_0 = ?;` only inside the short branch, concrete assignment in the long branch, `helper(p4_1, p4_0);`, and no phi syntax.

Add a hand-built `direct_branch_to_merge_copy_stays_inside_selected_arm` unit test where one branch successor is the phi block itself.

- [x] **Step 2: Run the three tests and verify current failures**

```bash
cargo test --test ir_pipeline structured_ir_defines_stack_phi_before_resolved_call -- --exact --nocapture
cargo test --test ir_pipeline structured_ir_stack_phi_preserves_short_path_underflow -- --exact --nocapture
cargo test --lib direct_branch_to_merge_copy_stays_inside_selected_arm -- --nocapture
```

Expected: integration tests find `phi(` and the critical-edge test finds its copy outside/missing from the selected arm.

- [x] **Step 3: Add a branch-edge region helper**

Add:

```rust
fn structure_edge_region(
    &self,
    from: BlockId,
    entry: BlockId,
    boundary: Option<BlockId>,
    visited: &mut HashSet<BlockId>,
) -> IrBlock {
    let mut out = IrBlock::new();
    out.stmts
        .extend(self.phi_lowering.edge_statements(from, entry));
    out.stmts
        .extend(self.structure_region(entry, boundary, visited, true).stmts);
    out
}
```

Use it for both normal branch arms. For a degenerate same-target branch, emit the one edge-copy group before continuing.

- [x] **Step 4: Carry incoming edge identity through switch recovery**

Store `(case_value, comparison_block, body_entry)` for every case and track `(default_from, default_entry)` along the else chain. Structure cases/default with `structure_edge_region` so direct-to-merge and case-entry phis remain path-local.

- [x] **Step 5: Run branch, switch, and IR-pipeline tests**

```bash
cargo test --lib direct_branch_to_merge_copy_stays_inside_selected_arm -- --nocapture
cargo test --lib decompiler::cfg::structure::tests -- --nocapture
cargo test --test ir_pipeline --all-features
```

Expected: no structured output contains a phi call; branch and switch behavior remains unchanged otherwise.

### Task 5: Lower Loop and Exception-Region Phis

**Files:**
- Modify: `src/decompiler/cfg/structure.rs`
- Test: `src/decompiler/cfg/structure.rs`

- [x] **Step 1: Add failing loop-placement tests**

Add `while_phi_copies_run_in_preheader_and_latch`, `do_while_phi_backedge_copy_stays_in_body`, and `entry_self_loop_keeps_virtual_initialization_separate`. Require preheader/virtual copies before the loop, backedge copies inside the body, and exit-edge copies after the loop.

- [x] **Step 2: Add failing try-region edge test**

Build a `TryEntry` CFG with a live phi at a handler or continuation. Require the selected body/handler edge copy inside that region and no copy on the untaken region.

- [x] **Step 3: Run all four tests and verify incorrect placement**

```bash
cargo test --lib phi_copies_ -- --nocapture
cargo test --lib entry_self_loop_keeps_virtual_initialization_separate -- --nocapture
```

Expected: copies are absent because loop and try entry edges bypass single-successor emission.

- [x] **Step 4: Integrate branch-headed loops and do-while latches**

Use `structure_edge_region(header, body_target, Some(header), ...)` for loop bodies. After the loop statement, append the header-to-exit copy group. For do-while, use a collision-free first-iteration flag to guard latch-to-header copies at the start of subsequent iterations; this prevents the false exit from executing the backedge group. Append latch-to-exit copies after the loop.

Update `try_emit_infinite_branch_loop` to structure both header successor regions through their incoming edges.

- [x] **Step 5: Integrate try/catch/finally entries**

Wrap each `structure_set` result by prepending the edge copies from the `TryEntry` block to that region's entry. Stop every protected region before the shared `ENDTRY`, then emit that block and its single-successor continuation copies once after the `TryCatch` node.

- [x] **Step 6: Run loop, try, and full structurer tests**

```bash
cargo test --lib decompiler::cfg::structure::tests -- --nocapture
```

Expected: every placement test passes and existing if/loop/switch/try recovery remains green.

### Task 6: Format Boundary, Review, and Repository Verification

**Files:**
- Review: `src/decompiler/cfg/phi_lowering.rs`
- Review: `src/decompiler/cfg/structure.rs`
- Review: `src/decompiler/cfg/ssa/optimize.rs`
- Review: `tests/ir_pipeline.rs`

- [x] **Step 1: Add a format-boundary regression**

For one SSA form with a nontrivial live phi, assert `render_ssa_form` still contains `φ(` with predecessor labels while `structure` recursively contains no `Expr::Call` named `phi`. This protects the distinction between analysis output and source-facing output.

- [x] **Step 2: Run focused verification**

```bash
cargo test --lib decompiler::cfg::phi_lowering::tests -- --nocapture
cargo test --lib decompiler::cfg::ssa::optimize::tests -- --nocapture
cargo test --lib decompiler::cfg::structure::tests -- --nocapture
cargo test --test ir_pipeline --all-features
```

- [x] **Step 3: Request independent review**

Require review of parallel-copy ordering, cycle handling, critical edges, virtual entry, while/do-while placement, switch/try routing, malformed missing operands, optimizer convergence, and the SSA-versus-structured format boundary. Fix every critical or important finding.

- [x] **Step 4: Run repository gates**

```bash
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
npm test --prefix js
npm test --prefix web
npm run build --prefix web
cargo deny check
tsc --noEmit --strict --target ES2022 --module NodeNext --moduleResolution NodeNext js/src/index.d.ts
node -e "JSON.parse(require('fs').readFileSync('docs/schema/decompile.schema.json', 'utf8'))"
git diff --check HEAD
```

- [x] **Step 5: Audit completion against the design**

Confirm all structured paths lower live phis to edge-local assignments, copy groups preserve parallel semantics, unknowns stay path-local, optimizer calls converge, analysis SSA retains phi visibility, and no C#, JavaScript, public API, schema, or dependency behavior changed.

**Workspace constraint:** this plan continues an existing cumulative dirty worktree. Do not create a partial commit that captures unrelated prior changes; any eventual commit must follow the repository Lore commit protocol.
