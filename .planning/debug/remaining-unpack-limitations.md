---
status: investigating
trigger: "continue analysis and resolve the nine remaining pinned-corpus limitations"
created: 2026-07-13T00:00:00+08:00
updated: 2026-07-13T12:19:00+08:00
---

## Current Focus

hypothesis: A fixed return-shape summary for direct internal calls whose every normal return is the same literal PACK/PACKSTRUCT arity will recover Contract_Returns and Contract_Tuple without weakening collection invalidation or loop safety.
test: Run the pinned Contract_Returns and Contract_Tuple disassembly plus structured IR to confirm producer return opcodes, call-site offsets, and the exact pre-fix UNPACK failures.
expecting: div@0x006A returns literal PACKSTRUCT(2) before RET and getResult@0x0000 returns literal PACKSTRUCT(4); mix@0x0084 and t1@0x0007 receive those values through resolved internal calls but report missing UNPACK provenance.
next_action: Use the CLI to disassemble and render structured IR for both pinned NEF/manifest pairs, then inspect only the relevant producer/caller spans.
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

## Resolution

root_cause: null
fix: null
verification: null
files_changed: []
