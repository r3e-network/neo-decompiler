# Neo N3 Decompiler — Codebase Review (2026-06-24)

## TL;DR

The codebase is reasonably well-structured and does **not** need a refactor right
now. The high-leverage work is **not** a refactor — it is a parity project for
the `--format ir` path, which is currently a research/WIP view far behind the
legacy high-level path. The big-ticket refactors named in the design spec (retire
the god-object `HighLevelEmitter`, delete the ~95 KB string `postprocess` suite,
collapse `csharpize`) are all **gated on IR-becomes-default**, which is not
imminent. Two confirmed dead-code items were removed during this review; the
rest is documented below as a roadmap.

## Methodology

- Largest-files survey (LOC) to find god objects.
- Targeted dead-code audit against the spec's Phase-5 named suspects.
- Duplication audit (legacy emitter vs IR structurer; C# renderer vs high-level).
- `--format ir` vs `--format high-level` output diff on real artifacts to assess
  parity (the precondition the big refactors wait on).
- Gates run throughout: `cargo test` (incl. full-corpus panic fence),
  `--no-default-features`, `cargo clippy -D warnings`, `cargo fmt --check`.

## Findings

### Code health: solid

~36.7 K LOC, 437+ tests, modular (`cfg`, `cfg/ssa`, `ir`, `high_level`,
`analysis`, `csharp`, `nef`, `cli`, `web`). Responsibilities are clear; clippy
`-D warnings` and fmt are clean; the full-corpus replay is a strong panic fence.
This is not a codebase in distress.

### Dead code — removed in this review ✅

Two items the spec named, now deleted (zero behavior change; full suite green):

- **`Stmt::Unlifted`** (`93b7cc7`) — defined with a constructor and render arm
  but constructed nowhere outside its own unit test.
- **`build_ssa_from_cfg` + `SsaConversion`** (`c5f9a75`) — the vestigial
  pre-rewrite "skeleton" SSA: returned an empty `SsaForm` (no stmts, no φ), used
  only as a redundant panic fence. The real `SsaBuilder` is the canonical entry
  point and is now re-exported.

### God object: `HighLevelEmitter` (deferred)

`pub(crate) struct HighLevelEmitter` (`high_level/emitter/mod.rs:35`) carries
**30+ fields** of mutable state (eval stack, statements, ~10 target/label maps,
loop stacks, init sets, pointer-value caches…) with `impl` blocks spread across
10+ files. It is the legacy string-based emitter and the **current default
path**. Splitting it is risky (deeply shared mutable state) and the spec
explicitly says to **retire** it once the IR path is default — not split it in
place. **Defer until IR-default.**

Largest non-generated, non-test source files (refactor candidates only if
tackled): `csharp/helpers.rs` 45 KB, `cfg/ssa/builder.rs` 44 KB,
`cfg/structure.rs` 40 KB, `analysis/call_graph.rs` 31 KB,
`high_level/emitter/postprocess/{switches,simplify,overflow_collapse,while_loops}.rs`
(~95 KB combined).

### Duplication: intentional parallel paths (defer)

The legacy `postprocess/*` string passes (switch/loop/overflow reconstruction by
regex-like text matching) overlap in *intent* with the CFG-based IR structurer.
This is the deliberate "two paths coexist until parity" posture from the spec —
not accidental duplication. Deduping now would be throwaway. **Defer until
IR-default.**

### The real gap: `--format ir` is far from parity 🔴

This is the single most important finding, because it gates every big refactor.
On real artifacts:

| Artifact | Legacy `--format high-level` | `--format ir` |
|---|---|---|
| `LoopIf.nef` | `loop { let loc0 = 0; if loc0 < 3 { loc0 += 1; } }` | `loc0_0 = 0;` / `if (loc0_0) {` / `t_4 = (0 + 1);` |
| `MultiMethod.nef` | `fn main(){return 1;}` + `fn helper(){return 1+1;}` + contract envelope | `// return/throw/abort at BlockId(0)` |

Concrete IR-path gaps observed:

1. **No multi-method splitting / contract envelope.** The IR view renders one
   method's spine with no `contract { }`, ABI method table, features/trusts, or
   method splitting. It cannot be the user-facing default as-is.
2. **Loop recovery unreliable.** `LoopIf`'s back-edge loop is not recovered at
   all (flat output, no `while`/`loop`).
3. **Condition extraction drops comparisons.** `if loc0 < 3` becomes
   `if (loc0_0)` — the `< 3` is lost (the condition picks the wrong reaching def
   or the compare is folded away).
4. **Aggressive const-folding destroys recoverable structure.** Because locals
   now flow as SSA values, `loc0 = 0` lets the optimizer fold `loc0 < 3` and
   `loc0 + 1` to constants, dissolving loops/arithmetic the structurer needs.
   This is correct optimization but wrong for a *decompiler* — it erases the
   structure the user wants to read. (Already recorded as a follow-up in
   `docs/superpowers/specs/2026-06-24-ssa-slot-modeling-design.md`.)

The IR structurer unit tests pass (hand-built SSA), but the *pipeline* (real
bytecode → SSA → optimize → structure → render) does not yet produce output
comparable to the legacy path. Phase 4 in the spec is marked "infrastructure
shipped"; the parity work to make it default-ready is the bulk that remains.

## Roadmap

### Done in this review

- Dead-code removal: `Stmt::Unlifted`, `build_ssa_from_cfg`/`SsaConversion`.

### The lever — IR-path parity project (the enabling prerequisite)

This is the work that unlocks all the big refactors. Approximate scope, ordered
by leverage:

1. **Tame the optimizer for decompilation.** ✅ Shipped (`9e869d7`) — slot
   variables (loc/arg/static) now stay symbolic in the optimizer instead of
   being const-propagated into uses, so branch conditions and loop-carried
   arithmetic survive (LoopIf's `loc0 < 3` and `loc0 + 1` no longer fold to
   constants). Slot-to-slot load-aliases still collapse; temps still fold.
2. **Condition extraction.** ✅ Shipped (`d58e284`) — plain `if`-branches now
   inline the comparison into the condition head when the last def is a
   relational/equality Binary (mirrors the switch path's `extract_eq_cond`).
   The body suppresses the condition def to avoid duplication. LoopIf now
   renders `if ((loc0_0 < 3))` instead of `if (t_2)`.
3. **Loop recovery on real bytecode.** ✅ Shipped (`dca5b3e`) — the dominance
   walks (depth_in_dominator_tree / find_common_dominator / compute_df) used
   the raw idom map, where the entry's sentinel self-idom caused walks to
   spin and clamp to equal depth, collapsing every LCA to the entry. A latch
   reachable only via its header was mis-reported as dominated by the entry
   itself, so `compute_loop_headers` produced an empty set and the structurer
   never took the while path. An `idom_parent` helper treats the entry's
   self-idom as no-parent in the walks (kept as `Some(entry)` in the raw map
   so `intersect_dominators` still recognises the entry as a predecessor's
   dominator). LoopIf now renders
   `while ((loc0_0 < 3)) { ... }`.
4. ~~**Contract envelope + method splitting for the IR view**~~ **SHIPPED** (2026-06-27,
    commits `97e38ed`→`f4d4154`). `Decompilation::render_structured_ir` now
    composes the legacy `contract { ... }` header (reused verbatim) with a
    per-method body (`fn name() -> ret { ... }`) for every reachable method in
    `MethodTable`, closing the envelope with `}`. `extract_method_cfgs` slices
    the whole-script CFG into per-method sub-CFGs, synthesises a fresh-id entry,
    and rewrites cross-range jumps to `Return` so cross-method edges don't leak.
    Fallback to the single-CFG render preserves the previous view when no
    methods are detected. E2E `tests/ir_pipeline.rs` guards `MultiMethod` and
    `LoopIf` envelopes. Reachability note: dead-code methods (helper unreachable
    after a `RET`) are correctly omitted from the body — the IR view is
    dataflow-based, so the envelope ABI list and the rendered bodies can
    legitimately differ. Gate: `cargo test`, `cargo test --no-default-features`,
    clippy `-D warnings`, fmt, corpus replay — all green.
5. **Corpus-wide parity diff** as the gate: `--format ir` output structurally
    matches `--format high-level` across `TestingArtifacts`.

### Later (gated on IR-default)

- Retire `HighLevelEmitter` (the 30+-field god object).
- Delete the legacy `postprocess/*` string suite (~95 KB).
- Collapse `csharpize_statement` to a type-name + attribute map.

## Recommendation

Sequence the work as: **(a) optimizer-taming (#1) → (b) condition/loop recovery
(#2/#3) → (c) envelope decision (#4) → (d) corpus parity gate (#5)**. Each is
independently shippable with the full suite as the fence. Only after (d) passes
should the big refactors (god object, postprocess suite) begin — before that,
they are premature and largely throwaway.

The immediate next action is **(a) the optimizer-taming / branch-fold fix**: make
the SSA optimizer stop folding branch conditions and loop-carried values that
erase structure. This is a focused, single-module change (`cfg/ssa/optimize.rs`)
with clear test cases (the `LoopIf` output above is the failing test).
