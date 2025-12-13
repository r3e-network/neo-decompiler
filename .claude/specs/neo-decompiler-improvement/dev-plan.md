# Neo-Decompiler Improvement - Development Plan

## Overview
Enhance neo-decompiler with selective output generation, optimized lookups, unified rendering logic, comprehensive test coverage, and advanced control-flow analysis.

## Task Breakdown

### Task 1: Selective Decompilation Outputs (API + CLI)
- **ID**: task-1
- **Description**: Refactor `Decompiler::decompile_bytes_with_manifest` to support selective output generation (disassembly only, high-level only, C# only) instead of always computing all three formats. Extend CLI with flags like `--output-format=<pseudo|high-level|csharp|all>` to avoid unnecessary computation when only one format is needed.
- **File Scope**:
  - `src/decompiler.rs` (core API changes)
  - `src/lib.rs` (public API exposure)
  - `src/cli/runner/decompile.rs` (CLI flag integration)
  - `tests/` (integration tests for CLI behavior)
- **Dependencies**: None
- **Test Command**:
  ```bash
  cargo test --lib decompiler -- --nocapture
  cargo test --test cli_smoke -- --nocapture
  cargo tarpaulin --out Stdout --skip-clean --packages neo-decompiler --exclude-files 'tests/*'
  ```
- **Test Focus**:
  - Verify that requesting only high-level output does NOT trigger C# rendering
  - Verify that requesting only disassembly does NOT invoke control-flow lifting
  - Validate CLI flags produce correct output format
  - Ensure API backward compatibility with default behavior (all outputs)
  - Test error handling when invalid format is requested

### Task 2: Faster Syscall/Native Contract Lookup
- **ID**: task-2
- **Description**: Replace linear `iter().find()` in `src/syscalls.rs` and `src/native_contracts.rs` with compile-time constant structures (e.g., `phf` crate for perfect hash maps or sorted arrays with binary search). Maintain identical public API signature (`lookup(...) -> Option<&'static ...>`) for zero breakage.
- **File Scope**:
  - `src/syscalls.rs` (replace SYSCALL_TABLE linear scan)
  - `src/native_contracts.rs` (replace NATIVE_CONTRACTS linear scan)
  - `Cargo.toml` (add `phf` dependency if chosen)
  - `src/syscalls/tests.rs` (new test module)
  - `src/native_contracts/tests.rs` (expand existing tests)
- **Dependencies**: None
- **Test Command**:
  ```bash
  cargo test syscalls::tests -- --nocapture
  cargo test native_contracts::tests -- --nocapture
  cargo bench --bench lookup_benchmarks -- --baseline before
  cargo tarpaulin --out Stdout --skip-clean --lib -- syscalls native_contracts
  ```
- **Test Focus**:
  - Roundtrip: All known syscall hashes resolve to correct names and vice versa
  - Unknown hash returns `None` without panic
  - `returns_value` field is correctly populated for all syscalls
  - Benchmark confirms O(1) lookup vs O(n) scan (3-5x speedup expected)
  - Native contract hash lookups remain accurate after refactor

### Task 3: Renderer DRY Refactor
- **ID**: task-3
- **Description**: Extract shared rendering logic from `src/decompiler/high_level/render/body.rs` and `src/decompiler/csharp/render/body.rs` into a unified trait or generic function. The refactor should accept configurable indent style and statement mappers while preserving exact output format of both renderers.
- **File Scope**:
  - `src/decompiler/high_level/render/body.rs` (refactor to use shared logic)
  - `src/decompiler/csharp/render/body.rs` (refactor to use shared logic)
  - `src/decompiler/render/common.rs` (new shared module)
  - `src/decompiler/tests/high_level/mod.rs` (regression tests)
  - `src/decompiler/tests/csharp/mod.rs` (regression tests)
- **Dependencies**: None
- **Test Command**:
  ```bash
  cargo test decompiler::tests::high_level -- --nocapture
  cargo test decompiler::tests::csharp -- --nocapture
  cargo test decompiler::render::common -- --nocapture
  cargo tarpaulin --out Stdout --skip-clean --lib -- decompiler::render
  ```
- **Test Focus**:
  - Golden file tests: Both renderers produce byte-identical output before and after refactor
  - Verify indent configuration works (2-space vs 4-space)
  - Ensure custom statement mappers (e.g., variable naming) are correctly applied
  - Test empty method body rendering
  - Validate edge cases (nested loops, exception handling blocks)

### Task 4: Add Missing Unit Tests
- **ID**: task-4
- **Description**: Achieve ≥90% line coverage for `src/syscalls.rs`, `src/instruction.rs`, and `src/native_contracts.rs` by adding direct unit tests. Focus on roundtrip invariants (hash ↔ name), edge cases (unknown opcodes, malformed instructions), and defaults (e.g., `returns_value` for unknown syscalls).
- **File Scope**:
  - `src/syscalls/tests.rs` (new comprehensive test suite)
  - `src/instruction/tests.rs` (new opcode parsing tests)
  - `src/native_contracts/tests.rs` (expand coverage)
  - `tests/fixtures/` (add malformed bytecode samples)
- **Dependencies**: task-2 (if syscall/native_contracts implementation changes)
- **Test Command**:
  ```bash
  cargo test --lib -- --nocapture
  cargo tarpaulin --out Stdout --skip-clean --lib --line --branch
  cargo tarpaulin --out Stdout --skip-clean --lib --exclude-files 'src/main.rs' --target-dir target/tarpaulin
  ```
- **Test Focus**:
  - Syscalls: Test all 50+ known syscall hashes resolve correctly; test unknown hash behavior
  - Instructions: Parse all NeoVM opcodes, validate operand extraction (e.g., PUSHINT operands)
  - Native contracts: Verify all 8 native contract hashes and method IDs
  - Roundtrip: `opcode -> bytes -> opcode` invariant holds for all instructions
  - Edge cases: Invalid opcode 0xFF, truncated instruction bytes
  - Coverage report confirms ≥90% line + branch coverage for target modules

### Task 5: Control-Flow Lifting Enhancements
- **ID**: task-5
- **Description**: Extend control-flow analysis in `src/decompiler/high_level/emitter/control_flow/` to lift more conditional branch patterns beyond the current JMPIFNOT-centric approach. Support JMP-based conditionals, inverted conditions, and chained if-else-if structures.
- **File Scope**:
  - `src/decompiler/high_level/emitter/control_flow/**` (pattern matching logic)
  - `src/decompiler/high_level/emitter/mod.rs` (integration)
  - `src/decompiler/tests/high_level/control_flow.rs` (new test cases)
  - `tests/fixtures/control_flow/` (add complex branch NEF samples)
- **Dependencies**: None
- **Test Command**:
  ```bash
  cargo test decompiler::tests::high_level::control_flow -- --nocapture
  cargo test decompiler::high_level::emitter -- --nocapture
  cargo tarpaulin --out Stdout --skip-clean --lib -- decompiler::high_level::emitter
  ```
- **Test Focus**:
  - Lift `JMP`-based if-then (forward jump over then-block)
  - Lift `JMPIF` patterns (inverted from JMPIFNOT)
  - Detect and lift if-else-if chains (multiple sequential conditionals)
  - Verify correct basic block boundaries and jump target resolution
  - Test nested if inside loop correctness
  - Ensure no regressions in existing JMPIFNOT-based lifting

## Acceptance Criteria
- [ ] API supports selective output generation (task-1) with CLI integration
- [ ] Syscall/native contract lookups use O(1) structure (task-2) with measurable performance gain
- [ ] Rendering logic is unified with zero output regression (task-3)
- [ ] All target modules (syscalls, instruction, native_contracts) have ≥90% line + branch coverage (task-4)
- [ ] Control-flow analysis lifts JMP/JMPIF patterns in addition to JMPIFNOT (task-5)
- [ ] All unit tests pass: `cargo test --all-targets`
- [ ] Code coverage ≥90% overall: `cargo tarpaulin --out Stdout --skip-clean`
- [ ] No clippy warnings: `cargo clippy --all-targets -- -D warnings`
- [ ] Documentation builds: `cargo doc --no-deps`

## Technical Notes
- **Backward Compatibility**: Task 1 API changes must default to current behavior (all outputs) to avoid breaking existing library consumers
- **Performance Baseline**: Establish benchmark baseline for Task 2 before refactoring (use `cargo bench` with criterion.rs)
- **Golden Files**: Task 3 requires snapshot tests to prevent rendering regressions; consider using `insta` crate
- **Coverage Tool**: Use `cargo-tarpaulin` on Linux or `cargo-llvm-cov` on other platforms for accurate coverage metrics
- **Test Isolation**: Tasks 1, 2, 3, 5 have no dependencies and can be executed in parallel by different contributors
- **Tech Stack**: Rust 1.70+, no unsafe code policy, maintain zero-dependency decompiler core (syscall optimization may add `phf` build dependency)
- **Code Review**: Each task should produce a unified diff for review before merge to ensure maintainability standards
