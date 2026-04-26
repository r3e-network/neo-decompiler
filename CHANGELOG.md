# Changelog

All notable changes to this project will be documented in this file. This
project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.6.3] - 2026-04-26 (Rust) / [1.3.0] - 2026-04-26 (JS)

This release closes the last known behavior asymmetry between the Rust
and JavaScript implementations and bundles a comprehensive performance
and correctness pass on both sides. The two implementations are now
verified behavior-equivalent across 967 JS unit tests, 316 Rust lib
tests, and 4030 cross-implementation differential and corpus-replay
cases.

### Fixed

- **Rust: orphaned-label removal pass**: New `remove_orphaned_labels`
  postprocess pass strips `label_0xXXXX:` lines whose only `goto` /
  `leave` / `if {goto}` references were optimised away by the
  fallthrough-goto pass. Previously these labels remained as visible
  noise; the JS port already had this pass.
- **JS: ~17 silent manifest validation divergences**: `parseManifest`
  now rejects the same set of malformed manifests that Rust's serde
  rejects, instead of silently defaulting fields, coercing types, or
  storing raw values. Required-field violations throw
  `ManifestParseError` with `code: "MissingField"`; type violations
  use `code: "InvalidType"`. Specifically:
  - Required: top-level `name`; `abi.methods[i].name`, `returntype`;
    `parameters[j].name`, `type`; `abi.events[i].name`,
    `parameters[j].name`, `type`; `groups[i].pubkey`, `signature`;
    `permissions[i].contract`.
  - Type-strict: `abi` must be an object; `features.storage` /
    `payable` must be boolean (was `Boolean(...)` coercion);
    `supportedstandards` must be an array;
    `groups`/`permissions`/`abi.methods`/`abi.events`/`parameters`
    must be arrays when present; `method.offset` must be a number;
    `method.safe` must be a boolean; permission entries must be
    objects.
  - Removed the JS-specific `parameter.kind` fallback shim. Inputs
    using `kind` instead of the spec-required `type` now fail
    consistently in both implementations.

### Added

- **JS: `parseManifest` strict mode**: Optional second argument
  `parseManifest(json, { strict: true })` validates canonical wildcard
  values (`"*"`) in `permissions` and `trusts`, matching Rust's
  `from_json_str_strict` semantics.
- **JS: manifest size limit enforcement**: When input is a string,
  `parseManifest` now enforces `MAX_MANIFEST_SIZE = 0xFFFF`, matching
  Rust's `from_bytes` behaviour.
- **Differential fuzzing harness**: New `js/test/differential-fuzz.mjs`
  generates random NEFs covering all 15 operand encoding kinds and
  runs 21 hand-crafted edge-case probes (empty PUSHDATA,
  PUSHINT128/256, TRY variants, deep nesting, etc.) against both
  implementations. 200/200 random + 21/21 edge cases agree.
- **Corpus-replay harnesses**: New `js/test/corpus-replay.mjs` and
  `js/test/manifest-corpus-replay.mjs` feed every input from the saved
  fuzz corpora through both implementations. 414/414 NEF/decompile
  inputs and 3416/3416 manifest inputs produce matching outcomes.

### Changed (performance)

- **JS: comprehensive hot-path optimisations** (averaged over 3 runs):
  - Disassembly 10KB: `0.222ms → 0.135ms` (-39%)
  - Full-analysis pipeline: `1.74ms → 1.20ms` (-31%)
  - Syscall-heavy contract: `0.116ms → 0.062ms` (-47%)
  - 10KB contract end-to-end: `3.27ms → 2.60ms` (-21%)
  - 10000-iteration stress: `154ms → 116ms` (-25%)

  Highlights: replaced per-call `DataView` allocations with bit-op
  reads; cached PUSH-immediate operand singletons; replaced
  `Int8Array(...)[0]` with bit-math sign extension; switched
  `renderPseudocode` from `+=` cons-string accumulation to
  array-push + `join("\n")`; added `hex8`/`hex16`/`hex32` helpers
  using a 256-entry lookup table and applied them at every hot
  `toString(16).padStart(...)` site; converted chained
  `if (mnemonic === ...)` cascades to `switch` in
  `tryControlStatement`, `tryStackShapeOperation`,
  `tryUnaryExpression`, `tryBinaryExpression`, and `inferTypes`;
  replaced regex-based `slotIndexFromMnemonic` with prefix-check +
  char-code digit parse (called 6×/instruction); fixed
  `tryLiftSimpleSwitch` O(n²) per-case scan via offset→index Map;
  eliminated 4 `slice().findIndex()` patterns in
  `tryLiftSimpleTryBlock`; dropped redundant recursion in postprocess
  `rewriteExpr` (O(N²) → O(N)); single-pass temp scan in
  `collectInlineCandidates` (O(K·N) → O(N)); lazy
  `state.programIndexByOffset` cache in `inferUnpackElementCount`;
  in-place `REVERSE3`/`REVERSE4`/`REVERSEN` swaps; eliminated
  `scriptHashLE` slice-then-reverse copy.
- **Rust: O(log n) lookups in `MethodTable`**: New `largest_le`
  helper using `partition_point` replaces `filter().max()` linear
  scans in the CallA fixpoint loop and
  `resolve_argument_target_for_method`. `resolve_internal_target`
  switched from linear `iter().find()` to `binary_search_by_key`.
  `callers_by_target` switched from `BTreeMap<usize, Vec<usize>>`
  to `BTreeMap<usize, BTreeSet<usize>>`, eliminating the redundant
  `Vec::contains` check on each fixpoint iteration. For a contract
  with K method starts and N CallA instructions, fixpoint complexity
  drops from O(K·N) to O(N·log K) per iteration.
- **Rust: postprocess scan-loop clean-up**: Replaced
  `iter().enumerate().skip(start)` with direct `(start..len)` range
  loops in `next_code_line`, `find_matching_brace`, and
  `find_matching_close`. The `.skip()` adapter on slice iterators is
  not always specialised to O(1), so each call previously paid an
  O(start) startup tax.

## [1.2.1] - 2026-04-08 (JS only)

### Changed

- **JS: O(n²) → O(n) in `eliminateIdentityTemps` and `collapseTempIntoStore`**: Pre-scan temp usage counts/first-occurrence indices to replace per-temp forward scans with O(1) lookups. 31-42x faster on temp-heavy contracts.
- **JS: eliminate O(n²) `.trim()` in `rewriteForLoops`**: Pre-trim statements once instead of calling `.trim()` on every line for every `while`-scan in `findMatchingClose`. 1.7x faster on 50KB contracts (441ms → 260ms).
- **JS: regex cache for identifier helpers**: `containsIdentifier`, `countIdentifier`, and `replaceIdentifier` now cache compiled regexps per identifier, avoiding recompilation on every call.
- **JS: single-pass method partition**: `buildMethodGroups` now partitions instructions in a single walk instead of O(groups × instructions) filter calls.
- **JS: O(n) blank-line removal**: Final cleanup uses write-pointer compaction instead of O(n²) splice-in-loop.

## [0.6.2] - 2026-04-07

### Fixed

- **JS: PUSHA signed offset**: PUSHA operand is now correctly interpreted as signed I32 for backward pointer resolution, fixing wrong output for contracts using indirect backward calls via `CALLA`.
- **JS: disassembler bounds checks**: All operand reads (I8, I16, I32, I64, Jump8, Jump32, U16, U32, Syscall) now throw a proper `DisassemblyError` with code `UnexpectedEof` on truncated bytecode instead of an unhelpful `RangeError`.
- **JS: inline use-count**: `collectInlineCandidates` now counts identifier occurrences instead of using a boolean check, preventing incorrect inlining of temps used multiple times in a single expression.
- **JS: `negateCondition` compound logic**: Compound conditions with `&&`/`||` are now wrapped with `!(...)` instead of incorrectly negating only the first operator.
- **JS: compiler field null handling**: NEF compiler field parsing now stops at the first null byte (matching Rust behavior) instead of stripping only trailing nulls.

### Added

- **JS: `inlineSingleUseTemps` option**: New opt-in postprocess pass that inlines single-use temporaries into their use sites, producing cleaner output. Enable via `decompileHighLevelBytes(bytes, { inlineSingleUseTemps: true })`.
- **Rust: C# `[SupportedStandards]` attribute**: C# output now emits a proper `[SupportedStandards(...)]` attribute instead of a comment.
- **Native contract methods**: Added missing `Decimals` and `Symbol` methods to GasToken and NeoToken definitions.

### Changed

- **Rust: manifest extra rendering**: Both C# header and high-level summary now emit all string-valued manifest extra fields instead of only `author` and `email`. Keys are now matched case-sensitively (matching the JSON exactly).

## [0.6.1] - 2026-03-27

### Fixed

- **WASM `initPanicHook` binding**: Added missing `js_name = initPanicHook` attribute to `web.rs`, fixing a runtime `TypeError` when calling `init()` from the TypeScript wrapper.
- **Test panics on missing devpack fixtures**: Two unit tests (`csharp_trims_initslot_boundaries`, `high_level_trims_initslot_boundaries`) now gracefully skip when the optional `TestingArtifacts/devpack/` directory is absent instead of panicking.
- **Incorrect `MAX_NEF_FILE_SIZE` doc comment**: Corrected "10 MiB" to "1 MiB" in the re-export doc comment (`src/decompiler.rs`), matching the actual constant `0x10_0000`.
- **Disassembler split doc comment**: Consolidated the doc comment above the `#[derive]` attribute so the full description appears in generated docs.
- **JS security tests calling wrong function**: Three tests in `security.test.mjs` now correctly call `decompileHighLevelBytesWithManifest` instead of passing a manifest as options to `decompileHighLevelBytes`.
- **JS unused code cleanup**: Removed dead `stack` variable and unused `jumpTarget`/`wrapExpression` imports in `high-level.js`. Removed dead 4th argument from all `slice()` call sites in `nef.js`.

### Changed

- **JS package repository URLs**: Fixed `package.json` URLs from `neo-ngd` to `r3e-network` to match the actual repository.
- **JS README browser claim**: Corrected feature list to state "Node.js 18+, Deno, Bun" instead of claiming browser support, since `util.js` uses `node:crypto`.
- **JS shared utilities**: Extracted duplicated `scanSlotCounts`, `scanStaticSlotCount`, and `slotIndex` functions from `types.js` and `xrefs.js` into shared `util.js`.
- **SECURITY.md**: Corrected branch reference from `main` to `master`.
- **README roadmap ordering**: Swapped v0.5.x and v0.6.x sections to chronological order.
- **Plan documents**: Added completion status markers to all six plan files.
- **RELEASING.md**: Added step for JS package version consideration during releases.

### Added

- **CI JS package job**: Added `js-package` job to `ci.yml` to run JS tests on every push/PR.
- **Justfile JS/web tasks**: Added `js-test` and `web-test` recipes; included `js-test` in the `ci` recipe.

## [0.6.0] - 2026-02-16

### Added

- **Overflow collapse pass**: Automatically collapses verbose Neo C# compiler int32/int64 overflow-check patterns into clean expressions.
- **While-loop recovery**: Backward `JMP` instructions are now recognized as `while` loops with proper condition extraction.
- **Goto-to-while conversion**: Postprocess pass converts forward goto patterns into structured `while` loops.
- **Switch-break goto elimination**: Removes residual `goto` statements inside recovered `switch` blocks.
- **Continue-in-try detection**: `ENDTRY` instructions targeting loop conditions are now emitted as `continue` statements.
- **Empty-if-else inversion**: Empty `if` bodies with non-empty `else` are inverted to remove the dead branch.
- **Identity temp elimination**: Redundant `let tN = locM; locM = tN;` patterns are collapsed.
- **Temp-into-store collapsing**: `let tN = expr; locM = tN;` is simplified to `let locM = expr;`.
- **Temp-into-return collapsing**: `let tN = expr; return tN;` is simplified to `return expr;`.
- **Stack comment stripping**: Verbose `// rotate top three stack values` comments are removed from output.
- **`if true { }` collapse**: Constant-true conditionals are unwrapped to their body.
- **CALL/CALL_L method boundary detection**: Internal call targets are now recognized as method entry points even without `INITSLOT`.
- **PUSHA function pointer rendering**: `PUSHA` targets render as function pointer references instead of raw integers.
- **Implicit else for noreturn branches**: Branches ending in `abort`/`throw` now correctly suppress else emission.
- **PUSHINT128 decimal display**: 128-bit push immediates render in decimal for readability.

### Fixed

- **Try/catch nesting**: Catch/finally blocks no longer nest incorrectly inside else branches; processing order corrected in `advance_to()`.
- **Try-exit stack restoration**: `ENDTRY` properly restores the try-exit stack outside the pending-closers gate.
- **Nested try/catch brace balancing**: Deeply nested try/catch structures now produce correctly balanced braces.
- **PACK/UNPACK element ordering**: Array pack/unpack operations emit elements in correct stack order.
- **CALLA resolution**: `CALLA` (indirect call) now resolves to the correct target method.
- **Break-in-try**: `break` statements inside try blocks are correctly detected.
- **Tail-call JMP recognition**: JMP instructions at method boundaries are recognized as tail calls rather than control flow.
- **Self-referencing ENDTRY**: ENDTRY instructions targeting themselves no longer cause infinite loops.
- **If-condition inlining**: Condition temporaries are properly inlined into if-statement headers.

### Changed

- **Switch detection threshold**: Lowered minimum guarded-goto cases from 6 to 2 for better small-switch recovery.
- **Edge lookup optimization**: `collect_post_ret_method_offsets` now uses HashMap indices for O(1) lookups instead of O(n×m) linear scans.
- Comprehensive audit and validation against 101 Neo N3 devpack contracts.

## [0.5.2] - 2026-02-10

### Added

- New global CLI flag `--strict-manifest` to enforce strict manifest validation in commands that load manifests (for example `info` and `decompile`).
- New strict parsing helpers in the public API:
  - `ContractManifest::from_json_str_strict(...)`
  - `ContractManifest::from_file_strict(...)`

### Changed

- `disasm` now executes a direct bytecode disassembly path without running the full decompilation analysis pipeline.
- Added a dedicated manifest validation error path (`manifest validation error: ...`) for strict-mode failures so malformed wildcard-like values are clearly reported.

### Fixed

- Entry-point rendering now always starts at the real script entry offset; when manifest method offsets do not align, high-level and C# outputs emit a synthetic script-entry method instead of dropping entry bytecode.

## [0.5.1] - 2026-02-07

### Added

- High-level control-flow lifting now covers `CALLA`/`CALLT`/`CALL`/`CALL_L`, comparison branches, and `JMP`/`JMPIF`/`ENDTRY` long forms with label-based transfer placeholders.
- Added regression tests for native-contract metadata completeness, including latest upstream contracts and legacy token contracts used by existing fixtures.

### Changed

- Native-contract metadata generation now merges local `neo_csharp` sources with upstream Neo sources, preventing stale snapshots from dropping current contracts.
- Syscall metadata generation now uses the same local+upstream merge strategy to keep bundled syscall catalogs current.
- Updated metadata coverage documentation and release snippets in the README.

## [0.5.0] - 2025-01-30

### Added

- **SSA Transformation**: Complete Static Single Assignment form implementation
  - φ (phi) node insertion at dominance frontiers
  - Variable versioning with subscript notation
  - Proper handling of all Neo VM stack operations
- **Dominance Analysis**: Full dominance tree construction
  - Immediate dominator computation (Cooper-Harvey-Kennedy algorithm)
  - Dominance frontier calculation for φ placement
  - Dominator tree visualization support
- **SSA Rendering**: Human-readable SSA output
  - Statistics display (blocks, φ nodes, variables)
  - Clean variable naming with version subscripts

## [0.4.1] - 2025-12-14

### Changed

- Documented analysis output in the README and added rustdoc examples for `decompiler::analysis`.

### Internal

- Expanded unit test coverage for v0.4.x analysis outputs (call graph, xrefs, type inference)
  and high-level post-processing passes (indexing + switch recovery).

## [0.4.0] - 2025-12-14

### Added

- New analysis layer exposed via `Decompilation`:
  - `call_graph`: inter-procedural relationships across `CALL*`, `CALLT`, and `SYSCALL`
  - `xrefs`: local/argument/static slot read/write offsets
  - `types`: best-effort primitive/collection type inference for locals/args/statics
- `neo-decompiler decompile --format json` now includes an `analysis` object (and the decompile JSON schema documents it).

### Changed

- High-level output rewrites collection helpers into more idiomatic syntax:
  - `PICKITEM` becomes bracket indexing (`a[b]`)
  - `SETITEM` becomes bracket assignment (`a[b] = c`)
  - `HASKEY` becomes `has_key(a, b)`
- High-level output can now rewrite equality-based `if`/`else` chains into `switch`/`case` blocks (conservative).

## [0.3.1] - 2025-12-14

### Changed

- High-level decompiler output is now brace-indented for more readable nested blocks.
- Conservative temp inlining can now substitute trivial literals/identifiers into `if`/`while`/`for`
  headers while still avoiding large expression inlining that harms readability.
- Rewrite simple `x = x + y` / `x = x - y` patterns into compound assignments (`x += y`, `x -= y`)
  in the high-level view, including within `for` headers.

### Fixed

- Removed stray blank lines caused by post-processing passes clearing lifted statements.
- CLI commands no longer panic on broken pipes when output is piped to tools like `head`.

## [0.3.0] - 2025-12-13

### Added

- **Control Flow Graph (CFG) infrastructure**: New `cfg` module providing
  explicit basic block representation for control flow analysis.
  - `BasicBlock`, `BlockId`, `Terminator` types for block-level analysis
  - `Cfg` graph structure with edges, successors/predecessors queries
  - `CfgBuilder` for constructing CFG from instruction streams
  - DOT format export via `Cfg::to_dot()` for Graphviz visualization
  - CLI export via `neo-decompiler cfg <contract.nef> > cfg.dot`
  - Reverse post-order traversal for dataflow analysis
- **Dead code detection helpers**: Reachability analysis via `Cfg::reachable_blocks()` /
  `Cfg::unreachable_blocks()` and unreachable block highlighting in DOT output.
- **Expression simplification helpers**: New `ir::simplify` module with algebraic
  simplifications for cleaner IR expressions (usable in downstream tooling and future passes).
  - Arithmetic identities: `x + 0 → x`, `x * 1 → x`, `x ** 0 → 1`
  - Boolean simplifications: `x == true → x`, `!!x → x`, `true && x → x`
  - Bitwise identities: `x ^ x → 0`, `x & 0 → 0`, `x | 0 → x`
- **Else-if chain detection**: Post-processing pass that collapses nested
  `} else { if ... {` patterns into cleaner `} else if ... {` syntax.
- **Loop header temp inlining**: Inline condition/increment temporaries into `while`/`for`
  headers for cleaner loop output.
- **Single-use temp inlining** (experimental, disabled by default): Optional pass
  for inlining temporary variables used exactly once (enable via
  `neo-decompiler decompile --inline-single-use-temps` or
  `Decompiler::with_inline_single_use_temps(true)`).
- CFG is now part of `Decompilation` result with `cfg_to_dot()` helper method.
- Public API exports for CFG types: `BasicBlock`, `BlockId`, `Cfg`, `CfgBuilder`,
  `Edge`, `EdgeKind`, `Terminator`.

### Changed

- MSRV bumped to Rust 1.74 to match CLI dependency requirements.
- CI now validates MSRV + `--no-default-features` builds and fails on rustdoc warnings.

### Internal

- Added 17 new CFG unit tests covering exit blocks, predecessors/successors,
  edge counting, and terminator properties.
- Added 17 expression simplification tests.
- Refactored large modules into focused submodules (`cfg::graph`, `decompiler` API, `error`, `instruction`, `nef::parser`, `manifest::model`, high-level emitter internals, disassembler operand handling, postprocess inliners).
- Test count increased from 130 to 180+ tests.

## [0.2.0] - 2025-12-13

### Changed

- **BREAKING**: Add `#[non_exhaustive]` to public enums and structs (`Error`,
  `NefError`, `DisassemblyError`, `ManifestError`, `Decompilation`, `NefFile`,
  `NefHeader`, `MethodToken`, `ContractManifest`, `Instruction`, `Operand`) for
  semver safety. Downstream code using exhaustive pattern matching will need
  wildcard arms.
- `OpCode::mnemonic()` now returns `&'static str` instead of `String`, removing
  per-call heap allocations on hot paths.

### Added

- New `DisassemblyError::OperandTooLarge` variant for rejecting oversized
  `PUSHDATA*` operands (1 MiB limit) to prevent memory exhaustion attacks.
- Checked arithmetic in disassembler slice operations to avoid integer overflow
  on malformed input.
- `#[must_use]` attributes on `Decompiler::new()`, `Disassembler::new()`,
  `NefParser::new()`, and related constructors.
- Comprehensive rustdoc with examples for `NativeMethodHint`, manifest
  `describe_*` helpers, `Instruction::new`, and `Disassembler` API.
- Crate-level `#![warn(missing_docs)]` and `#![warn(rust_2018_idioms)]` lints.
- Edge-case tests for invalid UTF-8/JSON manifests and oversized operands.

### Fixed

- CLI `expect()` calls in schema handling replaced with proper error propagation
  to avoid panics on edge cases.
- Removed redundant `#[must_use]` on `Result`-returning functions (clippy
  `double_must_use` warnings).

### Internal

- Modularized codebase: split monolithic files (`cli.rs`, `decompiler.rs`,
  `nef.rs`, etc.) into focused submodules for maintainability.
- Test count increased from 102 to 130 (107 unit + 16 CLI + 6 doctests + 1
  artifact).

## [0.1.0] - 2025-11-26

### Added

- Document MSRV (1.70) and add installation instructions in the README.
- Bundle dual-license texts and polish crate metadata (`homepage`,
  `documentation`, README links).
- Add contributor and community health guidelines (CONTRIBUTING,
  CODE_OF_CONDUCT, SECURITY, SUPPORT, RELEASING) and a developer `Justfile`.
- Centralise hexadecimal formatting utilities shared by the CLI, decompiler,
  and instruction display code.
- Teach `neo-decompiler info` to print the same detailed method-token metadata
  as the `tokens` subcommand for a more consistent UX.
- Print method tokens (from both the CLI and the high-level contract view)
  with human-readable call flag names (ReadStates, AllowCall, etc.) alongside
  the raw bitmask for quicker audits.
- Annotate recognized native contract hashes with canonical `Contract::Method`
  labels so you can immediately see which native entrypoint a token targets.
- Emit inline warnings when a method token references a known native contract
  but the method name does not match any published entry points.
- Compute and display the contract script hash (Hash160) in both little-endian
  and canonical forms so NEF dumps can be cross-checked against explorer data.
- Support `neo-decompiler info --format json` to produce a structured report
  (script hash, checksum, manifest ABI summary, native tokens/warnings) suitable
  for automation, plus `neo-decompiler tokens --format json`,
  `neo-decompiler disasm --format json`, and
  `neo-decompiler decompile --format json` for machine-friendly dumps of method
- Add a global `--json-compact` flag so any JSON output can omit extra
  whitespace when scripting or piping into other tooling.
- Include an `operand_kind` field in JSON disassembly/decompile output so tool
  consumers can distinguish jumps, immediates, booleans, syscalls, etc. without
  parsing the rendered operand string.
- Surface the resolved manifest path in JSON `info`/`decompile` output so
  consumers know which ABI file was used.
- Surface manifest permissions/trusts consistently across text, JSON, and
  high-level outputs so ABI metadata matches README claims.
- Emit JSON schema files (docs/schema) and reference them in the README so
  integrations can validate payloads.
- Document schema versioning/validation steps and extend tests so every JSON
  command is validated against the published schemas.
- Surface manifest groups (committee pubkeys/signatures) in both text and JSON
  outputs, plus document the new field in the README and schemas.
- Ship the JSON schema documents inside the binary (`neo-decompiler schema …`)
  so automation can fetch canonical schemas without cloning the repo, while the
  command now honours `--json-compact`, lists schemas (with versions) via text
  or JSON, includes the on-disk schema paths, supports `--validate path.json`,
  and can persist files via `--output`.
- Aggregate native-contract warnings into a top-level `warnings` array in every
  JSON report so scripting environments no longer need to parse free text.
