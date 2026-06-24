# Neo N3 Decompiler — SSA over Named Slots

**Status:** Design approved 2026-06-24; implementation pending.
**Risk posture:** Contained, additive change inside `cfg::ssa`. The 519-test suite
(including the full-corpus replay panic fence) is the regression fence throughout.
Output of `--format ssa|ir` changes deliberately; the legacy high-level path and
`parity.rs` are untouched.

## 1. Problem

The structured-IR pipeline (`--format ir`, the Phase-4 IR spine) recovers
`if`/`while`/`do-while`/`try`/`switch` correctly, but the **bodies render empty**
whenever they store then load a local/arg/static slot. Example: a real C# switch

```
if (loc0 == 0) loc0 = 10; else if (loc0 == 1) loc0 = 11; else loc0 = 12;
```

structures as `switch (loc0) { case 0:  case 1: }` with empty cases.

Root cause: the SSA builder models slot opcodes only by stack effect
(`cfg::ssa::effects`):

- **Load** (`Ldloc0`/`Ldarg`/`Ldsfld` family) → `(0, 1)`. It mints a fresh
  `loc0_N` whose value is the opaque call `ldloc0()` — a read "from nowhere".
- **Store** (`Stloc0`/`Starg`/`Stsfld` family) → `(1, 0)`. It pops the value and
  emits **no statement** — the stored value flows nowhere.

So a store is a sink and a load is an opaque source; they are never connected,
and locals/args/statics are not versioned SSA variables. This is the single
biggest blocker to the stated Phase-4 goal of promoting the IR path to the
default ("`--ir` rollout at parity"): the legacy high-level path renders full
bodies because it works at the instruction level and never goes through SSA.

## 2. Goal

Make named slots (locals, args, statics) **first-class versioned SSA variables**:
a store defines a new version, a load reads the reaching version, and joins place
φ where predecessors disagree. This makes straight-line and control-flow bodies
carry their real data flow, so the IR path produces output comparable to the
legacy path and can be promoted to default.

## 3. Design

### 3.1 Approach

Mirror the SSA builder's existing **stack-slot** φ machinery for **named slots**.
The builder already maintains `exit_stacks: {block → symbolic-stack}` and places
φ at joins (`compute_join_entry`, `cfg::ssa/builder.rs:167`). Named slots get an
analogous parallel structure. No new concepts; the proven fixpoint + φ design is
reused.

Rejected alternatives:
- **Unified "memory location" SSA** (stack + slots in one framework): elegant but
  a large refactor of a working, heavily-tested builder — scope creep and
  regression risk disproportionate to the goal.
- **Post-hoc optimizer pattern-match** to reconnect store→load: infeasible, the
  builder discards the stored value before the optimizer runs.

### 3.2 Data structures

A slot state is a map from slot name (`"loc0"`, `"arg1"`, `"static2"`) to the
reaching SSA variable:

```rust
type SlotState = BTreeMap<String, SsaVariable>;
```

Alongside the existing `exit_stacks` / `entry_stacks`, the builder keeps:

- `entry_slots: BTreeMap<BlockId, SlotState>`
- `exit_slots:  BTreeMap<BlockId, SlotState>`

`SsaForm` exposes no new public field — slots are materialised as ordinary
`SsaStmt::Assign` and `PhiNode`s, so the rest of the pipeline (optimizer,
`to_ir`, structurer) needs no changes to see them.

### 3.3 Block execution (`execute_block`)

`execute_block` carries a per-block `slots: SlotState` seeded from
`entry_slots[bid]`.

- **Store** (`Stloc0`/`Starg`/`Stsfld` and their `N`-operand forms): pop the
  value variable `v`; derive the slot name; mint a fresh slot variable `loc0_N`
  via `fresh_var(versions, "loc0")`; emit `Assign(loc0_N, Var(v))`; set
  `slots["loc0"] = loc0_N`.
- **Load** (`Ldloc0`/`Ldarg`/`Ldsfld` and their `N`-operand forms): derive the
  slot name.
  - If `slots` has a reaching version `r`: mint a fresh `loc0_M`, emit
    `Assign(loc0_M, Var(r))` (a copy), push `loc0_M`.
  - If **uninitialised** (read-before-write, or a function-input arg before any
    store): fall back to the current opaque form `loc0_M = ldloc0()` (safe; this
    is also the correct semantics for an uninitialised/external value).
- On block end: `exit_slots[bid] = slots`.

The existing `slot_name_for` helper already maps load opcodes to `loc`/`arg`/
`static` + index; a sibling helper maps the store opcodes (it currently only
covers loads).

### 3.4 Join handling (`compute_join_slots`)

A twin of `compute_join_entry`, operating over slot states:

```rust
fn compute_join_slots(
    &self,
    bid: BlockId,
    exit_slots: &BTreeMap<BlockId, SlotState>,
) -> (SlotState, Vec<PhiNode>)
```

For each slot name present in any predecessor's exit state: gather operands; if
all agree, the value flows through; if they disagree, place a φ whose target is
a fresh slot-name variable (`loc0_N`), operands keyed by predecessor. The result
seeds `entry_slots[bid]`. φ operands are recorded as uses at the block head (stmt
index 0), exactly as stack-slot φ are.

### 3.5 Fixpoint

`exit_slots` is added to the existing fixpoint iteration and must stabilise
alongside `exit_stacks`. The convergence argument is unchanged: `fresh_var`
draws versions from a per-pass counter that increments in deterministic
(block-id, instruction) order, so a given def-site always receives the same
version, hence the same slot identity, across passes — the slot-state maps reach
a fixed point. The loop's stability check is extended to require both `exit_stacks`
and `exit_slots` to be unchanged before termination.

### 3.6 Optimizer interaction (falls out for free)

`cfg::ssa::optimize` already handles arbitrary `Assign`/φ:

- **Copy propagation** folds the `loc0_M = loc0_N` load-copies, so redundant
  loads collapse to the reaching def.
- **Constant propagation** flows `loc0 = 10` forward through loads and φ.
- **Dead-code elimination** drops stores whose version is never read (directly or
  via φ) — matching the legacy path's "don't show dead assignments".

No optimizer changes are required for correctness; the slot assignments are just
ordinary SSA defs. (Verify the existing φ-aware DCE treats slot φ correctly.)

### 3.7 Statics — method-local approximation

Statics (`Ldsfld`/`Stsfld`) are contract-global: any `CALL`/`SYSCALL` may
clobber them. This design models them **method-locally** — a static's reaching
version is updated only by in-method `Stsfld`, and calls are assumed not to
clobber. This is a documented approximation; locals and args are exact. A
follow-up could invalidate reaching static versions across calls if/when
call-side-effect modelling lands.

### 3.8 Naming and output

- Before: `loc0_0 = ldloc0()` (opaque); stores invisible.
- After: `loc0_1 = 10` (store); `loc0_2 = loc0_1` (load). After optimization:
  copies fold and uses see `10` / `loc0_1`.

Only `--format ssa` and `--format ir` output changes (strictly more accurate).
The legacy high-level / C# / JSON paths are unaffected. Affected SSA/IR tests
are updated to the new output.

## 4. Phased plan

Each phase is independently committable and keeps the suite green.

- **Phase A — Store opcodes define slot versions.** `slot_name_for` gains a
  store sibling; `execute_block` emits `loc0_N = v` on stores and updates the
  per-block slot map. No load changes yet (loads still opaque); no φ yet. Unit
  test: a store shows up as an assignment in straight-line SSA.
- **Phase B — Loads read the reaching version (intra-block).** `execute_block`
  resolves a load against the per-block slot map, emitting the copy form;
  uninitialised reads keep the opaque fallback. Unit test: store-then-load in
  one block connects; const-prop flows the value.
- **Phase C — Cross-block φ for slots.** Add `entry_slots`/`exit_slots`,
  `compute_join_slots`, and extend the fixpoint stability check. Unit tests:
  a diamond places a slot φ; a loop header merges the latch's updated version.
- **Phase D — Tests, corpus, output updates.** Update affected `--format ssa|ir`
  tests to the richer output; strengthen `tests/ir_pipeline.rs` to assert the
  switch case bodies (`loc0 = 10/11/12`) appear; confirm the full corpus replay
  stays panic-free and `parity.rs` is unchanged.

## 5. Risks & mitigations

| Risk | Mitigation |
|---|---|
| Fixpoint no longer converges with extra slot state | Same deterministic-`fresh_var` argument; extend stability check to `exit_slots`; the corpus replay's `catch_unwind` fence catches divergence as a panic/timeout. |
| φ explosion blows up SSA size | Bounded by (#slots × #joins). Locals/args are bounded by `Initslot`/`Initsslot` counts; statics are few. Trivial phi elimination (already present) collapses single-operand φ. |
| DCE removes a store the legacy path would keep | Only stores with no reaching read are dropped — semantically dead. If a body regresses, the test fence surfaces it. |
| Statics clobbered by calls modelled inaccurately | Documented method-local approximation; revisit when call-side-effects land. |
| Output change breaks downstream `--format ssa` consumers | Change is additive in information and semantically more correct; called out in the CHANGELOG at release. |

## 6. Verification

- **Unit** (`cfg::ssa`): intra-block store→load connects; diamond places slot φ;
  loop header merges; dead store DCE'd; constant flows through slot; uninitialised
  read stays opaque.
- **Integration** (`tests/ir_pipeline.rs`): the switch case bodies render
  `loc0 = 10/11/12`; an `if` body renders its assignment.
- **Regression**: `tests/corpus_replay.rs` stays panic-free; full `cargo test`
  green; `parity.rs` output unchanged; `cargo clippy -D warnings` and
  `cargo fmt --check` clean; `--no-default-features` builds.

## 7. Out of scope

- Call-clobbering of static fields across `CALL`/`SYSCALL`.
- Seeding `arg`/`loc` initial versions from `Initslot`/`Initsslot` counts (cosmetic;
  uninitialised reads already render a correct opaque load).
- Promoting `--format ir` to the default output (a separate decision after this
  lands and output is reviewed).
- `for`-loop recovery in the structurer (separate, listed in the parent spec).
