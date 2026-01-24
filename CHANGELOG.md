# Changelog

All notable changes to this project will be documented in this file. This
project adheres to [Semantic Versioning](https://semver.org/).

## [0.5.0] - 2025-01-25

### Added

- **Static Single Assignment (SSA) transformation**: Complete SSA infrastructure
  for advanced bytecode analysis and optimization.
  - `SsaForm`: SSA representation with φ nodes and versioned variables
  - `DominanceInfo`: Immediate dominators, dominator tree, dominance frontiers
  - `SsaBuilder`: Two-phase SSA construction (φ placement + variable renaming)
  - `SsaStats`: Statistics (blocks, φ nodes, statements, variables)
  - SSA rendering via `Decompilation::render_ssa()`
  - Lazy SSA computation via `Decompilation::compute_ssa()`
- **Dominance analysis**: Cooper-Harvey-Kennedy algorithm for computing dominators
  with cycle detection and iteration limits for malformed CFG safety
- **Versioned variables**: Each variable definition gets a unique version (e.g.,
  `stack_0_0`, `stack_0_1`) ensuring single-assignment property
- **φ nodes**: Merge points at control flow confluence with predecessor-based
  operand selection
- Public API exports for SSA types: `DominanceInfo`, `PhiNode`, `SsaBlock`,
  `SsaConversion`, `SsaExpr`, `SsaForm`, `SsaStats`, `SsaStmt`, `SsaVariable`

### Changed

- SSA computation is lazy and optional, triggered via `Decompilation::compute_ssa()`
- SSA results are cached in `Decompilation` for efficient repeated access
- `SsaVariable::Display` hides version numbers from end users (internal detail)

### Internal

- Implemented two-phase SSA construction algorithm:
  1. φ node placement using dominance frontiers
  2. Variable renaming via dominator tree traversal
- Added comprehensive SSA unit tests (45+ tests for dominance, variables, φ nodes)
- Test count increased from 180 to 225 tests
- Fixed infinite loop bugs in dominance computation with proper cycle detection
- Added iteration limits to prevent DoS via malformed CFG input
- Integrated SSA with existing CFG infrastructure for seamless dataflow analysis

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
