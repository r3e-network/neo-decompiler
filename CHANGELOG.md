# Changelog

All notable changes to this project will be documented in this file. This
project adheres to [Semantic Versioning](https://semver.org/).

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
