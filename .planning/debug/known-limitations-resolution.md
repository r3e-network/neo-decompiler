---
status: resolved
trigger: "analysis adn check if we can go further to resolve those known limitations"
created: 2026-07-12T15:27:05+08:00
updated: 2026-07-13T07:05:30+08:00
---

## Current Focus

hypothesis: RESOLVED - the pinned fallback set, nested/multi-continuation finally control, plain ENDTRY nonlocal transfers, bypassable branch merges, terminal-exit loops, and the two residual dead-control artifacts are covered by semantic CFG/SSA/IR handling and regression tests.
test: Strictly extract 103 v3.10.0 pairs from commit 5b0b63880b6201ae3f974cc845e93a90462d8043, run the zero-fallback gate plus all Rust/JS/Web quality gates, and inspect Contract_Break, Contract_Continue, Contract_GoTo, and Contract_Returns generated C#.
expecting: Confirmed - all pinned contracts use structured bodies; affected branches, returns, breaks, continues, and finalizers remain ordered; no unknown placeholders, dead pending-return label, or false while-loop artifact remains in the inspected contracts.
next_action: Keep genuinely runtime-variable stack transforms fail-closed, retain conservative exceptional-only finally SSA, and rerun the full Roslyn corpus census when Neo.SmartContract.Framework.dll is available locally.
reasoning_checkpoint: null
tdd_checkpoint: null

## Symptoms

expected: Known limitations should distinguish intentionally invalid inputs from valid contracts, and the full pinned devpack corpus should be available for a fail-closed structured C# coverage gate.
actual: CallFlagInvalid is listed as unsupported, the normal CI corpus omits devpack, and the full 103-contract corpus produces structured-rendering fallbacks.
errors: Unsupported call flags 0x10; unsupported ENDFINALLY; unresolved dynamic PICK, ROLL, and REVERSEN stack semantics; one unknown value stored by Contract_Abort.
reproduction: Extract neo-devpack-dotnet v3.10.0 artifacts and run csharp_corpus_has_zero_structured_fallback with TestingArtifacts/devpack present.
started: Observed during the structured IR C# cutover audit on 2026-07-12; no evidence that full devpack coverage previously passed.

## Eliminated

- hypothesis: ENDFINALLY can be modeled as ordinary fallthrough or as a single fixed jump target encoded by the instruction.
  evidence: Neo VM ENDFINALLY has no operand. It pops the active ExceptionHandlingContext and either jumps to the path-specific EndPointer saved by ENDTRY/ENDTRY_L or rethrows UncaughtException; multiple ENDTRY sites may enter one finally with different targets.
  timestamp: 2026-07-12T16:34:00+08:00

- hypothesis: Contract_Abort STLOC1 is itself modeled incorrectly or lacks its fixed pop-one stack effect.
  evidence: STLOC1 uses the correct (1,0) effect, and the same compiler catch prologue at Contract_Abort offset 82 receives exception_0 without loss because THROW terminates the try body and leaves the handler with only an Exception predecessor. The offset-50 failure occurs only when no-return CALLs add a normal predecessor.
  timestamp: 2026-07-12T16:45:00+08:00

- hypothesis: The pinned v3.10.0 corpus failures require arbitrary runtime-variable PICK/ROLL/REVERSEN support or are inherently unrepresentable in scalar SSA.
  evidence: All 38 exact-tag occurrences use an immediately preceding literal count (PICK: 32; ROLL: 4; REVERSEN: 2). DefinitionFacts already resolves integer literals for PACK, and a temporary scalar-SSA spike removes all 35 affected methods.
  timestamp: 2026-07-12T17:02:00+08:00

- hypothesis: /tmp/neo-devpack-dotnet-audit and /tmp/devpack-artifacts-audit represent the pinned v3.10.0 release baseline.
  evidence: The checkout is floating master at 1c2f7adc, not tag v3.10.0. Its 68-method/18-contract and 39-operation figures are diagnostic history only and are superseded by the exact-tag checkout at 5b0b63880b6201ae3f974cc845e93a90462d8043.
  timestamp: 2026-07-12T17:20:00+08:00

## Evidence

- timestamp: 2026-07-12T15:27:05+08:00
  checked: Neo CallFlags and MethodToken validation plus CallFlagInvalid bytes.
  found: 0x10 is outside All=0x0F; the fixture checksum is valid but token metadata is intentionally invalid and the script never executes CALLT.
  implication: Strict rejection is correct; this fixture should be reclassified, not accepted.

- timestamp: 2026-07-12T15:27:05+08:00
  checked: Initial local devpack audit checkout and extractor.
  found: All 103 committed NEF and manifest pairs extract and parse without compiling upstream, but later provenance verification identified this checkout as floating master 1c2f7adc rather than v3.10.0.
  implication: Corpus availability is an infrastructure gap, but this checkout cannot supply the pinned release baseline.

- timestamp: 2026-07-12T15:27:05+08:00
  checked: Full-corpus zero-fallback gate.
  found: Failures concentrate in ENDFINALLY, dynamic PICK, ROLL, REVERSEN, and one unknown STLOC1 value.
  implication: Current zero-fallback claim does not extend to the full devpack corpus; semantic support must be added before enabling the gate.

- timestamp: 2026-07-12T16:08:00+08:00
  checked: Project-local skill and debug knowledge-base discovery.
  found: Neither .codex/skills nor .agents/skills contains a project skill, and .planning/debug/knowledge-base.md is absent.
  implication: There is no project-specific rule or prior resolved-session hypothesis to apply before open-ended investigation.

- timestamp: 2026-07-12T16:08:00+08:00
  checked: Worktree status before diagnostics.
  found: The requested worktree already contains broad modified and untracked implementation changes, including the SSA, CFG, structured renderer, corpus test, and extraction tool files relevant to this investigation.
  implication: Diagnostics must preserve current contents and avoid product edits or cleanup; conclusions apply to this exact dirty state.

- timestamp: 2026-07-12T16:08:00+08:00
  checked: Audit artifact availability.
  found: /tmp/neo-devpack-dotnet-audit exists and /tmp/devpack-artifacts-audit contains 206 flat files; TestingArtifacts/devpack is absent in the worktree.
  implication: The full 103-pair corpus is locally available, but the gate's path handling must be inspected before reproduction.

- timestamp: 2026-07-12T16:17:00+08:00
  checked: Initial floating-master csharp_corpus_has_zero_structured_fallback test with /tmp/devpack-artifacts-audit temporarily exposed at TestingArtifacts/devpack.
  found: The gate deterministically failed after decompiling the corpus and reported only PICK, ROLL, REVERSEN, ENDFINALLY, and Contract_Abort STLOC1 primary issues; the temporary symlink was removed after the run.
  implication: The recorded symptom is reproducible in the current dirty worktree and no additional primary fallback category appeared.

- timestamp: 2026-07-12T16:22:00+08:00
  checked: Mechanical aggregation of floating-master fallback records in /tmp/known-limitations-corpus.log.
  found: There are 68 affected method entries in 18 distinct devpack contracts. PICK accounts for 31 methods/6 contracts (Contract_Integer, Contract_Math, Contract_Pattern, Contract_PropertyMethod, Contract_Record, Contract_Reentrancy); ROLL for 2 methods/1 contract (Contract_String); REVERSEN for 3 methods/3 contracts (Contract_ClassInit, Contract_Foreach, Contract_NULL); ENDFINALLY for 31 methods/8 contracts (Contract_Abort, Contract_Assert, Contract_Break, Contract_Continue, Contract_GoTo, Contract_Returns, Contract_TryCatch, Contract_WriteInTry); STLOC1 for 1 method/1 contract (Contract_Abort). Dynamic stack transforms therefore total 36 methods/10 contracts; 85 of 103 contracts have no reported structured fallback.
  implication: This established the failure categories, but the counts are not the pinned release baseline and are superseded below.

- timestamp: 2026-07-12T16:22:00+08:00
  checked: Manifest ABI offset mapping for all 68 floating-master fallback entries plus upstream source for the one non-ABI start.
  found: Every fallback start maps one-to-one to a method. Sixty-seven starts map directly to manifest ABI names; Contract_Returns offset 131 is the private upstream method TryReturnInternal, confirmed in tests/Neo.Compiler.CSharp.TestContracts/Contract_Returns.cs.
  implication: Counts are method counts rather than duplicate coverage aliases, and the full affected method set can be named exactly.

- timestamp: 2026-07-12T16:34:00+08:00
  checked: Neo VM control semantics in neo-vm commit 2c2be02e5067c524677d9bf74062077283e1ecd8, JumpTable.Control.cs and ExceptionHandlingContext.cs, corroborated by neo-devpack-dotnet v3.10.0 OpCode documentation.
  found: TRY pushes a context containing catch/finally pointers and state. Normal ENDTRY computes its explicit target; if the context has finally, it changes state to Finally, saves that target in EndPointer, and jumps to FinallyPointer. Exceptional entry leaves UncaughtException set and also jumps to finally. ENDFINALLY pops the context, jumping to the saved EndPointer only when UncaughtException is null; otherwise it invokes exception dispatch/rethrow. A single finally can therefore resume multiple ENDTRY targets or propagate an exception.
  implication: Sound static modeling requires path-sensitive pending-continuation/exception state (for example context-expanded/cloned finally CFG nodes or a synthetic continuation token/dispatch plus exceptional edge); ENDFINALLY is neither fallthrough nor a statically single-target jump.

- timestamp: 2026-07-12T16:34:00+08:00
  checked: Current CFG and SSA implementation in cfg/builder/{terminator,edges}.rs, cfg/basic_block/terminator.rs, cfg/ssa/builder.rs, and cfg/structure.rs.
  found: Endfinally becomes Terminator::Unknown and receives no edge; the terminator enum has no EndFinally/pending-continuation representation. EndTry always adds a direct unconditional edge to its explicit continuation even when an enclosing finally exists. TryEntry adds handler/finally edges only from the TRY block, with no region/context identity. SSA then explicitly records UnsupportedControl for every ENDFINALLY, while handle_try assumes one reachable shared EndTry/continuation and structures the finally independently.
  implication: The primary fallback is deliberate, but it protects against real CFG and data-flow loss. Removing the issue alone would be unsound, especially for multiple ENDTRY targets and exceptional finally entry.

- timestamp: 2026-07-12T16:34:00+08:00
  checked: Actual CFG emitted for Contract_Assert.testAssertInTry (method start 28; TRY 33, body ENDTRY 39, catch ENDTRY 44, finally ENDFINALLY 48, continuation 49).
  found: The graph routes body and catch ENDTRY blocks directly to the post-try block; the finally block is reachable only from TryEntry and has no outgoing edge. Thus PUSH2/STLOC0 in the finally does not reach the post-try LDLOC0 data-flow join.
  implication: The representation gap is observable on a corpus failure, not merely inferred from enum shape; current SSA can bypass finally side effects on normal completion.

- timestamp: 2026-07-12T16:34:00+08:00
  checked: neo-devpack-dotnet's own TryCatchFinallyCoverage static analyzer.
  found: Its traversal explicitly carries (tryBlock, endFinallyBlock, tryType, continueAfterFinally) as path state, routes ENDTRY into finally with a saved ending block, and lets ENDFINALLY either visit that saved block or stop for an outstanding throw.
  implication: An existing upstream static model independently confirms the continuation-state requirement and supplies an actionable reference algorithm.

- timestamp: 2026-07-12T16:45:00+08:00
  checked: Contract_Abort.testAbortInTry disassembly, per-method structured IR trace, call graph, inferred method contracts, and upstream source.
  found: The try body branches to CALL testAbortMsg at 46 or CALL testAbort at 48; both callees end in ABORTMSG/ABORT and never return in Neo VM. Nevertheless both manifest methods declare Integer, MethodContract ReturnBehavior is value (the enum has only Value/Void/Unknown), CallContract returns_value is true, apply_known_call pushes a result, and CFG construction treats every CALL as ordinary fallthrough. This creates impossible edges 46->48 and 48->the catch handler at 50.
  implication: The first semantic divergence is the no-return call at offset 46 on the true path (offset 48 on the false path): termination is absent from shared call contracts and is unavailable to CFG construction.

- timestamp: 2026-07-12T16:45:00+08:00
  checked: SSA catch-entry join and emitted IR for Contract_Abort.testAbortInTry.
  found: Neo VM ExecuteThrow pushes exactly one exception item before jumping to CatchPointer. SsaBuilder injects exception_0 only when every predecessor edge is Exception. Because the impossible CALL fallthrough makes the offset-50 handler a mixed-predecessor block, compute_join_entry instead merges the empty TRY exception edge with the fabricated call-result stack, yielding a tainted phi; emitted IR shows the call-result operand versus '?' and then loc1 = that phi. STLOC1 correctly reports consumption of the tainted value.
  implication: The actionable root cause has two coupled gaps: call contracts/CFG need a NeverReturns outcome that suppresses fallthrough, and exception payloads should be modeled per exception edge (or via handler-entry state) rather than only by an all-predecessors heuristic.

- timestamp: 2026-07-12T16:45:00+08:00
  checked: Differential catch prologue at Contract_Abort.testAbortInCatch offset 82.
  found: Its try body ends in THROW, so no normal fallthrough reaches the handler; the handler is recognized as an exception entry and STLOC1 stores exception_0 without an incomplete issue.
  implication: This controlled comparison rules out STLOC1 and general catch-slot lowering, isolating the divergence to no-return CALL control flow plus mixed-edge exception injection.

- timestamp: 2026-07-12T17:02:00+08:00
  checked: Neo VM v3.10.0 stack-operation implementations and bounds behavior.
  found: PICK pops/coerces n and duplicates the item n positions below the remaining top; ROLL pops/coerces n and moves that item to the top; REVERSEN pops/coerces n and reverses the top n items. Negative, out-of-range, non-integer-coercible, or Int32-overflow counts fault. Relative to the pre-instruction stack, PICK preserves depth while ROLL and REVERSEN remove the count (net -1).
  implication: Dynamic forms have deterministic finite semantics and are soundly modelable, but exact lowering must preserve count coercion/bounds faults as well as data-dependent value identities.

- timestamp: 2026-07-12T17:02:00+08:00
  checked: Current fidelity classification and SsaBuilder::apply_special for PICK/ROLL/REVERSEN.
  found: classify_opcode unconditionally marks all three incomplete before inspecting provenance. apply_special then pops the count and records a use but emits no statement, does not call the existing resolve_nonnegative_literal helper, and performs no Vec<SsaVariable> transform. PICK therefore loses its copied output and makes the stack one slot too shallow; ROLL preserves the resulting depth but leaves identities in their old order; REVERSEN leaves the selected range unreversed. The nearby comment that SSA does not track concrete indices is contradicted by DefinitionFacts and literal resolution already used for PACK.
  implication: The exact first representation loss is in apply_special, with an additional unconditional reporting guard. The C# throwing stub is necessary today because downstream scalar data flow is already wrong, not merely unpretty.

- timestamp: 2026-07-12T17:02:00+08:00
  checked: Dynamic-op occurrence scan across the initial floating-master devpack checkout and rebuilt current structured IR.
  found: There are 39 operations in 36 methods/10 contracts: PICK 32 occurrences in 31 methods/6 contracts (31 PUSH2, 1 PUSH3), ROLL 4 in 2 methods/1 contract (all PUSH3), REVERSEN 3 in 3 methods/3 contracts (two PUSH5, one PUSH6). The extra operations are a second PICK in Contract_Record.test_RecordEquality and a second ROLL in each Contract_String.testEndWith overload. Current Contract_Integer.divRemByte visibly lowers to left/left and right%(left/left), then an underfilled PACK, confirming downstream corruption after the dropped PICK result.
  implication: The semantic conclusion remains valid, but the 39-operation count is superseded by the 38 operations in exact v3.10.0.

- timestamp: 2026-07-12T17:02:00+08:00
  checked: SsaExpr/SsaStmt and typed IR expression/statement vocabularies for arbitrary runtime counts.
  found: They contain only scalar assignments/expressions and fixed collection literals; there is no stack-state, dynamic permutation, or multi-result/projection node. The earlier type analysis likewise pushes Unknown for dynamic PICK and silently retains the old order for dynamic ROLL/REVERSEN.
  implication: Truly runtime-variable support requires either finite scalar expansion (evaluate/coerce once, guard bounds, select/merge every affected slot) or a first-class stack-state/multi-result transform with projections and an uncatchable VM-fault path. Until then, genuinely dynamic instances must remain fail-closed.

- timestamp: 2026-07-12T17:10:00+08:00
  checked: Full ENDFINALLY opcode scan in the eight affected contracts from the floating-master checkout.
  found: The corpus has 47 ENDFINALLY instructions in 32 methods/8 contracts: Contract_Abort 2 instructions/2 methods, Contract_Assert 3/3, Contract_Break 5/1, Contract_Continue 5/1, Contract_GoTo 6/1, Contract_Returns 2/1 private method, Contract_TryCatch 19/18, Contract_WriteInTry 5/5. ENDFINALLY is the primary gate issue in 31 methods; Contract_Abort.testAbortInTry contains ENDFINALLY but reports its earlier STLOC1 issue first.
  implication: Primary-issue aggregation understates ENDFINALLY's affected-method count by one; the 47-instruction count is diagnostic history from floating master, not the pinned release baseline.

- timestamp: 2026-07-12T17:10:00+08:00
  checked: Rebuilt current CLI against every flat NEF in the floating-master /tmp/devpack-artifacts-audit corpus.
  found: All 103 floating-master NEF/manifest pairs decompile successfully (103 success, 0 errors). That floating-master structured gate reports 68 fallback method entries in 18 contracts; 85 contracts have none.
  implication: This confirms ingestion for floating master but does not establish pinned-tag counts.

- timestamp: 2026-07-12T17:10:00+08:00
  checked: CallFlagInvalid parser rule, expected-unsupported registry, and captured error.
  found: CALL_FLAGS_ALLOWED_MASK is ReadStates|WriteStates|AllowCall|AllowNotify = 0x0F; parser rejects any other bit. The fixture stores 0x10, is explicitly listed as expected unsupported, and produces "unsupported bits (allowed mask 0x0F)".
  implication: CallFlagInvalid is a negative metadata fixture and must remain rejected; it is not part of the valid 103-contract semantic gap.

- timestamp: 2026-07-12T17:10:00+08:00
  checked: Diagnostic cleanup and scoped worktree status.
  found: TestingArtifacts/devpack is absent after the temporary gate runs. No product file was edited by this diagnosis; only this debug session artifact was updated. Pre-existing SSA, CFG, analysis, renderer, and planning changes remain untouched.
  implication: The dirty worktree has been preserved and the session is resumable without transient corpus state.

- timestamp: 2026-07-12T17:20:00+08:00
  checked: Exact neo-devpack-dotnet v3.10.0 checkout at 5b0b63880b6201ae3f974cc845e93a90462d8043 and extraction to /tmp/devpack-artifacts-v3.10.0.
  found: The pinned tag supplies 103/103 NEF and manifest pairs. Its authoritative zero-fallback gate reports 67 methods in 17 contracts: PICK 31, ROLL 2, REVERSEN 2, ENDFINALLY 31, and STLOC1 1; 86 contracts have no primary structured fallback.
  implication: These figures supersede every 68/18 count from floating master and are the release baseline.

- timestamp: 2026-07-12T17:20:00+08:00
  checked: Exact-tag dynamic scan and diagnosis-only scalar-SSA spike in /tmp/neo-decompiler-stack-spike-20260712.
  found: v3.10.0 has 38 literal stack-transform operations: PICK 32, ROLL 4, and REVERSEN 2. The spike resolves and applies those literal transforms and eliminates all 35 dynamic primary failures, leaving 32 methods in 8 contracts: 31 ENDFINALLY and 1 STLOC1.
  implication: The entire pinned-corpus dynamic-stack limitation is immediately resolvable with existing scalar SSA; genuinely runtime-variable counts remain a separate fail-closed design problem.

- timestamp: 2026-07-12T17:20:00+08:00
  checked: Full Roslyn census of generated exact-tag C# after the literal stack-transform spike.
  found: 28 contracts compile and 75 fail. Leading first-diagnostic codes are CS0266 (32), CS0161 (26), CS0103 (19), CS0019 (15), and CS0021 (9).
  implication: Zero structured fallback is not equivalent to generated-C# compile coverage; the Roslyn failures are a separate renderer/type/control-flow remediation backlog.

- timestamp: 2026-07-12T20:04:24+08:00
  checked: Contract_Returns.TryReturnInternal disassembly, CFG, optimized SSA, structured IR, and pinned upstream source.
  found: Nested ENDTRY chains now unwind through the inner and outer finally regions; exceptional finally entry is represented separately from normal ENDTRY entry; both `return ++a` paths capture their value before cleanup; and the physical RET no longer receives an unknown stack value.
  implication: Return-through-finally preserves source ordering and pending-return semantics without ENDFINALLY or LostStackValue fallback.

- timestamp: 2026-07-12T20:04:24+08:00
  checked: Exact v3.10.0 zero-fallback gate and broad Rust verification after the finally/return changes.
  found: The 103-contract gate passes with zero structured fallback; 646 library tests pass with 1 ignored; all 44 ir_pipeline tests pass; cargo check --all-targets and cargo clippy --all-targets -- -D warnings pass; the transient TestingArtifacts/devpack symlink is absent after the gate.
  implication: The original valid-corpus limitation set is resolved without accepting the intentionally invalid CallFlagInvalid fixture.

- timestamp: 2026-07-13T06:48:53+08:00
  checked: Freshly extracted v3.10.0 corpus provenance plus generated C# and IR for Contract_GoTo and Contract_Returns after the latest plain-ENDTRY changes.
  found: The strict extractor recreated 103 pairs from commit 5b0b63880b6201ae3f974cc845e93a90462d8043 and the zero-fallback gate passes, but Contract_Returns.sum/subtract omit the integer-normalization false arm and can fall through without returning. Contract_GoTo.test treats a both-successors-internal natural-loop header as a conditional while and emits its update after an unconditional continue.
  implication: Zero structured fallback proves representability, not semantic structuring. Reachability-only merge selection must be constrained by post-dominance, and terminal exits must not disqualify an otherwise unconditional branch-headed loop.

- timestamp: 2026-07-13T07:05:30+08:00
  checked: Current finally/ENDTRY implementation and its focused CFG/SSA/structurer regressions.
  found: Triple-nested ENDTRY chains unwind one parent at a time; one physical finally dispatches to every saved normal continuation; natural and nonlocal catch ENDTRY targets are distinguished; exceptional finally entry does not taint the normal return stack; unconditional backedges to TRY entries become while(true); and nonlocal plain ENDTRY returns from a try-entry loop.
  implication: The saved debug record now reflects the later multi-continuation, plain-ENDTRY, protected-region, and try-entry-loop work rather than stopping at the earlier single-continuation snapshot.

- timestamp: 2026-07-13T07:05:30+08:00
  checked: Post-dominator merge selection, per-arm visitation, terminal-exit loop recovery, and fixed-point unreachable-control cleanup against focused regressions and regenerated pinned C#.
  found: Contract_Returns.sum/subtract now preserve both integer-normalization arms and return on every path; Contract_GoTo.test and testTry each render while(true) with reachable return; Contract_Returns no longer emits label_31/return p17_0; Contract_Continue recovers do { continue; } while(false); referenced irreducible labels remain intact.
  implication: The semantic loss hidden by the zero-fallback metric and both residual cosmetic artifacts are resolved without deleting required goto targets.

- timestamp: 2026-07-13T07:05:30+08:00
  checked: Final quality gates on the current worktree.
  found: cargo test --all-targets passes with 655 library tests passed and 1 ignored, plus 31 CLI, 3 corpus-replay, 25 artifact, 44 IR-pipeline, 6 SSA, and 4 typed-declaration tests; cargo test --no-default-features --lib passes 655/1; all-target Clippy passes with warnings denied; cargo fmt --check and git diff --check pass; JavaScript passes 1065 tests; Web passes 4 tests after installing its locked TypeScript 5.9.3 dependency; the exact 103-contract zero-fallback gate passes.
  implication: The control-flow changes preserve the surrounding Rust and JavaScript surfaces under default and no-default configurations.

- timestamp: 2026-07-13T07:05:30+08:00
  checked: Local Roslyn-gate prerequisites and transient corpus state.
  found: No Neo.SmartContract.Framework.dll exists under /home/neo, so the representative Roslyn test remains intentionally ignored; TestingArtifacts/devpack is absent after every pinned gate run.
  implication: Full generated-C# compile coverage remains an environment-dependent follow-up, while the repository worktree is free of the temporary corpus symlink.

## Baseline Corpus Impact

Before the fixes, the exact v3.10.0 gate reported one primary issue per fallback method:

| Primary issue | Opcode occurrences in affected methods | Methods | Contracts |
| --- | ---: | ---: | ---: |
| PICK | 32 | 31 | 6 |
| ROLL | 4 | 2 | 1 |
| REVERSEN | 2 | 2 | 2 |
| ENDFINALLY | not re-counted on exact tag | 31 primary / 32 affected | 8 |
| Lost value at STLOC1 | 1 | 1 | 1 |
| Total unique | n/a | 67 | 17 |

Dynamic-stack method groups: Contract_Integer.divRem* (8), Contract_Math.divRem* (8), Contract_Pattern recursive-pattern methods (3), Contract_PropertyMethod property methods (3), Contract_Record record methods (8), Contract_Reentrancy.noReentrancyByAttribute (1), both Contract_String.testEndWith overloads (2), Contract_Foreach.byteStringForeach (1), and Contract_NULL.nullCoalescingAssignment (1).

Finally method groups: Contract_Abort.testAbortInTry/testAbortInCatch (2), Contract_Assert testAssertInTry/testAssertInCatch/testAssertInFinally (3), Contract_Break.breakInTryCatch (1), Contract_Continue.continueInTryCatch (1), Contract_GoTo.testTryComplex (1), Contract_Returns.TryReturnInternal (1 private), Contract_TryCatch (18 methods), and Contract_WriteInTry (5 methods). Contract_Abort.testAbortInTry is also the sole STLOC1 primary failure, so it is counted once in the 67 unique methods.

## Resolution

root_cause: >-
  Five confirmed causes explain the limitation set. (1) The exact v3.10.0 corpus at 5b0b63880b6201ae3f974cc845e93a90462d8043 is available, but the repository gate normally omits it; its 103 pairs expose semantic fallbacks in 17 contracts and 67 unique methods. (2) PICK/ROLL/REVERSEN were unconditionally classified incomplete and apply_special popped their literal count without transforming the symbolic stack; all 38 pinned-tag operations have immediate literal counts, while truly runtime-variable counts require explicit stack-state expansion or a multi-result representation with VM fault semantics. (3) ENDFINALLY depends on a path-specific EndPointer or outstanding exception, but the original CFG had no exception-context/pending-continuation state and assumed one shared continuation. (4) Contract_Abort.testAbortInTry first diverged at ABORT/ABORTMSG-only callees because shared call contracts and CFG construction fabricated normal fallthrough, which then corrupted catch-entry exception payload joining. (5) Reachability-only merge selection accepted bypassable joins, shared one visited set between mutually exclusive arms, and treated every branch-headed natural loop as a conditional while; zero fallback therefore hid missing normalization paths, non-void fallthrough, and an unreachable loop update. CallFlagInvalid remains correctly rejected because 0x10 is outside CallFlags.All=0x0F. The earlier exact-tag Roslyn census (28 pass, 75 fail) also proves that zero fallback is not compile coverage.
fix: >-
  Applied exact literal PICK/ROLL/XDROP/REVERSEN lowering with fail-closed runtime-variable forms; modeled non-returning calls and per-edge exception payload entry; recovered nested finally ownership and parent continuation chains; represented exceptional finally entry and every saved normal continuation as distinct edge kinds; classified each ENDTRY transfer independently; made nonlocal returns/breaks/continues terminal within their structured arm; computed method post-dominators for merge selection; isolated branch-arm visitation; recovered branch-headed loops with terminal exits as while(true); and removed unreachable statements/unreferenced labels to a fixed point while preserving referenced irreducible labels.
verification: Exact v3.10.0 zero-fallback gate passes for all 103 contracts; all-target Rust tests pass (655 library passed, 1 ignored, including all 44 ir_pipeline tests); no-default library tests pass 655/1; all-target Clippy passes with warnings denied; JavaScript passes 1065 tests; Web passes 4 tests; formatting and diff checks pass. Contract_Break, Contract_Continue, Contract_GoTo, and Contract_Returns contain no structured fallback or unknown placeholder; their nested finalizers and transfers remain ordered, both GoTo loops retain reachable returns, the false do-while is recovered, and the dead pending-return label is absent. Transient corpus symlink removed. Roslyn compilation was not rerun because Neo.SmartContract.Framework.dll is unavailable locally.
files_changed: [src/decompiler/cfg/builder/finally.rs, src/decompiler/cfg/basic_block/terminator.rs, src/decompiler/cfg/builder/edges.rs, src/decompiler/cfg/graph/edge.rs, src/decompiler/cfg/ssa/builder.rs, src/decompiler/cfg/structure.rs, src/decompiler/cfg/tests/try_blocks.rs, tests/ir_pipeline.rs]
