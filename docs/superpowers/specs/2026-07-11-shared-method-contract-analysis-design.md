# Shared Method Contract Analysis Design

**Date:** 2026-07-11
**Status:** Approved continuation of the advanced-decompiler Phase 4 design

## Goal

Compute each method's stack-call contract once and use it in every user-facing renderer. Private helpers proven to return no value must render as statements in Rust high-level output, Rust C#, structured IR, and JavaScript, preserving unrelated caller stack values.

The analysis must also be available to API consumers instead of remaining a structured-renderer implementation detail.

## Problem

The current worktree infers private return behavior only inside `cfg::method_view::render_envelope`. Normal Rust high-level and C# renderers receive manifest-declared return metadata only, while JavaScript does the same in `buildHighLevelContext`.

For a caller that holds `9`, passes `1` to a private helper, and returns after the helper consumes its argument without producing a value, structured IR correctly emits:

```text
sub_0x0007(1);
return 9;
```

Rust high-level, Rust C#, and JavaScript currently treat the helper as value-producing and emit an equivalent of `return sub_0x0007(1)`. This is a semantic error, not cosmetic output drift.

## Return Model

Return behavior is tri-state:

- `value`: declared by a manifest as non-void;
- `void`: declared void or proven void by fixed-point inference;
- `unknown`: no trustworthy declaration and not proven void.

Renderers treat `unknown` conservatively as value-producing. This preserves current opaque-call behavior without publishing a guessed `value` classification as fact.

Each method contract contains:

```text
method: MethodRef { offset, name }
argument_count: usize
return_behavior: value | void | unknown
```

Contracts are sorted by method offset for deterministic output.

## Alternatives Considered

### Shared analysis result (selected)

Create one method-contract analysis, compute it after the call graph, store it on the decompilation result, and pass it to all renderers. Rust and JavaScript expose equivalent result shapes in their native naming conventions. This establishes one semantic source of truth per implementation and makes the inference inspectable by API clients.

### Infer independently inside each renderer

This minimizes signature changes but duplicates a fixed-point analysis across structured IR, high-level, and C#. The current bug exists because those copies already disagree, so renderer-local inference is rejected.

### Switch C# and high-level output directly to structured IR

That is the long-term architecture, but the structured path still drops collection mutations and renders source-facing `phi(...)` calls. Promoting it now would replace one semantic bug with broader regressions. Shared contracts are a prerequisite that improves both paths immediately.

## Rust Architecture

Add `decompiler::analysis::method_contracts` with serializable public types:

- `ReturnBehavior`;
- `MethodContract`;
- `MethodContracts`;
- `infer_method_contracts(instructions, manifest, call_graph)`.

The analysis reuses existing method discovery, inferred entry arity, per-method CFG extraction, `SsaBuilder`, and `CallContract`. Manifest methods seed `value` or `void`. Other methods begin as `unknown`; calls to unknown methods remain conservatively value-producing. A fixed-point loop marks a method `void` only when every observed `RET` lacks a value under the current callee contracts. Wrapper chains therefore converge from leaf to caller.

`Decompiler::decompile_bytes_with_manifest` computes contracts immediately after the call graph and stores them on `Decompilation`. High-level, C#, and structured rendering receive the same result. Inferred Rust high-level definitions use `-> any` for `unknown` and omit a return type for `void`; inferred C# definitions use `dynamic` and `void`, respectively.

CLI and web analysis reports include `method_contracts`, and `docs/schema/decompile.schema.json` defines the serialized shape.

## JavaScript Architecture

Add a focused `method-contracts.js` module. It seeds contracts from manifest methods and inferred argument counts, then runs a fixed-point pass over private method groups using `liftMethodBody` with the evolving call-return map. A method becomes `void` only when it has at least one rendered return and every rendered return is bare. Any mixed, missing, or value return remains `unknown`.

`buildHighLevelContext` owns the resulting map and `createState` consults it when deciding whether `RET` consumes a value. Internal calls already consult `methodReturnsValueByOffset`; that map will now come from the shared contract result rather than manifest metadata alone.

High-level and analysis APIs expose `methodContracts`, and `index.d.ts` describes the new types. This slice does not add JavaScript C# generation.

## Data Flow

```text
instructions + manifest + call graph
    -> method discovery + argument counts
    -> declared return seeds
    -> fixed-point private return inference
    -> MethodContracts
       -> Rust structured IR
       -> Rust high-level
       -> Rust C#
       -> Rust CLI/web analysis JSON
       -> JavaScript high-level/analysis result
```

## Safety And Fallbacks

- Manifest return declarations always win over inference.
- Ambiguous or mixed-return private methods remain `unknown`.
- Recursive cycles with no provable void leaf remain `unknown`.
- Unknown contracts remain value-producing for stack simulation.
- Method-token contracts keep their NEF-declared arity and return flag.
- Unresolved and indirect calls retain the existing opaque-call barrier.
- No renderer infers a type or return value merely from a method name.

## Tests

Rust tests will prove:

1. the private leaf helper is classified `void` with one argument;
2. a private wrapper chain converges to `void`;
3. recursive or mixed ambiguous methods remain `unknown`;
4. manifest declarations override inference;
5. high-level and C# preserve the caller's ambient `9` around a private void call;
6. C# emits a `private static void` helper definition;
7. structured IR still uses the same contract result;
8. CLI and web analysis JSON expose the deterministic contract list.

JavaScript tests will use the same private-helper and wrapper fixtures, assert the high-level output, inspect `methodContracts`, and update TypeScript API coverage.

The full Rust, JavaScript, web, lint, formatting, dependency-policy, schema, and diff gates remain the completion fence.

## Non-Goals

- Replacing the C# renderer with structured IR in this slice.
- SSA destruction or removing `phi(...)` from structured output.
- Lifting collection mutations; that is the next structured-IR semantic gap.
- Inferring source-level return types beyond `void` versus value/unknown.
- Treating unresolved or indirect calls as void without metadata.
