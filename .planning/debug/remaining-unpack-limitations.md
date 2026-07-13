---
status: investigating
trigger: "continue analysis and resolve the nine remaining pinned-corpus limitations"
created: 2026-07-13T00:00:00+08:00
updated: 2026-07-13T15:52:45+08:00
---

## Current Focus

hypothesis: One-level constant-index field postconditions plus unanimous private-entry/static facts can carry Array(2) from Reentrancy constructor arg0[0] through static0 into all three remaining fixed-shape UNPACK sites without inferring arity from consumers.
test: Infer only non-escaping per-argument indexed writes, intersect every non-null static writer and resolved incoming call, then seed PICKITEM results while preserving null/runtime faults and invalidating conflicting, dynamic-index, resizing, or opaque paths.
expecting: Contract_Reentrancy@0272 and @02AE move from incomplete to conservative, yielding Exact 1106 / Conservative 70 / Incomplete 3; Foreach and both Enum methods remain fail-closed until their separate models are addressed.
next_action: Add bounded indexed-shape facts and negative tests for conflicting writes, dynamic indexes, partial paths, static conflicts, alias escape, and unknown callsites before wiring the Reentrancy fixed point.
reasoning_checkpoint: null
tdd_checkpoint: null

## Symptoms

expected: Every pinned v3.10.0 contract whose runtime collection arity is statically provable should decompile exactly, while genuinely variable UNPACK sources remain fail-closed.
actual: The corpus has nine incomplete methods: seven standalone UNPACK roots and two enum loop-stack roots. Four standalone roots have bounded recovery paths; three Reentrancy roots need field/static summaries; Foreach is genuinely variable.
errors: UNPACK source is not a direct unmodified PACK or PACKSTRUCT definition; loop stack joins lose exact enum parse values.
reproduction: Run the pinned v3.10.0 Roslyn corpus census with /tmp/devpack-artifacts-v3.10.0 and inspect Contract_Returns.mix, Contract_Tuple.t1, Contract_NEP11.transfer, Contract_Record.test_DeconstructRecord, Contract_Reentrancy helpers, Contract_Foreach, and the two Contract_Enum parse-ignore-case methods.
started: Remaining after the 2026-07-13 structured C# corpus fixes at commit 858d850.

## Eliminated

- hypothesis: Clear invalidated_collection_roots between SSA fixpoint passes so call-return shapes can be rediscovered.
  evidence: This recovers the shape experiments but breaks loop_backedge_mutation_invalidates_header_collection_provenance, allowing a later loop mutation to unsafely validate an earlier header UNPACK.
  timestamp: 2026-07-13T00:00:00+08:00

- hypothesis: Infer Reentrancy UNPACK arity from syscall consumers or the observed two-value prefix.
  evidence: A three-element runtime source changes the storage key and leaves a tail value across RET, so consumer-driven arity two is unsound.
  timestamp: 2026-07-13T00:00:00+08:00

## Evidence

- timestamp: 2026-07-13T00:00:00+08:00
  checked: Pinned corpus census at commit 858d850.
  found: Exact 1103, Conservative 67, Incomplete 9. The incomplete roots are Foreach 04AC; NEP11 025D; Record 00E5; Reentrancy 0272/02AE; Returns 0084; Tuple 0007; and Enum 0057/00F1.
  implication: The residual set is small enough for focused soundness work and exact before/after accounting.

- timestamp: 2026-07-13T00:00:00+08:00
  checked: Fixed call-return shape experiment.
  found: Returns.div proves Struct(2) and Tuple.getResult proves Struct(4), and the shape reaches each resolved call before persistent method-wide invalidation later poisons it.
  implication: Shape-only method summaries plus flow-sensitive invalidation are the shared architectural path; fixed return shapes are the smallest independently testable slice.

- timestamp: 2026-07-13T12:10:00+08:00
  checked: Debug knowledge base, project-local skills, and common bug patterns.
  found: No .planning/debug/knowledge-base.md and no .codex/skills or .agents/skills are present; the relevant generic category is a missing data-shape/API contract rather than environment, timing, or coercion behavior.
  implication: Investigation should derive the exact interprocedural shape contract from repository code and tests without importing a broader prior inference rule.

- timestamp: 2026-07-13T12:18:00+08:00
  checked: Method-contract, call-contract, SSA definition-fact, and UNPACK implementations plus pinned sources/manifests.
  found: MethodContract and CallContract carry return existence but no collection shape; known calls define an ordinary SsaExpr::Call; UNPACK accepts only a locally resolved unmodified Array/Struct definition. Contract_Returns.div and Contract_Tuple.getResult are ABI-declared Array-returning methods, not private inference candidates.
  implication: Shape inference must examine all method views (including manifest-declared producers), flow an optional exact shape into only resolved internal CallContracts, and attach that fact to the fresh call-result definition without treating manifest Array alone as proof.

- timestamp: 2026-07-13T12:19:00+08:00
  checked: Per-block collection invalidation dataflow against focused provenance tests and the pinned corpus.
  found: Predecessor-unioned invalidation state preserves internal-call and loop-backedge mutation negatives while preventing a later acyclic call from poisoning an earlier UNPACK. The census moved from Exact 1103 / Conservative 67 / Incomplete 9 to Exact 1103 / Conservative 68 / Incomplete 8; Contract_NEP11@025D is no longer incomplete.
  implication: Persistent method-global invalidation was the confirmed NEP11 root cause. Flow-sensitive state is sound on the existing mutation regressions and is a prerequisite for call-return shape facts.

- timestamp: 2026-07-13T12:21:00+08:00
  checked: Pinned disassembly and structured IR for Contract_Returns and Contract_Tuple.
  found: div@0x006A builds PACKSTRUCT(2) at 0x0082 on both normal branches and mix calls it at 0x0089 before UNPACK 0x008C; getResult@0x0000 builds PACKSTRUCT(4) at 0x0005 and t1 calls it at 0x001E before UNPACK 0x0021. Both callers currently emit unpack(call_result), unknown consumers, and the exact MissingProvenance/underflow warnings predicted by the hypothesis.
  implication: The producer summaries are locally provable from runtime bytecode and the first divergence is precisely the absent call-result shape fact, not call-target resolution, pack decoding, or consumer inference.

- timestamp: 2026-07-13T12:21:00+08:00
  checked: Concurrent per-block invalidation diff and SSA optimizer handling of Call/Index definitions.
  found: Invalidation sets now flow per block and union at joins; internal calls invalidate existing roots before defining their fresh result. The optimizer substitutes literals/copies and folded unary/binary expressions, but does not substitute Call or Index assignments.
  implication: The call result must be registered as a new collection root after call-side invalidation. Later path mutations/calls can then invalidate it normally, backedges remain conservative, earlier calls do not poison it retroactively, and one retained call definition can safely feed multiple runtime indexes.

- timestamp: 2026-07-13T12:43:00+08:00
  checked: Fixed return-shape implementation against focused inference/order/mutation tests, full Rust gates, pinned fidelity census, generated C#, and Roslyn.
  found: All reachable unmodified returns must agree on Array/Struct kind and length; resolved internal calls carry that optional fact into a fresh DefinitionFact; UNPACK emits one index per element in VM order without duplicating the call. Exact rose from 1103 to 1105 and Incomplete fell from 8 to 6 after the prior NEP11 slice. Contract_Returns.mix and Contract_Tuple.t1 now use runtime indexes, and Roslyn compiled all 103 pinned contracts. The oversized-method test initially exposed an 82-second regression; honoring the existing 16384-instruction lift cap restored it to 0.42 seconds.
  implication: The return-shape hypothesis is confirmed and bounded by mutation, mixed-return, loop, and size-cap negatives. The remaining incomplete roots are Record, three Reentrancy UNPACK sites across two helpers, Foreach, and two Enum loop-stack methods.

- timestamp: 2026-07-13T15:52:45+08:00
  checked: Shape-preserving internal-call argument effects, typed index provenance, focused escape negatives, pinned Record output, and the fidelity census.
  found: Effects are granted per argument only when SSA proves that argument is a non-escaping SETITEM/REVERSEITEMS/MEMCPY receiver; returned, static-stored, nested, called, or resized aliases remain Unknown. SETITEM discards exact contents but retains arity, so Contract_Record.test_DeconstructRecord emits runtime loc0[0]/loc0[1] indexes and returns field zero. Index-derived declaration safety now reaches copy chains by fixed point and dynamicizes live parameter/static storage. The census is Exact 1106 / Conservative 68 / Incomplete 5.
  implication: Record is recovered without preserving stale elements or trusting escaped aliases. The remaining fixed-shape work is exclusively the three nested Reentrancy UNPACK sites.

- timestamp: 2026-07-13T15:52:45+08:00
  checked: Independent soundness review plus full all-target feature/no-feature Rust tests, both Clippy configurations, generated typed C#, and pinned net10 Roslyn.
  found: Review caught an initially unsound all-arguments preservation rule and traversal-order/static/parameter typed hazards before commit. A broad external-symbol seed then caused Contract_FieldKeyword CS0266 errors; narrowing provenance to actual runtime Index definitions restored required casts. Final gates pass: 715 library tests passed / 1 ignored, all integration targets pass with and without default features, both Clippy runs pass with warnings denied, Contract_Record has no stale object[] casts, and Roslyn is 103 passed / 0 failed / 0 errors.
  implication: The Record slice is ready to checkpoint; the observed Roslyn regression is eliminated rather than accepted as a known limitation.

## Resolution

root_cause: Fixed collection shape was lost both across resolved call returns and across content-only argument mutation; method-global invalidation also let later calls poison earlier facts. Record additionally exposed stale typed declarations for runtime index values.
fix: Added flow-sensitive content-versus-shape invalidation, unanimous Array/Struct return summaries, escape-aware per-argument shape-preserving effects, runtime Index expansion, and fixed-point typed-index safety for locals plus live parameter/static storage.
verification: Focused positive/negative shape and escape tests pass; full all-target Rust tests pass with all and no default features; both Clippy configurations pass with warnings denied; pinned census is Exact 1106 / Conservative 68 / Incomplete 5; pinned Roslyn is 103 passed / 0 failed / 0 errors; formatting and diff checks pass.
files_changed: [src/decompiler/analysis/method_contracts.rs, src/decompiler/cfg/method_view.rs, src/decompiler/cfg/ssa/builder.rs, src/decompiler/cfg/ssa/context.rs, src/decompiler/cfg/ssa/mod.rs, src/decompiler/csharp/render.rs, src/decompiler/csharp/render/structured/plan.rs, src/decompiler/csharp/render/structured/stmt.rs, src/decompiler/csharp/render/structured/tests.rs, src/lib.rs]
