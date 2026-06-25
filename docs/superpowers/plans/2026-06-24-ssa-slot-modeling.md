# SSA Over Named Slots — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make locals/args/statics versioned SSA variables so store-then-load bodies render real content in the structured-IR/SSA views (the blocker for promoting `--format ir` to the default).

**Architecture:** Mirror the SSA builder's existing stack-slot φ machinery for named slots. A per-block `SlotState` (`{slot_name → reaching SsaVariable}`) is threaded through `execute_block`; stores (`Stloc0`/`Starg`/`Stsfld` family) define a new version and update the map, loads (`Ldloc0`/`Ldarg`/`Ldsfld` family) read the reaching version (uninitialised reads keep the opaque `ldloc0()` fallback). A new `compute_join_slots` (twin of `compute_join_entry`) places φ where predecessors disagree, and `exit_slots` stabilises alongside `exit_stacks` in the existing fixpoint. Copy/const prop and DCE need no changes — slot defs are ordinary `Assign`/φ.

**Tech Stack:** Rust, `src/decompiler/cfg/ssa/` (builder/form/variable/effects modules), `cargo test` / `cargo clippy -D warnings` / `cargo fmt --check`.

**Spec:** `docs/superpowers/specs/2026-06-24-ssa-slot-modeling-design.md`

---

## File Structure

- **Modify** `src/decompiler/cfg/ssa/builder.rs` — the heart of the change:
  - generalize `slot_name_for` (builder.rs:527) to cover store opcodes too;
  - add `slot_name_for_store`/`compute_join_slots` helpers + a `SlotState` alias;
  - thread `SlotState` through `execute_block` (217) / `apply_instruction` (245);
  - emit store assignments and load-copies;
  - extend the fixpoint loop (104) and final assembly pass (124) with `entry_slots`/`exit_slots`.
- **No changes to** `form.rs`, `variable.rs`, `optimize.rs`, `to_ir.rs`, `effects.rs`, or the structurer — slot defs surface as ordinary `SsaStmt::Assign` / `PhiNode`.
- **Tests** in `src/decompiler/cfg/ssa/builder.rs` `#[cfg(test)] mod tests` (helpers `linear`/`instr` at builder.rs:761).
- **Modify** `tests/ir_pipeline.rs` — strengthen the switch test to assert case bodies render.

---

## Conventions for every task

- After each code change run: `cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings`.
- Tests live in the existing `mod tests` inside `builder.rs` unless noted. Test helpers already imported there: `super::*`, `crate::decompiler::cfg::{BasicBlock, BlockId, CfgBuilder, EdgeKind, Terminator}`, `crate::instruction::{Instruction, OpCode, Operand}`. `linear(Vec<Instruction>) -> (Vec<Instruction>, Cfg)` builds a straight-line CFG; `instr(off, OpCode)` makes a no-operand instruction.
- Commit message style: conventional commits (`feat(ssa): ...`, `test(ssa): ...`).

---

## Task A: Stores define slot versions (intra-block)

**Files:** Modify `src/decompiler/cfg/ssa/builder.rs`.

- [ ] **Step A1: Write the failing test**

Add to the `mod tests` block (after `dup_creates_a_copy_definition`, around builder.rs:831):

```rust
#[test]
fn store_local_emits_a_slot_assignment() {
    // PUSH10 ; STLOC0 ; RET  →  the store must define a loc0 SSA var.
    let ins = vec![
        Instruction::new(0, OpCode::Push10, None),
        Instruction::new(1, OpCode::Stloc0, None),
        Instruction::new(2, OpCode::Ret, None),
    ];
    let (ins, cfg) = linear(ins);
    let ssa = SsaBuilder::new(&cfg, &ins).build();
    let block = ssa.blocks_iter().next().expect("a block exists").1;

    // Expect two assignments: t? = 10 (push), loc0_1 = <that push var> (store).
    let has_loc0_assign = block.stmts.iter().any(|s| match s {
        SsaStmt::Assign { target, .. } => target.base == "loc0",
        _ => false,
    });
    assert!(
        has_loc0_assign,
        "STLOC0 should define a loc0 SSA variable; got {:?}",
        block.stmts
    );
}
```

- [ ] **Step A2: Run the test to verify it fails**

Run: `cargo test --lib store_local_emits_a_slot_assignment`
Expected: FAIL — `STLOC0` currently emits no statement (it is a `(1,0)` sink), so no `loc0` assign exists.

- [ ] **Step A3: Generalize `slot_name_for` to cover store opcodes**

Replace the body of `slot_name_for` (builder.rs:527-566) so the match also covers the store families. The full replacement:

```rust
fn slot_name_for(op: OpCode, operand: &Option<Operand>) -> Option<String> {
    use OpCode::*;
    let (kind, idx): (&str, usize) = match op {
        // Loads.
        Ldloc0 => ("loc", 0),
        Ldloc1 => ("loc", 1),
        Ldloc2 => ("loc", 2),
        Ldloc3 => ("loc", 3),
        Ldloc4 => ("loc", 4),
        Ldloc5 => ("loc", 5),
        Ldloc6 => ("loc", 6),
        Ldarg0 => ("arg", 0),
        Ldarg1 => ("arg", 1),
        Ldarg2 => ("arg", 2),
        Ldarg3 => ("arg", 3),
        Ldarg4 => ("arg", 4),
        Ldarg5 => ("arg", 5),
        Ldarg6 => ("arg", 6),
        Ldsfld0 => ("static", 0),
        Ldsfld1 => ("static", 1),
        Ldsfld2 => ("static", 2),
        Ldsfld3 => ("static", 3),
        Ldsfld4 => ("static", 4),
        Ldsfld5 => ("static", 5),
        Ldsfld6 => ("static", 6),
        // Stores (symmetric to the loads above).
        Stloc0 => ("loc", 0),
        Stloc1 => ("loc", 1),
        Stloc2 => ("loc", 2),
        Stloc3 => ("loc", 3),
        Stloc4 => ("loc", 4),
        Stloc5 => ("loc", 5),
        Stloc6 => ("loc", 6),
        Starg0 => ("arg", 0),
        Starg1 => ("arg", 1),
        Starg2 => ("arg", 2),
        Starg3 => ("arg", 3),
        Starg4 => ("arg", 4),
        Starg5 => ("arg", 5),
        Starg6 => ("arg", 6),
        Stsfld0 => ("static", 0),
        Stsfld1 => ("static", 1),
        Stsfld2 => ("static", 2),
        Stsfld3 => ("static", 3),
        Stsfld4 => ("static", 4),
        Stsfld5 => ("static", 5),
        Stsfld6 => ("static", 6),
        Ldloc | Ldarg | Ldsfld | Stloc | Starg | Stsfld => {
            let kind = match op {
                Ldloc | Stloc => "loc",
                Ldarg | Starg => "arg",
                Ldsfld | Stsfld => "static",
                _ => return None,
            };
            match operand {
                Some(Operand::U8(n)) => (kind, *n as usize),
                _ => return None,
            }
        }
        _ => return None,
    };
    Some(format!("{kind}{idx}"))
}
```

- [ ] **Step A4: Add the `SlotState` type alias and a store-handling helper**

Just above `fn slot_name_for` (builder.rs:524), add:

```rust
/// Per-block reaching definition for each named slot (`"loc0"` → latest SSA
/// version). Threaded through `execute_block`; stores define a new version,
/// loads read the reaching version, and at joins `compute_join_slots` places φ
/// where predecessors disagree.
type SlotState = BTreeMap<String, SsaVariable>;
```

- [ ] **Step A5: Thread `SlotState` through `execute_block` and `apply_instruction`**

Update `BlockExec` (builder.rs:501) to carry the exit slot state:

```rust
#[derive(Default)]
struct BlockExec {
    exit_stack: Vec<SsaVariable>,
    exit_slots: SlotState,
    stmts: Vec<SsaStmt>,
    uses: Vec<(SsaVariable, usize)>,
}
```

Update `execute_block` (builder.rs:217) to seed a local slot map from the passed-in entry slots and return it (the `entry_slots` parameter is `&[SsaVariable]`-style; for Task A pass an empty `SlotState` — the cross-block wiring arrives in Task C):

```rust
fn execute_block(
    &self,
    bid: BlockId,
    entry: &[SsaVariable],
    entry_slots: &SlotState,
    versions: &mut BTreeMap<String, usize>,
) -> BlockExec {
    let Some(block) = self.cfg.block(bid) else {
        return BlockExec::default();
    };
    let mut stack: Vec<SsaVariable> = entry.to_vec();
    let mut slots: SlotState = entry_slots.clone();
    let mut stmts: Vec<SsaStmt> = Vec::new();
    let mut uses: Vec<(SsaVariable, usize)> = Vec::new();

    for idx in block.instruction_range.clone() {
        let Some(instr) = self.instructions.get(idx) else {
            continue;
        };
        self.apply_instruction(instr, bid, &mut stack, &mut slots, &mut stmts, &mut uses, versions);
    }

    BlockExec {
        exit_stack: stack,
        exit_slots: slots,
        stmts,
        uses,
    }
}
```

Update `apply_instruction` (builder.rs:245) signature to take `slots: &mut SlotState`, and handle stores. The full replacement of the function body (keep the reorder/special early-returns at the top):

```rust
fn apply_instruction(
    &self,
    instr: &Instruction,
    _bid: BlockId,
    stack: &mut Vec<SsaVariable>,
    slots: &mut SlotState,
    stmts: &mut Vec<SsaStmt>,
    uses: &mut Vec<(SsaVariable, usize)>,
    versions: &mut BTreeMap<String, usize>,
) {
    let op = instr.opcode;

    if effects::is_stack_reorder(op) {
        self.apply_reorder(op, stack, stmts, versions);
        return;
    }
    if effects::is_stack_special(op) {
        self.apply_special(instr, stack, stmts, uses);
        return;
    }

    let (pop, push) = effects::stack_effect(op);

    let mut popped: Vec<SsaVariable> = Vec::with_capacity(pop);
    for _ in 0..pop {
        let v = stack.pop().unwrap_or_else(unknown_var);
        popped.push(v);
    }
    popped.reverse();

    let use_index = stmts.len();
    for v in &popped {
        if !is_unknown(v) {
            uses.push((v.clone(), use_index));
        }
    }

    if push == 1 {
        // A load whose slot has a reaching version reads that version instead of
        // an opaque ldloc0(); otherwise fall through to the call placeholder.
        let reaching = slot_name_for(op, &instr.operand).and_then(|name| slots.get(&name).cloned());
        let expr = match reaching {
            Some(var) => SsaExpr::var(var),
            None => self.build_expr(op, instr, &popped),
        };
        let base = slot_name_for(op, &instr.operand).unwrap_or_else(|| "t".to_string());
        let target = fresh_var(versions, &base);
        stmts.push(SsaStmt::assign(target.clone(), expr));
        stack.push(target);
    } else if push == 0 {
        // A store defines a new version of its target slot: `loc0_N = <value>`.
        if let Some(name) = slot_name_for(op, &instr.operand) {
            if let Some(value) = popped.first().cloned() {
                let target = fresh_var(versions, &name);
                stmts.push(SsaStmt::assign(target.clone(), SsaExpr::var(value)));
                slots.insert(name, target);
            }
        }
        // Other push==0 opcodes (assert/throw/jump condition) only consumed;
        // uses were already recorded above.
    }
}
```

- [ ] **Step A6: Update callers of `execute_block` to pass an empty `SlotState`**

In the fixpoint loop (builder.rs:110) and the final assembly pass (builder.rs:132), `execute_block` now takes an `entry_slots` argument. For Task A pass `&SlotState::default()` (an empty map) at both call sites — the cross-block state lands in Task C.

```rust
// fixpoint loop (builder.rs:110)
let exec = self.execute_block(bid, &new_entry, &SlotState::default(), &mut versions);
```

```rust
// final assembly pass (builder.rs:132)
let exec = self.execute_block(bid, &entry, &SlotState::default(), &mut versions);
```

- [ ] **Step A7: Run the test to verify it passes**

Run: `cargo test --lib store_local_emits_a_slot_assignment`
Expected: PASS — `STLOC0` now emits `loc0_1 = <push var>`.

- [ ] **Step A8: Run the whole SSA suite to check for regressions**

Run: `cargo test --lib cfg::ssa`
Expected: PASS — all existing SSA tests still green (loads still produce `loc0_N = ldloc0()` because no reaching version exists intra-block until a preceding store, and existing tests don't store-then-load).

- [ ] **Step A9: fmt + clippy + commit**

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
git add src/decompiler/cfg/ssa/builder.rs
git commit -m "feat(ssa): stores define slot versions (intra-block)"
```

---

## Task B: Loads read the reaching version (intra-block)

**Files:** Modify `src/decompiler/cfg/ssa/builder.rs` (test only — the load-copy logic already landed in Step A5).

- [ ] **Step B1: Write the failing test**

Add to `mod tests`. It asserts the connection structurally on the **unoptimized** SSA — the load must read the reaching version (a `Variable`), not be an opaque `Call`:

```rust
#[test]
fn store_then_load_connects_within_a_block() {
    // PUSH10 ; STLOC0 ; LDLOC0 ; RET
    //   store defines a loc0 var; the load must read that var, not stay opaque.
    let ins = vec![
        Instruction::new(0, OpCode::Push10, None),
        Instruction::new(1, OpCode::Stloc0, None),
        Instruction::new(2, OpCode::Ldloc0, None),
        Instruction::new(3, OpCode::Ret, None),
    ];
    let (ins, cfg) = linear(ins);
    let ssa = SsaBuilder::new(&cfg, &ins).build();
    let block = ssa.blocks_iter().next().expect("a block exists").1;

    // Collect loc0 definitions in order: [store, load].
    let loc0_defs: Vec<&SsaStmt> = block
        .stmts
        .iter()
        .filter(|s| matches!(s, SsaStmt::Assign { target, .. } if target.base == "loc0"))
        .collect();
    assert!(
        loc0_defs.len() >= 2,
        "expected a store def and a load def for loc0; got {:?}",
        block.stmts
    );
    // The last loc0 def is the load. Its value must reference the stored var,
    // NOT be an opaque ldloc0() Call.
    let load_def = loc0_defs.last().copied().unwrap();
    let SsaStmt::Assign { value, .. } = load_def else {
        panic!("load def should be an Assign: {load_def:?}");
    };
    assert!(
        matches!(value, SsaExpr::Variable(_)),
        "LDLOC0 after STLOC0 should read the stored var; got {value:?}"
    );
    assert!(
        !matches!(value, SsaExpr::Call { .. }),
        "LDLOC0 should not stay an opaque ldloc0() call once a store exists; got {value:?}"
    );
}
```

- [ ] **Step B2: Run the test to verify it fails**

Run: `cargo test --lib store_then_load_connects_within_a_block`
Expected: FAIL — with only Task A landed, `STLOC0` defines `loc0` but `LDLOC0` still resolves no reaching version if Step A5's load-copy branch is not yet wired (it is wired in Step A5, so this may already pass). If it already passes, keep it as a regression guard.

- [ ] **Step B3: Confirm the load-copy branch fires (only if B2 failed)**

Open `apply_instruction` and verify the `push == 1` branch reads `slots.get(name)`. For `STLOC0` followed by `LDLOC0` in the same block, the store inserts `slots["loc0"] = loc0_1`; the load then resolves `reaching = Some(loc0_1)` and emits `loc0_2 = loc0_1` (a `Variable`, not a `Call`). No further code change is expected beyond Step A5.

- [ ] **Step B4: Run the SSA suite + full-corpus panic fence**

Run: `cargo test --lib cfg::ssa && cargo test --test corpus_replay`
Expected: PASS — no panics across the corpus; existing SSA tests green.

- [ ] **Step B5: fmt + clippy + commit**

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
git add src/decompiler/cfg/ssa/builder.rs
git commit -m "test(ssa): intra-block store→load connects through the reaching slot version"
```

---

## Task C: Cross-block φ for slots

**Files:** Modify `src/decompiler/cfg/ssa/builder.rs`.

- [ ] **Step C1: Write the failing test**

Add to `mod tests` (a diamond that stores different values to the same local in each arm, then loads at the merge):

```rust
#[test]
fn diamond_places_a_phi_for_a_slot() {
    // BB0 pushes a constant, STLOC0, branches to BB1 / BB2.
    // BB1: PUSH11 ; STLOC0 ; jmp BB3
    // BB2: PUSH12 ; STLOC0 ; jmp BB3
    // BB3 (merge): LDLOC0 ; RET  →  needs a slot φ(loc0) over BB1/BB2.
    let ins = vec![
        // BB0
        Instruction::new(0, OpCode::Push1, None),
        Instruction::new(1, OpCode::Stloc0, None),
        // BB1
        Instruction::new(0, OpCode::Push11, None),
        Instruction::new(1, OpCode::Stloc0, None),
        // BB2
        Instruction::new(0, OpCode::Push12, None),
        Instruction::new(1, OpCode::Stloc0, None),
        // BB3
        Instruction::new(0, OpCode::Ldloc0, None),
        Instruction::new(0, OpCode::Ret, None),
    ];

    let mut cfg = Cfg::new();
    cfg.add_block(BasicBlock::new(BlockId(0), 0, 1, 0..2, Terminator::Branch {
        then_target: BlockId(1),
        else_target: BlockId(2),
    }));
    cfg.add_block(BasicBlock::new(BlockId(1), 1, 2, 2..4, Terminator::Jump { target: BlockId(3) }));
    cfg.add_block(BasicBlock::new(BlockId(2), 2, 3, 4..6, Terminator::Jump { target: BlockId(3) }));
    cfg.add_block(BasicBlock::new(BlockId(3), 3, 4, 6..8, Terminator::Return));
    cfg.add_edge(BlockId(0), BlockId(1), EdgeKind::ConditionalTrue);
    cfg.add_edge(BlockId(0), BlockId(2), EdgeKind::ConditionalFalse);
    cfg.add_edge(BlockId(1), BlockId(3), EdgeKind::Unconditional);
    cfg.add_edge(BlockId(2), BlockId(3), EdgeKind::Unconditional);

    let ssa = SsaBuilder::new(&cfg, &ins).build();
    let merge = ssa.block(BlockId(3)).expect("merge block exists");

    let has_slot_phi = merge.phi_nodes.iter().any(|phi| phi.target.base == "loc0");
    assert!(
        has_slot_phi,
        "merge of two STLOC0 arms should place a loc0 φ; got {:?}",
        merge.phi_nodes
    );
}
```

- [ ] **Step C2: Run the test to verify it fails**

Run: `cargo test --lib diamond_places_a_phi_for_a_slot`
Expected: FAIL — with Task A's `&SlotState::default()` entry, the merge has no slot φ (loads still opaque).

- [ ] **Step C3: Add `compute_join_slots`**

Add this method to `impl<'a> SsaBuilder<'a>` next to `compute_join_entry` (after builder.rs:212). It mirrors the stack-slot join but over `SlotState`, minting φ targets with `fresh_var` so a slot φ keeps its slot base name (`loc0_N`):

```rust
/// Compute a block's entry slot state and the φ nodes it needs, from its
/// predecessors' current exit slot states. For each slot name: if all
/// predecessors that hold it agree, the value flows through; otherwise a φ is
/// placed (target named after the slot, e.g. `loc0_N`, so the structurer's
/// `strip_version` keeps it associated with the slot).
fn compute_join_slots(
    &self,
    bid: BlockId,
    exit_slots: &BTreeMap<BlockId, SlotState>,
    versions: &mut BTreeMap<String, usize>,
) -> (SlotState, Vec<super::variable::PhiNode>) {
    use super::variable::PhiNode;
    let preds = self.cfg.predecessors(bid);
    if preds.is_empty() {
        return (SlotState::new(), Vec::new());
    }

    // Collect the reaching version each predecessor holds, per slot name.
    let mut names: BTreeSet<String> = BTreeSet::new();
    for pred in &preds {
        if let Some(state) = exit_slots.get(pred) {
            for name in state.keys() {
                names.insert(name.clone());
            }
        }
    }

    let mut entry = SlotState::new();
    let mut phis = Vec::new();
    for name in names {
        let mut operands: Vec<(BlockId, SsaVariable)> = Vec::new();
        for pred in &preds {
            if let Some(var) = exit_slots.get(pred).and_then(|s| s.get(&name)) {
                operands.push((*pred, var.clone()));
            }
        }
        if operands.is_empty() {
            continue;
        }
        let first = operands[0].1.clone();
        let all_agree = operands.iter().all(|(_, v)| *v == first);
        if all_agree {
            entry.insert(name, first);
        } else {
            let target = fresh_var(versions, &name);
            let mut phi = PhiNode::new(target.clone());
            for (pred, var) in &operands {
                phi.add_operand(*pred, var.clone());
            }
            entry.insert(name, target);
            phis.push(phi);
        }
    }
    (entry, phis)
}
```

- [ ] **Step C4: Wire slot state into the fixpoint loop**

In `build_ssa_blocks` (builder.rs:85), add slot-state work maps next to `entry_stacks`/`exit_stacks` (after builder.rs:93):

```rust
let mut entry_slots: BTreeMap<BlockId, SlotState> = BTreeMap::new();
let mut exit_slots: BTreeMap<BlockId, SlotState> = BTreeMap::new();
```

Update the per-block body of the fixpoint loop (builder.rs:108-119) to compute slot entry, execute with it, and include slot stability in `changed`:

```rust
for &bid in &block_ids {
    let (new_entry, _new_phis) = self.compute_join_entry(bid, &exit_stacks);
    let (new_slot_entry, _new_slot_phis) = self.compute_join_slots(bid, &exit_slots, &mut versions);
    let exec = self.execute_block(bid, &new_entry, &new_slot_entry, &mut versions);

    let exit_changed = exit_stacks.get(&bid) != Some(&exec.exit_stack);
    let entry_changed = entry_stacks.get(&bid) != Some(&new_entry);
    let slot_exit_changed = exit_slots.get(&bid) != Some(&exec.exit_slots);
    let slot_entry_changed = entry_slots.get(&bid) != Some(&new_slot_entry);
    if exit_changed || entry_changed || slot_exit_changed || slot_entry_changed {
        changed = true;
    }
    entry_stacks.insert(bid, new_entry);
    exit_stacks.insert(bid, exec.exit_stack);
    entry_slots.insert(bid, new_slot_entry);
    exit_slots.insert(bid, exec.exit_slots);
    block_uses.insert(bid, exec.uses);
}
```

- [ ] **Step C5: Wire slot phis into the final assembly pass**

In the final assembly loop (builder.rs:129-158), compute the slot phis alongside the stack phis and add them before the statements. Replace the body that currently does `let (_, phis) = self.compute_join_entry(...)` + `execute_block(bid, &entry, &SlotState::default(), ...)` with:

```rust
for &bid in &block_ids {
    let entry = entry_stacks.get(&bid).cloned().unwrap_or_default();
    let (_, stack_phis) = self.compute_join_entry(bid, &exit_stacks);
    let slot_entry = entry_slots.get(&bid).cloned().unwrap_or_default();
    let (_, slot_phis) = self.compute_join_slots(bid, &exit_slots, &mut versions);
    let exec = self.execute_block(bid, &entry, &slot_entry, &mut versions);

    let mut sb = SsaBlock::new();
    for phi in stack_phis.iter().chain(slot_phis.iter()) {
        definitions.insert(phi.target.clone(), bid);
        for var in phi.operands.values() {
            uses.entry(var.clone()).or_default().insert(UseSite::new(bid, 0));
        }
        sb.add_phi(phi.clone());
    }
    for (i, stmt) in exec.stmts.iter().enumerate() {
        if let SsaStmt::Assign { target, value } = stmt {
            definitions.insert(target.clone(), bid);
            for used in collect_expr_uses(value) {
                uses.entry(used).or_default().insert(UseSite::new(bid, i));
            }
        }
        sb.add_stmt(stmt.clone());
    }
    for (var, idx) in block_uses.get(&bid).cloned().unwrap_or_default() {
        uses.entry(var).or_default().insert(UseSite::new(bid, idx));
    }
    ssa_blocks.insert(bid, sb);
}
```

- [ ] **Step C6: Run the new test to verify it passes**

Run: `cargo test --lib diamond_places_a_phi_for_a_slot`
Expected: PASS — the merge has a `loc0` φ over BB1/BB2.

- [ ] **Step C7: Run the full SSA suite + corpus fence**

Run: `cargo test --lib cfg::ssa && cargo test --test corpus_replay`
Expected: PASS. If the fixpoint diverges (timeout / panic in corpus_replay), the most likely cause is φ-target instability — confirm `compute_join_slots` runs before `execute_block` each pass and that `fresh_var` order is deterministic (it is: blocks in program order, slots in BTreeMap order).

- [ ] **Step C8: fmt + clippy + commit**

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
git add src/decompiler/cfg/ssa/builder.rs
git commit -m "feat(ssa): cross-block φ for named slots at control-flow joins"
```

---

## Task D: Output updates, strengthened IR test, full verification

**Files:** Modify `tests/ir_pipeline.rs`; update any `--format ssa|ir` test whose expected output shifted.

- [ ] **Step D1: Find any SSA/IR tests whose expected output changed**

Run: `cargo test`
Capture any failures in `tests/ssa_e2e.rs`, `tests/ir_pipeline.rs`, or in-crate `cfg::ssa` / `ir` tests that assert specific `ldloc0()` / SSA text. These are expected, correct changes (loads now read reaching defs).

- [ ] **Step D2: Update affected assertions to the new, richer output**

For each failing assertion, update the expected string to the new output. Do NOT weaken assertions (e.g. don't change `contains("loc0 = 10")` to a vaguer check) unless the new output genuinely dropped information — if it did, stop and investigate (a body should not regress).

- [ ] **Step D3: Strengthen the IR switch test to assert case bodies**

In `tests/ir_pipeline.rs`, extend `ir_pipeline_recovers_a_switch_from_real_bytecode` so it also asserts the case bodies render the stored constants. After the existing `assert!(ir.contains("case "))`, add:

```rust
assert!(
    ir.contains("10") && ir.contains("11") && ir.contains("12"),
    "switch case bodies should carry the stored constants (10/11/12); got:\n{ir}"
);
```

- [ ] **Step D4: Run the strengthened test**

Run: `cargo test --test ir_pipeline ir_pipeline_recovers_a_switch_from_real_bytecode`
Expected: PASS — case bodies now carry `10`/`11`/`12` instead of rendering empty.

- [ ] **Step D5: Full verification gate**

Run each; all must pass:

```bash
cargo test
cargo test --no-default-features
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: all green; `parity.rs` output unchanged (legacy path, separate code path).

- [ ] **Step D6: Commit**

```bash
git add tests/ir_pipeline.rs   # plus any other updated test files
git commit -m "test(ir): assert switch case bodies carry stored constants after slot SSA"
```

---

## Definition of Done

- `STLOC0`/`STARG0`/`STSFld0` families define SSA versions; `LD*` families read the reaching version; joins place slot φ.
- `tests/ir_pipeline.rs` switch test asserts case bodies render `10`/`11`/`12`.
- `cargo test` (incl. `corpus_replay` panic fence), `--no-default-features`, clippy `-D warnings`, and `fmt --check` all green.
- Legacy high-level path and `parity.rs` output unchanged.
