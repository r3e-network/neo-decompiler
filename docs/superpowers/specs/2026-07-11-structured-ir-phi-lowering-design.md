# Structured IR Phi Lowering Design

**Date:** 2026-07-11
**Status:** Approved continuation of the advanced-decompiler structured-IR work

## Goal

Remove source-visible `phi(...)` pseudo-calls from structured output while preserving the exact predecessor-dependent value selection for branches, loops, malformed unequal stacks, switches, and exception regions. The analysis-facing SSA view keeps real phi nodes.

## Current Failure

SSA construction correctly records a phi target and a predecessor-keyed operand map. The structurer currently discards that edge identity and deliberately manufactures an ordinary typed-IR call:

```text
p4_0 = phi(t_1, t_2);
```

That call has no C# or Neo VM semantics. It also blocks promotion of structured IR into the C# backend. A representative diamond currently renders:

```text
if (1) {
    t_2 = 2;
} else {
    t_1 = 1;
}
p4_0 = phi(t_1, t_2);
check(p4_0);
```

The correct imperative lowering assigns the selected value on the edge that reaches the merge:

```text
if (1) {
    t_2 = 2;
    p4_0 = t_2;
} else {
    t_1 = 1;
    p4_0 = t_1;
}
check(p4_0);
```

Copy coalescing can simplify those temporaries later. This slice first establishes general, exact semantics.

## Considered Approaches

### 1. Edge-keyed parallel copies at SSA-to-IR lowering (selected)

Build a private copy plan while predecessor identity is still available. For every live phi in successor block `B`, add `target <- operand[pred]` to edge `(pred, B)`. Emit that group only when the structurer takes that exact edge. Multiple copies on one edge execute in parallel; cycles use one collision-free synthetic temporary.

This leaves `SsaForm` valid and inspectable, handles loops and arbitrary predecessor counts, and adds no source-level `phi` concept to typed IR.

### 2. Coalesce every phi web into one output name

Renaming incoming definitions, phi targets, and uses to one mutable variable produces compact output without explicit copies. It is unsafe without liveness and interference analysis: a phi operand may remain live after the join, and overlapping phi webs can overwrite values that are still needed. This is deferred until a proper copy-coalescing pass exists.

### 3. Reconstruct ternary or conditional expressions

A pure two-arm diamond can become `value = condition ? left : right`. That does not generalize to loop-carried values, more than two predecessors, unequal stacks, exception edges, or side-effecting branch conditions. It is a later presentation optimization, not a general phi elimination strategy.

### 4. Destroy SSA by mutating the CFG

Inserting copies into predecessor blocks requires splitting every critical edge and then abandoning single-assignment invariants in `SsaForm`. It would complicate analysis APIs and optimizer indexes solely for rendering. The output boundary already knows the structured edge being emitted, so mutating analysis state is unnecessary.

## Components

### `cfg/phi_lowering.rs`

A new private module owns two responsibilities:

1. Build edge copy groups from live `SsaBlock::phi_nodes`, preserving the authoritative predecessor map.
2. Schedule each group as parallel assignments after source names are applied.

The plan includes every real CFG predecessor. If a malformed or hand-built phi omits an operand for a real predecessor, that edge receives the existing unknown value `?`. The virtual predecessor `BlockId(usize::MAX)`, used for entry loops, is stored separately as entry initialization.

Final-name no-op copies are removed. Acyclic copies are ordered so no destination overwrites a source still needed by another copy. For a cycle such as `a <- b, b <- a`, lowering emits:

```text
_copy_tmp_0 = b;
b = a;
a = _copy_tmp_0;
```

Temporary names are checked against every lowered SSA/source name and against other copy temporaries.

### `cfg/structure.rs`

`StructCtx` receives the copy plan and stops emitting phi calls. Edge copies are placed as follows:

- a block with one successor appends that edge's copies after its ordinary statements;
- normal branch arms prepend copies for the header-to-arm edge, including a direct critical edge to the merge;
- a while body prepends the header-to-body copies, and the false exit copies appear immediately after the loop;
- latch-to-header copies are emitted at the bottom of the loop body;
- a do-while with backedge copies uses a collision-free first-iteration guard, applying them at the start of subsequent iterations only; exit copies appear after the loop;
- switch cases/default and try/catch/finally regions prepend copies for their actual incoming edge;
- a shared `ENDTRY` block is emitted once after `try`/`catch`/`finally`, including its continuation copies;
- virtual-entry copies execute once before structuring the entry block.

This placement keeps copy execution path-sensitive without introducing edge objects into typed IR.

### `cfg/ssa/optimize.rs`

Trivial phi substitution currently leaves the original node behind, causing repeated nominal rewrite rounds and stale operand liveness. The optimizer will remove a phi once its target has a substitution, prune dead phi nodes whose targets have no uses, and rebuild indexes. A second optimization call must then return zero.

This cleanup is independent of general lowering: nontrivial live phis remain in SSA and are lowered only at the structured output boundary.

## Data Flow

```text
SsaForm with predecessor-keyed PhiNode values
    -> optimize trivial/dead phis only
    -> PhiLowering::new(ssa, source_names)
    -> edge-keyed parallel copy groups
    -> StructCtx emits copies on the exact traversed edge
    -> typed IR contains assignments/control flow, never phi calls
    -> existing IR renderer
```

The analysis-only `render_ssa_form` continues to print Greek `φ` with predecessor labels. That output is intentionally compiler-facing rather than source-facing.

## Error Handling

- Missing stack values remain explicit `?` assignments on only the short edge.
- Missing real-predecessor operands in malformed phi nodes also become `?`.
- A phi with no live target produces no copies.
- Copy cycles always terminate through a synthetic temporary.
- A virtual predecessor is valid only as entry initialization; it is never treated as a real CFG edge.

No panic or new public error type is introduced for malformed bytecode.

## Tests

Test-first coverage will include:

1. the existing diamond integration fixture emits one assignment in each branch and no `phi(`;
2. unequal stack heights assign `?` only on the short edge and keep the merged call argument defined;
3. a direct branch-to-merge critical edge keeps its copy inside that arm;
4. two acyclic copies preserve old source values;
5. a two-copy cycle uses one temporary and preserves parallel semantics;
6. while preheader and latch copies appear outside/bottom of the loop respectively;
7. virtual-entry initialization remains distinct from a self-backedge update;
8. switch and try-region entry copies stay within the selected case/handler/finally region;
9. a shared `ENDTRY` continuation copy appears once after all protected regions;
10. trivial/dead-SCC phi optimization removes obsolete nodes, rebuilds definitions/uses, and converges even for long copy chains;
11. SSA text may still contain `φ`, while structured output contains neither `φ` nor `phi(`.

The full Rust, JavaScript, web, lint, formatting, dependency-policy, schema, declaration, and diff gates remain the completion fence.

## Non-Goals

- Interference-based phi-web coalescing or eliminating every temporary copy.
- Rewriting simple diamonds into ternary expressions.
- Adding variable declarations or final C# types for synthetic merged values.
- Changing SSA construction, phi placement, or the public analysis-facing SSA renderer.
- Migrating C# rendering to structured IR in the same slice; migration follows after this prerequisite is verified.
- Changing JavaScript, whose high-level path does not expose this Rust SSA representation.
