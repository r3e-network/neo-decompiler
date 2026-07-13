# Structured IR Syscall Lifting Design

**Date:** 2026-07-10
**Status:** Approved continuation of the advanced-decompiler Phase 4 design

## Goal

Make the structured SSA/IR path preserve Neo syscalls as readable call expressions with exact known arity and return behavior, instead of consuming their stack effects and emitting `?` or no statement.

## Scope

This slice changes the Rust structured-IR path only. Rust and JavaScript already consume the same generated syscall catalog (`tools/data/syscalls.json`); the JavaScript high-level lifter already renders known and unknown syscalls correctly. The work therefore closes the Rust SSA/IR gap without adding another metadata source or changing generated files.

Source-level temporary inlining is a separate follow-up. This slice may initially render a value syscall as an SSA assignment followed by its use; preserving the call and its semantics comes before presentation-only substitution.

## Architecture

`SsaBuilder` remains the single structured lift for VM stack semantics. Its syscall-special path will resolve `SyscallInfo` from the existing generated table and construct an ordinary `SsaExpr::Call` named `syscall`. The syscall name is represented as the first string-literal argument, followed by VM arguments in declaration order. Existing SSA-to-IR lowering and IR rendering then handle the call without new public expression variants.

Known syscalls use the catalog contract:

- pop exactly `param_count` values top-first, which is Neo's declaration order;
- substitute the existing unknown SSA value for each missing argument;
- emit `SsaStmt::Expr` for void syscalls;
- emit an assignment and push its SSA result for value-returning syscalls;
- preserve unrelated values below the declared arguments.

Unknown hashes have no trustworthy arity. They will use the conservative opaque-call barrier: clear the pre-call stack, emit a visible `syscall("0xDEADBEEF")` result, and push only that result. This prevents unknown arguments from resurfacing after the call while keeping the operation visible. A missing operand follows the same fallback with `syscall("unknown")`.

## Data Flow

```text
Instruction::Syscall
    -> syscalls::lookup(hash)
    -> exact argument pops / opaque fallback
    -> SsaExpr::Call("syscall", [name_or_hash, args...])
    -> SsaStmt::Expr or SsaStmt::Assign
    -> existing SSA optimization
    -> existing typed IR lowering
    -> structured IR text
```

No manifest parsing, call-graph lookup, or renderer-side string rewrite is added. Syscall analysis metadata remains unchanged.

## Error Handling

Malformed known calls retain a `?` for every missing argument instead of dropping the syscall. Unknown syscall hashes and missing operands never guess parameter counts. Their opaque barrier is deliberately more conservative than the known-contract path.

## Tests

Builder-level tests will prove:

1. a known value syscall consumes arguments in declaration order and pushes only its result;
2. a known void syscall remains an expression statement and preserves an ambient stack value;
3. a known syscall underflow renders `?` arguments;
4. an unknown syscall clears pre-call values, remains visible, and produces one conservative result.

An end-to-end `ir_pipeline` test will assert readable structured output for both `System.Runtime.CheckWitness` and `System.Runtime.Log`. The full Rust, JavaScript, web, lint, formatting, dependency-policy, and diff gates remain the completion fence.

## Non-Goals

- Adding parameter or return source types that are absent from the generated syscall catalog.
- Introducing public `Syscall` variants in `SsaExpr` or `ir::Expr`.
- Replacing the legacy high-level syscall emitter in this slice.
- General single-use temporary inlining or C# dialect-specific syscall rewrites.
