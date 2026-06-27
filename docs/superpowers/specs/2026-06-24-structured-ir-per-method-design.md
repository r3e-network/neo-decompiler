# Per-Method Structured IR + Contract Envelope

**Status:** Design approved 2026-06-24; spec written; implementation pending.

## Problem

`Decompilation::render_structured_ir()` currently runs the CFG structurer
on the whole-contract `Cfg` / `SsaForm` (a single CFG spanning every method's
instructions concatenated). For multi-method contracts the output collapses to
a single comment (e.g. `MultiMethod.nef` → `// return/throw/abort at
BlockId(0)`), so `--format ir` can't represent a contract as a collection of
methods inside a contract envelope. This is the remaining gap before the IR
view can reach parity with the legacy high-level path and become the default.

## Goal

Make `--format ir` render a real per-method structured-IR view of a contract:

```
contract Name {
    // script hash (…): …
    // compiler: …
    features { … }
    trusts = …;
    // ABI methods
    fn main() -> int;
    fn helper() -> int;

    fn main() -> int {
        …
    }

    fn helper() -> int {
        …
    }
}
```

Each method body carries the typed data flow produced by the existing SSA +
structurer (#1–#3 of the parity project).

## Approach (A — sliced sub-CFGs)

Keep the existing single whole-script `Cfg` / `SsaForm` (the legacy pipeline,
`call_graph` / `xrefs` / `types` / `high_level` / `csharp` all share it) and
**extract per-method sub-CFGs at render time**. For each method's
instruction-offset range (from `MethodTable`), select the blocks whose
`instruction_range` lies within the range, prepend a synthesised entry, and
synthesise method-return terminators where the original had a cross-range
jump. Run `SsaBuilder` + `optimize_ssa` + `structure_cfg` per sub-CFG and
wrap the resulting methods in a contract envelope built from
`self.nef` / `self.manifest`. Long-term, the pipeline can move to producing
per-method CFGs upfront (approach B in the brainstorm); this spec implements
A as the contained, low-risk path that reuses the working single-CFG
pipeline and delivers real per-method IR.

### Inter-method boundaries

Edges whose target address lies outside a method's range are rewritten so the
sub-CFG stays self-contained:

- `CALL` / `CALLA` / `CALLT` to another method's entry → kept in the
  sub-CFG's instruction slice (renders as an opaque call expression in the
  body); control synthesises a return after the call.
- `JMP` / `JMPIF*` / `JMPIFNOT*` to an address outside the range →
  synthesised `Return` terminator on the source block.
- `RET` → kept; renders as a structured `return`.

### Envelope

Reuse the legacy envelope shape for consistency: contract name, script hash
(both endians), compiler, `features { … }`, `trusts = …`, ABI method table,
then the per-method bodies. The renderer reads `self.nef` (name + hash) and
`self.manifest` (features, trusts, ABI methods) — already present on
`Decompilation`.

## Architecture

- `Decompilation::render_structured_ir()` becomes the per-method + envelope
  renderer. The existing whole-script path stays as a private
  `render_structured_ir_single_cfg()` used for the fallback and diagnostics.
- New module `crate::decompiler::cfg::method_view` (or
  `crate::decompiler::ir::method_view`):
  - `extract_method_cfgs(cfg: &Cfg, table: &MethodTable) ->
    Result<Vec<MethodView>, MethodViewError>`. `MethodView { method: MethodRef,
    cfg: Cfg, instructions: Vec<Instruction> }`.
  - `synthesize_method_returns(cfg: &mut Cfg, range: Range<usize>)`: rewrites
    cross-range `Jmp*` / `Jmpif*` / `Jmpifnot*` terminators to `Return`.
  - `build_sub_cfg(cfg: &Cfg, range, entry: BlockId) -> (Cfg, Vec<Instruction>)`:
    selects blocks, prepends a fresh entry block, returns the sub-CFG and the
    sliced instructions.
  - `render_method_body(view: &MethodView) -> String`: builds SSA, optimises,
    structures, renders `fn name() { ... }`.
  - `render_envelope(decomp: &Decompilation, methods: &[MethodView]) -> String`:
    contract header + ABI + bodies.

### Data flow

1. `Decompilation::render_structured_ir()` builds `MethodTable::new(&self.instructions, self.manifest.as_ref())`.
2. Calls `extract_method_cfgs(&self.cfg, &table)` → `Vec<MethodView>`.
3. For each `MethodView`: `SsaBuilder::new(&view.cfg, &view.instructions).build()`,
   `optimize_ssa`, `structure_cfg`, render the body.
4. Compose the envelope: contract name + hashes + features + trusts + ABI list,
   then each method body.
5. If step 2 fails (empty table, parse error) → fall back to the existing
   single-CFG render wrapped in a minimal envelope (or no envelope), so the
   view never regresses.
6. If an individual method's SSA build fails → render that method as a
   placeholder `fn name() { /* unparseable */ }` so the envelope and other
   methods still render.

### Compatibility

- Legacy `high_level` / `csharp` / `pseudocode` / `json` / `web` outputs are
  untouched: they use the existing whole-script `Cfg` / `SsaForm` unchanged.
- The whole-script `SsaForm` on `Decompilation` is still built and used by
  `render_optimized_ssa`; the IR view no longer uses it (per-method SSAs).
- `--format ssa` keeps rendering the whole-script optimised SSA (unchanged).

## Risks & mitigations

| Risk | Mitigation |
|---|---|
| Sub-CFG slicing drops a block (range boundary, off-by-one) | Unit tests on synthetic 2-method CFGs; corpus replay must stay panic-free. |
| Cross-range `CALL*` not recognised as opaque (renders as a structurer stumble) | Unit test: a method that calls another → body shows the call as an opaque expr, control returns. |
| Envelope diverges from legacy format | Reuse the legacy envelope's exact field set and formatting; add a corpus test that asserts both render the same header for a known contract. |
| Fallback path never triggers in practice (silent regression) | Add a test that asserts MultiMethod's IR contains both `fn main` and `fn helper`. |
| Per-method SSA diverges from the single-script SSA's data flow | Acceptable: the IR view is now per-method; the whole-script SSA is still available via `--format ssa`. |

## Verification

- Unit (`cfg::method_view`): `extract_method_cfgs` on a synthetic diamond +
  cross-method `CALL`; `synthesize_method_returns` on a block with a cross-range
  jump; `render_method_body` round-trips.
- Integration (`tests/ir_pipeline.rs`): MultiMethod renders `fn main` + `fn
  helper` inside a contract with the ABI methods listed; LoopIf still
  renders `while(...)` (no regression); a single-method artifact renders its
  one method inside the envelope.
- Regression: full suite (519+ tests) green, clippy `-D warnings`, fmt,
  `--no-default-features`, corpus replay panic-free.

## Out of scope

- Approach B (per-method CFGs from the pipeline) — separate, later.
- Inlining called methods into the caller — never.
- Changing the legacy envelope format — reuse it.
- `for`-loop recovery / default-output switch — separate gaps.