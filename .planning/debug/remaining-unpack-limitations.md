---
status: narrowed
trigger: "continue analysis and resolve the nine remaining pinned-corpus limitations"
created: 2026-07-13T00:00:00+08:00
updated: 2026-07-16T17:42:45+08:00
---

## Current Focus

hypothesis: Contract_Foreach@0458 calls a no-INITSLOT helper that requires four entry-stack values, but the caller supplies none; the subsequent dynamic `PICKITEM; UNPACK` cannot prove tuple element shape.
test: Preserve the proven indexed-return path for uniform nested collections, while keeping unresolved call arity and dynamic UNPACK provenance fail-closed in the pinned corpus.
expecting: Contract_Foreach@0458 remains the only incomplete method; generated C# still compiles across all 103 pinned contracts.
next_action: Keep this bytecode path explicit as a known limitation unless a corrected compiler artifact or VM-validated call-entry model supplies the missing tuple values.
reasoning_checkpoint: null
tdd_checkpoint: null

## Symptoms

expected: Every pinned v3.10.0 contract whose collection shape and call-entry stack are statically provable should decompile exactly, while unresolved call arity and dynamic UNPACK sources remain fail-closed.
actual: The corpus now has one incomplete method: Foreach@0458 has a four-value CALL underflow followed by a dynamic tuple UNPACK. The two Enum loop-stack methods are exact after the targeted join recovery.
errors: Foreach reports LostStackValue at CALL/STLOC3/STLOC4 and MissingProvenance at UNPACK; no Enum join underflow remains.
reproduction: Run the pinned v3.10.0 Roslyn corpus census with /tmp/devpack-artifacts-v3.10.0 and inspect Contract_Returns.mix, Contract_Tuple.t1, Contract_NEP11.transfer, Contract_Record.test_DeconstructRecord, Contract_Reentrancy helpers, Contract_Foreach, and the two Contract_Enum parse-ignore-case methods.
started: Remaining after the 2026-07-13 structured C# corpus fixes at commit 858d850.

## Eliminated

- hypothesis: Clear invalidated_collection_roots between SSA fixpoint passes so call-return shapes can be rediscovered.
  evidence: This recovers the shape experiments but breaks loop_backedge_mutation_invalidates_header_collection_provenance, allowing a later loop mutation to unsafely validate an earlier header UNPACK.
  timestamp: 2026-07-13T00:00:00+08:00

- hypothesis: Infer Reentrancy UNPACK arity from syscall consumers or the observed two-value prefix.
  evidence: A three-element runtime source changes the storage key and leaves a tail value across RET, so consumer-driven arity two is unsound.
  timestamp: 2026-07-13T00:00:00+08:00

- hypothesis: Recovering the proven top value at a compiler-generated DUP/slot-load/conditional join would be unsound across all short-stack merges.
  evidence: The recovery is gated on the exact three-instruction shape, two predecessors, a one-value common-prefix extension, and a non-unknown shorter-path top value. It resolves both Enum methods without changing unrelated join behavior.
  timestamp: 2026-07-14T15:11:45+08:00

## Evidence

- timestamp: 2026-07-13T00:00:00+08:00
  checked: Pinned corpus census at commit 858d850.
  found: Exact 1103, Conservative 67, Incomplete 9. The incomplete roots are Foreach 0458; NEP11 025D; Record 00E5; Reentrancy 0272/02AE; Returns 0084; Tuple 0007; and Enum 0057/00F1.
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

- timestamp: 2026-07-13T18:20:48+08:00
  checked: Bounded indexed argument postconditions, unanimous static/private-entry fixed point, read-only versus content-mutating argument effects, static alias invalidation, and the pinned Reentrancy constructor/consumer chain.
  found: sub_0x0252 proves arg0[0] = Array(2); noReentrancyByAttribute stores Array(2) with nested field zero into static0; direct incoming calls prove the same indexed fact for sub_0x0272 and sub_0x02AE. Runtime UNPACK indexes inherit the nested shape, while conflicting/dynamic/partial/overwritten fields, unknown incoming calls, ABI/address-taken entries, opaque calls, static conflicts, and alias resize remain fail-closed. A seeded-static SSA version collision found by the regression fixture was fixed by reserving static version zero.
  implication: All three Reentrancy UNPACK sites are now locally producer-proven rather than consumer-inferred, and static facts cannot be reused after a possible alias mutation.

- timestamp: 2026-07-13T18:20:48+08:00
  checked: Full all-target Rust tests with all and no default features, both Clippy configurations with warnings denied, pinned v3.10.0 fidelity census, generated Reentrancy C#, and pinned net10 Roslyn compilation.
  found: 722 library tests pass / 1 ignored in both feature configurations; all integration targets pass; both Clippy runs pass; census is Exact 1106 / Conservative 70 / Incomplete 3; Reentrancy helpers emit direct runtime indexes with no synthetic UNPACK fallback; Roslyn compiles 103 contracts with 0 failures and 0 errors; TestingArtifacts/devpack is absent after the gate.
  implication: The Reentrancy slice is verified and ready to checkpoint. The residual investigation is limited to genuinely variable Foreach and the two Enum loop-stack methods.

- timestamp: 2026-07-14T15:11:45+08:00
  checked: Targeted DUP-join recovery, native C# API lowering, focused renderer tests, and the pinned v3.10.0 Roslyn census.
  found: The current env-enabled Rust fidelity gate reports 103 contracts with Exact 1101 / Conservative 70 / Incomplete 1. The only incomplete method is Contract_Foreach@0x0458: CALL at 0x045B consumes four unproven entry-stack values while its caller supplies none, then UNPACK and two local stores lose provenance. Native properties (`NeoToken.Symbol`, `GasToken.Symbol`, `LedgerContract.CurrentHash`, and `CurrentIndex`) render as properties, signature-sensitive casts cover `MemorySearch` and `RoleManagement.GetDesignatedByRole`, and Roslyn compiles 103/103 contracts with 0 errors.
  implication: Enum loop joins and all known C# framework API-shape failures are resolved. The generic indexed-return fact path is verified, while Foreach remains intentionally fail-closed because the private helper's first tuple element is not proven and the dynamic UNPACK cannot be reconstructed soundly.

- timestamp: 2026-07-14T22:35:00+08:00
  checked: Raw v3.10.0 `Contract_Foreach.nef` bytes at `0x0450..0x0457`, with the caller's `CALL -11` at `0x045B` targeting `0x0450`, and `CommonForEachStatement.cs` in the pinned compiler.
  found: The detached helper is exactly `PUSH2; PACKSTRUCT; PUSH2; PACKSTRUCT; SWAP; PUSH2; PACK; RET`, with no literal element pushes and no `INITSLOT`. The caller starts with `INITSLOT 5,0` and executes only `CALL -11`; the VM therefore supplies zero stack values to a helper whose first two `PACKSTRUCT` operations require four. The compiler lowering confirms tuple foreach emits `UNPACK; DROP; STLOC...`, but it does not provide a hidden argument or tuple-value ABI.
  implication: The outer `Array(2)` length is provable, but neither outer element is uniformly proven to be `Struct(2)`: under the inferred four-value ABI one element is an unconstrained entry argument, and under the actual caller stack the helper underflows. Recovering two tuple fields would invent values or silently change VM fault behavior, so the C# renderer must retain the compatibility/null path and incomplete fidelity marker.

- timestamp: 2026-07-14T23:15:14+08:00
  checked: Hash-bound native `CALLT` return typing for the generated C# renderer, focused expression/planner tests, generated `Contract_ByteString` and `Contract_Types` output, all-target Rust tests, both Clippy configurations, the pinned corpus fidelity census, and Roslyn compilation against a locally built `Neo.SmartContract.Framework` assembly.
  found: Stable `StdLib` method-token signatures now provide exact C# helper return types (`BigInteger`, `object[]`, `ByteString`, or `string`) while their VM value types remain conservative (`integer`, `array`, or `bytestring`). Restricted, unresolved, and hash-mismatched tokens remain dynamic. The generated ByteString helpers are typed for `StrLen`, `MemorySearch`, and `StringSplit`; the Base58Check wrapper is typed as `string`. The representative Roslyn suite passed, and the full pinned corpus compiled 103 contracts with 0 failures and 0 errors.
  implication: Known framework API return types no longer force private C# helpers through unnecessary `dynamic`, without assigning types to arbitrary external calls or changing the residual Foreach limitation. The fidelity census remains Exact 1101 / Conservative 70 / Incomplete 1.

- timestamp: 2026-07-15T00:00:00+08:00
  checked: Catalog-bound syscall return typing, fallback rendering for known syscalls with unresolved argument types, focused renderer tests, both Clippy configurations, and the representative plus full pinned net10 Roslyn suites.
  found: Stable `Storage`, `Runtime`, `Contract`, `Crypto`, and `Iterator` syscall hashes now provide exact C# return types (`StorageContext`, `Iterator`, `ByteString`, `UInt160`, `BigInteger`, `object[]`, `Transaction`, `string`, or `bool`) while unknown hashes remain dynamic. When a known API cannot be lowered because an argument is still dynamic, its `Runtime.LoadScript` compatibility form now carries an explicit catalog return cast. The representative suite passed and the full pinned corpus compiled 103 contracts with 0 failures and 0 errors.
  implication: Known syscall return types are compile-safe even on compatibility fallbacks; arbitrary hashes and unresolved APIs remain fail-closed. The fidelity census remains Exact 1101 / Conservative 70 / Incomplete 1, with `Contract_Foreach@0x0458` still the sole incomplete method.

- timestamp: 2026-07-15T03:03:09+08:00
  checked: Typed static-field boundary conversions, the Contract_RefSupport Roslyn regression, focused structured-renderer tests, the pinned fidelity census, and both representative plus full pinned net10 Roslyn suites.
  found: Static slots whose neutral method symbol table omits the global field type now receive the contract-level C# field type during assignment rendering. Unknown compatibility values are emitted through an explicit `(TargetType)(dynamic)(...)` boundary; Contract_RefSupport no longer produces `object`-to-`BigInteger` CS0266 errors. The census remains Exact 1101 / Conservative 70 / Incomplete 1, and Roslyn compiles 103/103 contracts with zero errors.
  implication: Typed C# output remains compile-valid when conservative low-level calls write known static fields, without weakening the fail-closed Foreach limitation or assigning arbitrary external-call types.

- timestamp: 2026-07-15T14:01:11+08:00
  checked: Conservative private-helper parameter inference, indexed/nullability safety guards, focused planner regressions, and all default/no-default Rust gates.
  found: An inferred helper parameter is promoted from `dynamic` only when every resolved internal incoming call supplies the same concrete C# type. Conflicting, incomplete, indirect/unresolved, null-checked, and directly indexed parameters remain dynamic; recursive IR walkers are isolated in `parameter_calls.rs` and `parameter_index.rs`. The focused suite covers unanimous promotion plus conflicting, null-checked, and indexed negatives; all Rust tests and both Clippy configurations pass.
  implication: Private C# helper signatures are more readable without inventing an ABI type or weakening the existing fail-closed indexed/nullability boundaries.

- timestamp: 2026-07-15T16:05:31+08:00
  checked: `Contract_Array` value-array boundary regression, full default/no-default Rust tests, both Clippy configurations, the pinned C# fidelity census, and the net10 Roslyn corpus gate.
  found: A typed `BigInteger[]` array literal assigned to a VM `object[]` boundary now renders as `new object[]` with boxed elements; the focused renderer regression passes. The pinned census remains Exact 1101 / Conservative 70 / Incomplete 1, with Roslyn compiling all 103 contracts and zero errors.
  implication: C# output no longer emits an invalid value-array-to-object-array assignment in `Contract_Array`; typed arrays remain concrete where their target is typed. `Contract_Foreach@0x0458` remains the sole intentionally incomplete method.

- timestamp: 2026-07-15T18:47:06+08:00
  checked: Conservative private-helper parameter inference for proven indexable and nullable-reference arguments, focused planner regressions, the pinned C# census, and the net10 Roslyn corpus gate.
  found: Private helper parameters are promoted when every resolved caller supplies the same concrete indexable/reference type, including `object[]` parameters used by `PICKITEM` and array validators guarded by `ISNULL`. Proven scalar candidates still remain dynamic when indexed or null-checked. The typed census moved from 1,719 to 1,498 dynamic occurrences, while fidelity stayed Exact 1101 / Conservative 70 / Incomplete 1 and Roslyn remained 103/103.
  implication: C# helper signatures now expose proven collection contracts without turning VM-invalid scalar calls into compile-time failures; the indexed/nullability fail-closed boundaries remain for ambiguous or non-indexable values.

- timestamp: 2026-07-16T14:24:00+08:00
  checked: Structured symbol refinement for compiler-generated Phi assignments, focused common-type/conflict regressions, the pinned C# fidelity census, and the net10 Roslyn corpus gate.
  found: Phi values materialized as assignments now retain a concrete C# type when every observed arm agrees; conflicting arms remain `Any` and therefore render as `dynamic`. The corpus fidelity boundary is unchanged at Exact 1126 / Conservative 52 / Incomplete 1, while the generated C# corpus compiles 103/103 contracts with zero errors.
  implication: Address-taken and branch-merged numeric helpers no longer needlessly expose `dynamic` return values; unresolved or mixed Phi provenance remains conservative, and `Contract_Foreach@0x0458` is still intentionally fail-closed.

- timestamp: 2026-07-16T16:04:39+08:00
  checked: C# local/static slot refinement, unknown/conflicting/null negative paths, PUSHA pointer provenance, all-target Rust tests with and without default features, both Clippy configurations, and the pinned C# fidelity census.
  found: Local and static symbols are refined only when every observed structured definition agrees; unknown, conflicting, and null-plus-concrete paths widen to `Any`. PUSHA targets are tracked from the exact method slice so integer-looking function pointers remain `Pointer`/`dynamic` instead of being mislabeled as `BigInteger`. The census remains Exact 1126 / Conservative 52 / Incomplete 1, with `Contract_Foreach` as the sole incomplete contract. Roslyn could not run because no `Neo.SmartContract.Framework.dll` is installed in this environment.
  implication: Readable C# declarations improve for proven compiler-generated locals without weakening fail-closed behavior or changing the known Foreach limitation; the existing compile gate remains required when the framework assembly is available.

- timestamp: 2026-07-16T17:42:45+08:00
  checked: Pinned provenance `5b0b63880b6201ae3f974cc845e93a90462d8043`, raw Foreach offsets, the C# fidelity census, Foreach parity tests, and the full pinned Roslyn compile gate.
  found: The extracted v3.10.0 bytes place the compiler-generated helper at `0x0450`, `CALL -11` at `0x045B`, and the incomplete caller method at `0x0458`; the previous `0x04AC` baseline named a `JMPLE` instruction rather than a method entry. The census remains Exact 1126 / Conservative 52 / Incomplete 1, all three Foreach parity tests pass, and Roslyn compiles 103/103 generated contracts with zero failures and errors.
  implication: The pinned baseline and documentation now identify the real method entry without changing the fail-closed behavior. The Foreach call still has a statically proven four-value VM underflow, so tuple fields remain intentionally unrecovered.

## Resolution

root_cause: Fixed collection shape was lost across resolved call returns, content-only argument mutation, constructor field writes, static storage, and private method entry. Method-global invalidation also let later calls poison earlier facts, while Record exposed stale typed declarations for runtime index values.
fix: Added flow-sensitive content-versus-shape invalidation, unanimous Array/Struct return summaries, escape-aware read-only/shape-preserving effects, bounded constant-index postconditions, unanimous static/private-entry fixed points, flow-sensitive static alias invalidation, runtime Index expansion, fixed-point typed-index safety for locals plus live parameter/static storage, and conservative common-type refinement for structured Phi assignments.
verification: Focused positive/negative shape, escape, field, static, entry, mutation, DUP-join, direct uniform-element, interprocedural return-fact, and Phi-type tests pass; native C# renderer tests pass; the env-enabled pinned census is Exact 1126 / Conservative 52 / Incomplete 1; pinned Roslyn is 103 passed / 0 failed / 0 errors. Foreach@0x0458 remains fail-closed by design because its call entry values and tuple element shape are not statically proven.
files_changed: [src/decompiler.rs, src/decompiler/analysis/method_contracts.rs, src/decompiler/cfg/method_view.rs, src/decompiler/cfg/ssa/builder.rs, src/decompiler/cfg/ssa/context.rs, src/decompiler/cfg/ssa/mod.rs, src/decompiler/cfg/method_body.rs, src/decompiler/csharp/render.rs, src/decompiler/csharp/render/structured/declarations.rs, src/decompiler/csharp/render/structured/expr_context.rs, src/decompiler/csharp/render/structured/plan.rs, src/decompiler/csharp/render/structured/plan_methods/return_types.rs, src/decompiler/csharp/render/structured/stmt.rs, src/decompiler/csharp/render/structured/tests.rs, src/decompiler/csharp/render/structured/tests_expr_types.rs, src/decompiler/csharp/render/structured/tests_plan.rs, src/decompiler/native_method_types.rs]
