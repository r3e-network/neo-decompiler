# Neo N3 Contract Decompiler Refactoring - Development Plan

## Overview
Complete refactoring of neo-decompiler crate to fix compilation errors, improve architecture quality, enhance decompilation capabilities, and achieve ≥90% test coverage.

## Task Breakdown

### Task 1: Fix Critical Compilation Errors
- **ID**: task-1
- **Description**: Create missing `src/nef/parser.rs` module with `NefParser` struct and required methods (`parse()`, `calculate_checksum()`). Ensure the module properly exports types and integrates with existing `method_tokens.rs`. This is the critical blocking issue preventing any compilation.
- **File Scope**:
  - `src/nef/parser.rs` (new file - main parser implementation)
  - `src/nef/parser/mod.rs` (create if needed for module structure)
  - `src/nef.rs` (verify mod declaration at line 19)
  - `src/nef/parser/method_tokens.rs` (verify integration)
- **Dependencies**: None
- **Test Command**: `cargo build --package neo-decompiler && cargo test --package neo-decompiler --lib nef::parser -- --nocapture`
- **Test Focus**:
  - NefParser successfully parses valid NEF files with correct magic number, compiler, and method tokens
  - Checksum calculation matches Neo N3 reference implementation
  - Error handling for malformed NEF files (invalid magic, truncated data, checksum mismatch)
  - Integration with existing method token parsing

### Task 2: Introduce Typed IR System
- **ID**: task-2
- **Description**: Replace `Vec<String>` statement representation with typed intermediate representation (IR) structs. Define IR nodes for expressions (literals, binary ops, function calls), statements (assignments, returns), and control flow constructs (if/else, while loops) to enable semantic analysis and transformations.
- **File Scope**:
  - `src/decompiler/ir/` (new directory)
  - `src/decompiler/ir/mod.rs` (module root)
  - `src/decompiler/ir/expression.rs` (expression IR nodes)
  - `src/decompiler/ir/statement.rs` (statement IR nodes)
  - `src/decompiler/ir/control_flow.rs` (control flow IR nodes)
  - `src/decompiler/high_level/emitter.rs` (update to use IR instead of Vec<String>)
  - `src/decompiler/csharp/emitter.rs` (update to use IR)
- **Dependencies**: task-1
- **Test Command**: `cargo test --package neo-decompiler --lib decompiler::ir -- --nocapture && cargo tarpaulin --out Stdout --lib --exclude-files 'tests/*' --packages neo-decompiler --timeout 120`
- **Test Focus**:
  - IR nodes correctly represent all NEF opcode semantics (arithmetic, stack ops, calls, jumps)
  - IR transformations preserve semantics (constant folding, dead code elimination)
  - Conversion from IR to high-level language string representation
  - Round-trip tests where possible

### Task 3: Refactor Architecture and Eliminate Code Duplication
- **ID**: task-3
- **Description**: Break down `HighLevelEmitter` god object into focused, single-responsibility components: stack simulator, control flow analyzer, IR builder, and formatter. Extract duplicated instruction slicing logic (manifest offset-based method extraction) and emitter run loop (indentation handling) into shared utilities in `src/decompiler/shared/`.
- **File Scope**:
  - `src/decompiler/high_level/emitter.rs` (refactor into smaller modules)
  - `src/decompiler/high_level/stack_sim.rs` (new - stack simulation component)
  - `src/decompiler/high_level/control_flow.rs` (new - CFG analysis)
  - `src/decompiler/high_level/ir_builder.rs` (new - IR construction)
  - `src/decompiler/shared/` (new directory for shared utilities)
  - `src/decompiler/shared/mod.rs`
  - `src/decompiler/shared/instruction_slicer.rs` (extract from methods.rs files)
  - `src/decompiler/shared/render_loop.rs` (extract emitter run loop logic)
  - `src/decompiler/csharp/render/methods.rs` (refactor to use shared instruction_slicer)
  - `src/decompiler/high_level/render/methods.rs` (refactor to use shared instruction_slicer)
  - `src/decompiler/csharp/render/body.rs` (refactor to use shared render_loop)
  - `src/decompiler/high_level/render/body.rs` (refactor to use shared render_loop)
- **Dependencies**: task-2
- **Test Command**: `cargo test --package neo-decompiler --lib decompiler::high_level -- --nocapture && cargo test --package neo-decompiler --lib decompiler::shared -- --nocapture && cargo clippy -- -D warnings`
- **Test Focus**:
  - Each refactored component has single, well-defined responsibility
  - Shared utilities successfully eliminate code duplication (verify no duplicated logic remains)
  - Refactored code maintains existing functionality (regression tests)
  - Stack simulation correctly tracks operand types and stack depth
  - Control flow graph accurately represents method structure

### Task 4: Improve Opcode Semantic Lifting
- **ID**: task-4
- **Description**: Implement semantic lifting for all conditional jump opcodes (JMPEQ, JMPNE, JMPGT, JMPGE, JMPLT, JMPLE) to structured if/while statements with proper comparison operators. Reconstruct CALL/CALL_L/CALLA/CALLT as function call expressions instead of comments. Handle PUSHINT128/PUSHINT256 as BigInteger literals with proper formatting instead of raw hex strings.
- **File Scope**:
  - `src/decompiler/high_level/emitter.rs` (or refactored components from task-3)
  - `src/decompiler/high_level/opcodes/` (new directory for opcode-specific lifting)
  - `src/decompiler/high_level/opcodes/mod.rs`
  - `src/decompiler/high_level/opcodes/jumps.rs` (conditional jump lifting)
  - `src/decompiler/high_level/opcodes/calls.rs` (call reconstruction)
  - `src/decompiler/high_level/opcodes/constants.rs` (large integer handling)
  - `src/decompiler/high_level/control_flow.rs` (enhance CFG analysis for loop detection)
- **Dependencies**: task-2, task-3
- **Test Command**: `cargo test --package neo-decompiler --lib decompiler::high_level::opcodes -- --nocapture && cargo test --package neo-decompiler --test integration_decompile -- --nocapture`
- **Test Focus**:
  - All conditional jumps correctly reconstructed as if/else structures with appropriate comparison operators
  - Loop patterns (back-edges in CFG) detected and rendered as while/for loops
  - Function calls properly identified with target addresses and parameter counts
  - PUSHINT128/PUSHINT256 render as readable BigInteger literals (e.g., `BigInteger("123456789...")`)
  - Edge cases: nested conditionals, loop-within-loop, call chains

### Task 5: Enhance C# Code Generation Quality
- **ID**: task-5
- **Description**: Make C# output either (a) fully compilable by mapping decompiled constructs to real Neo.SmartContract.Framework types/methods, OR (b) clearly labeled as pseudocode with explicit documentation. Replace all placeholder expressions (`abs(x)`, `Map()`, `syscall("...")`) with either framework equivalents (e.g., `StdLib.Abs()`, `new Map<K,V>()`) or pseudocode markers with explanatory comments.
- **File Scope**:
  - `src/decompiler/csharp/**/*.rs` (all C# generation files)
  - `src/decompiler/csharp/framework_mapping.rs` (new - if pursuing compilable option)
  - `src/decompiler/csharp/render/expressions.rs` (update expression formatting)
  - `src/decompiler/csharp/render/statements.rs` (update statement formatting)
  - `src/decompiler/csharp/render/types.rs` (Neo type mapping)
  - `README.md` (document C# output format and limitations)
  - `docs/csharp-output-format.md` (new - detailed C# output documentation)
- **Dependencies**: task-4
- **Test Command**: `cargo test --package neo-decompiler --lib decompiler::csharp -- --nocapture && cargo test --package neo-decompiler --test csharp_output_validation -- --nocapture`
- **Test Focus**:
  - Generated C# either compiles with Neo.SmartContract.Framework reference, OR is clearly marked as pseudocode with documentation
  - Framework type mappings are accurate (e.g., NEF integer types → C# numeric types)
  - Syscalls map to correct Neo.SmartContract.Framework.Services methods
  - Native contract calls properly formatted (e.g., ContractManagement.Deploy)
  - Output documentation matches actual generated format
  - Integration test: decompile known contract, verify C# validity

### Task 6: Add Comprehensive Test Suite
- **ID**: task-6
- **Description**: Develop comprehensive unit tests for all modules (NEF parsing, IR transformations, opcode lifting, stack simulation, control flow analysis, C# generation). Create integration tests using real Neo N3 contract NEF files from mainnet/testnet. Add property-based tests for opcode combinations. Achieve ≥90% code coverage across the entire crate.
- **File Scope**:
  - `tests/**/*.rs` (all test files)
  - `tests/fixtures/` (new directory - sample NEF files from real contracts)
  - `tests/unit/` (new directory - module-specific unit tests)
  - `tests/integration/` (new directory - end-to-end decompilation tests)
  - `tests/property/` (new directory - property-based tests using proptest)
  - All `src/**/*.rs` files (add `#[cfg(test)]` modules for unit tests)
  - `.cargo/config.toml` (configure tarpaulin settings if needed)
- **Dependencies**: task-1, task-2, task-3, task-4, task-5
- **Test Command**: `cargo test --all-targets -- --nocapture && cargo tarpaulin --out Stdout --lib --bins --tests --exclude-files 'tests/*' --packages neo-decompiler --timeout 300 --fail-under 90`
- **Test Focus**:
  - Unit tests for each public function/method in every module
  - Integration tests for full NEF → decompiled output pipeline
  - Error path coverage (invalid NEF files, unsupported opcodes, edge cases)
  - Property-based tests: opcode sequences maintain stack invariants, IR transformations preserve semantics
  - Regression tests for previously fixed bugs
  - Code coverage report demonstrates ≥90% coverage (measured by tarpaulin)

### Task 7: Add Architecture Documentation
- **ID**: task-7
- **Description**: Create comprehensive architecture documentation explaining module structure, design decisions, IR system design, opcode dispatch architecture, control flow reconstruction algorithm, stack simulation approach, and rationale for refactoring choices. Include diagrams for module dependencies and data flow.
- **File Scope**:
  - `docs/architecture.md` (new file - main architecture document)
  - `docs/diagrams/` (new directory - architecture diagrams, optional)
  - `docs/diagrams/module-structure.svg` (module dependency diagram)
  - `docs/diagrams/ir-design.svg` (IR node hierarchy)
  - `docs/diagrams/decompilation-pipeline.svg` (data flow from NEF to output)
  - `README.md` (add link to architecture documentation)
  - `src/lib.rs` (add module-level documentation comments)
- **Dependencies**: task-3
- **Test Command**: `cargo doc --no-deps --document-private-items --open && markdown-link-check docs/architecture.md || echo "Manual review: verify all links and code examples"`
- **Test Focus**:
  - Documentation accurately reflects refactored codebase structure
  - Module responsibilities clearly explained with rationale
  - IR design documented: node types, transformations, rationale for typed approach
  - Control flow reconstruction algorithm described (dominance analysis, loop detection)
  - Stack simulation approach explained
  - Opcode dispatch system documented
  - Code examples in documentation are correct and compile
  - Diagrams accurately represent actual module structure

## Acceptance Criteria
- [ ] Project compiles without errors (`cargo build --release`)
- [ ] All tests pass (`cargo test --all-targets`)
- [ ] Code coverage ≥90% (`cargo tarpaulin --fail-under 90`)
- [ ] No SOLID violations (each component has single responsibility)
- [ ] No code duplication (DRY principle enforced, shared utilities extracted)
- [ ] All conditional jump opcodes lift to structured control flow (if/else/while)
- [ ] Function calls (CALL/CALL_L/CALLA/CALLT) properly reconstructed
- [ ] C# output is either compilable with Neo.SmartContract.Framework OR clearly documented as pseudocode
- [ ] Architecture documentation complete, accurate, and linked from README
- [ ] No `.unwrap()` calls on I/O operations (consistent `Result` propagation)
- [ ] All clippy warnings resolved (`cargo clippy -- -D warnings`)
- [ ] Integration tests pass with real Neo N3 contract samples

## Technical Notes
- **IR Design**: The typed IR should use Rust enums with pattern matching to represent expressions (Binary, Unary, Call, Literal, Variable) and statements (Assign, Return, If, While). This enables type-safe transformations and optimization passes.
- **Control Flow Reconstruction**: Consider using dominance analysis or the structured CFG algorithm (Schwartz et al.) to detect natural loops and nested conditionals. Build a proper CFG with basic blocks before lifting to structured statements.
- **C# Framework Mapping**: Neo.SmartContract.Framework namespace structure should be documented. If full compilable mapping is infeasible (due to semantic gaps), clearly document the pseudocode syntax and mark output with comments like `// PSEUDOCODE - NOT COMPILABLE`.
- **Test Strategy**: Use property-based testing (proptest crate) for opcode sequence combinations to verify stack invariants. Include real Neo N3 contracts from mainnet (e.g., NEO token, GAS token, common DeFi contracts) as integration test fixtures.
- **Coverage Tool**: Use `cargo tarpaulin` for coverage measurement. Exclude test files themselves (`--exclude-files 'tests/*'`) and aim for ≥90% coverage of library code. Use `--fail-under 90` to enforce the threshold.
- **Error Handling**: Replace all `.unwrap()` calls with proper `Result` propagation using `?` operator or explicit error messages via `.expect("context")`. Use `anyhow` or `thiserror` for structured error types.
- **Performance**: Consider lazy evaluation for large NEF files. The IR builder should stream opcodes rather than loading entire method bodies into memory for contracts with large methods.
- **Pluggable Architecture**: The refactored opcode dispatch system should use trait objects or enum dispatch to allow adding new opcode handlers without modifying core emitter logic.
