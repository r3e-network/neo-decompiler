# Structured Collection Mutation Lifting Design

**Date:** 2026-07-11
**Status:** Approved continuation of the advanced-decompiler structured-IR work

## Goal

Preserve Neo VM collection and buffer mutations in the Rust structured SSA/IR path as readable, ordered effect statements instead of silently consuming their operands.

## Scope

This slice changes the Rust structured path and its shared semantic analyses for six fixed-arity, zero-result opcodes:

| Opcode | Structured helper | Arguments |
| --- | --- | --- |
| `SETITEM` | `set_item` | container, key, value |
| `APPEND` | `append` | container, item |
| `REMOVE` | `remove_item` | container, key |
| `CLEARITEMS` | `clear_items` | container |
| `REVERSEITEMS` | `reverse_items` | container |
| `MEMCPY` | `memcpy` | destination, destination index, source, source index, count |

The legacy Rust high-level renderer already uses these names and arities. Reusing them keeps both Rust paths aligned without introducing a second source-language vocabulary. JavaScript already preserves these operations in its high-level lifter, so no JavaScript implementation change belongs in this slice.

## Considered Approaches

### 1. Reuse `SsaStmt::Expr(SsaExpr::Call)` (selected)

Map each opcode to its established helper name while the SSA builder still has the ordered stack operands, then emit an ordinary effect statement. The optimizer, structurer, typed IR lowering, and renderer already preserve this representation. This is the smallest change and retains existing def/use behavior.

### 2. Add a mutation-specific SSA or typed-IR enum

A dedicated representation could distinguish mutations from arbitrary calls, but no current consumer needs that distinction. It would expand every optimizer, lowering, renderer, serializer, and pattern-match boundary for no source-level benefit. This is rejected as unnecessary surface area.

### 3. Reconstruct mutations in the renderer

Renderer-side recovery would act after SSA construction has already discarded the statements and their operand uses. It cannot reliably recover the original operation or keep constructor definitions alive through dead-code elimination. This is rejected because it is semantically too late.

## Architecture

`SsaBuilder::apply_instruction` remains responsible for fixed stack effects. It already pops fixed-arity operands and reverses the temporary pop list into deep-to-top order, which is the source order required by these collection operations. After branch/comparison handling and before ordinary result/store handling, the builder will recognize an effectful collection opcode and append:

```rust
SsaStmt::expr(SsaExpr::call(
    helper_name,
    popped.iter().cloned().map(SsaExpr::var).collect(),
))
```

The opcode-to-helper mapping will live in one focused builder helper returning `Option<&'static str>`. No renderer-specific string parsing or public IR variant is added.

`effects::stack_effect` remains the canonical arity table for SSA. `MEMCPY` will be corrected from three consumed values to five, matching Neo N3 semantics and the existing legacy renderer. The specialized entry-stack argument inference in `helpers/lifted.rs` must receive the same correction because it feeds shared method contracts but also tracks literal-dependent operations that the fixed SSA table cannot model directly.

The lightweight type analyzer will consume the same fixed operands for `MEMCPY` and `REVERSEITEMS`; its other four mutation branches are already correct. This prevents mutation inputs from leaking into later slot-type inference.

## Data Flow

```text
fixed-arity mutation instruction
    -> effects::stack_effect(opcode)
    -> pop operands and normalize to deep-to-top order
    -> effectful_collection_call(opcode, operands)
    -> SsaStmt::Expr(SsaExpr::Call)
    -> existing SSA substitution and use-index rebuild
    -> existing structurer and typed IR lowering
    -> set_item(...); / append(...); / ...

same opcode contract
    -> entry-stack argument inference
    -> shared MethodContract argument_count

same opcode contract
    -> lightweight type-analysis stack consumption
    -> no stale mutation operands at later slot stores
```

Because the effect statement references the constructor and operand definitions, existing optimizer use collection keeps those definitions live. Constants may still propagate into the call, which is desirable.

## Malformed Input

Stack underflow remains explicit and deterministic. The builder already substitutes the unknown SSA variable for every missing fixed-arity operand. Mutation calls therefore preserve their full declared arity and render `?` in the missing positions instead of disappearing or inventing values.

No new error type is needed. These opcodes have no bytecode operand to validate, and the existing decompiler policy is to produce conservative output for malformed scripts.

## Tests

Test-first coverage will establish six boundaries:

1. the fixed stack-effect table reports `MEMCPY` as `(5, 0)`;
2. a builder table test covers all six opcode/name/arity mappings and exact deep-to-top operand order, including five distinct `MEMCPY` literals;
3. an optimizer regression proves an effect statement retains the SSA definition it consumes and rebuilds its use index;
4. an end-to-end `SETITEM` script renders both `newmap()` and `set_item(...);` in execution order, with no dropped mutation.
5. a `MEMCPY`-only inferred helper requires five entry-stack arguments;
6. typed analysis consumes `MEMCPY` and `REVERSEITEMS` inputs instead of leaking their types into a following local store.

Focused SSA and integration tests run before the full Rust, JavaScript, web, lint, formatting, dependency-policy, and diff gates.

## Non-Goals

- Lifting `ASSERT`, `ASSERTMSG`, `THROW`, `ABORT`, or `ABORTMSG` as effect statements.
- Changing branch or terminator semantics.
- Adding source-level index assignment or collection-method syntax to structured IR.
- Removing source-visible `phi(...)` artifacts or performing SSA destruction.
- Migrating C# rendering to structured IR.
- Changing the JavaScript high-level collection lifter.
