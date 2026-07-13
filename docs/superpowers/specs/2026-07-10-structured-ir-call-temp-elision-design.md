# Structured IR Call-Temp Elision Design

**Date:** 2026-07-10
**Status:** Approved continuation of the structured-IR syscall slice

## Goal

Remove presentation-only generated call temporaries from structured output when doing so cannot reorder evaluation or hide a side effect. A returned value call should render directly as `return call(...);`, while a dropped value call should remain visible as `call(...);` rather than an unused assignment.

## Scope

This slice changes only SSA-to-structured-IR statement emission in Rust. It does not rewrite SSA, change public IR types, change call contracts, or add general expression inlining.

The rewrite is intentionally restricted to assignments whose target has the generated base name `t` and whose value is `SsaExpr::Call`:

- `t_N = call(...); return t_N;` becomes `return call(...);` only when the statements are adjacent and the return is the temp's sole indexed use.
- An assigned call temp with no indexed or typed structural uses becomes an expression statement.
- Named slots, non-call expressions, multi-use temps, cross-block uses, and non-return consumers remain assignments.

## Alternatives Considered

### Local structurer rewrite (selected)

Walk each block's SSA statements by index while emitting structured IR. The structurer can validate adjacency against the typed statements and validate use status against both `SsaForm::uses` and the typed SSA graph. This keeps the optimization presentation-only and preserves the analyzed SSA graph.

### SSA optimizer substitution

Substituting calls through the optimizer could eliminate more temps, but it would require side-effect-aware ordering rules throughout expression rewriting. In particular, moving a Neo call into another call's arguments can change behavior because Neo arguments are evaluated right-to-left while C# evaluates them left-to-right. This is broader than the requested cleanup.

### Rendered-text cleanup

A post-render regex or line pass could recognize the visual pattern, but it would lose SSA variable identity and use-site information. It would also duplicate logic across output dialects. The typed structuring boundary is the last point where the safety proof is cheap and exact.

## Architecture

`StructCtx::emit_body` will walk ordinary block statements by index so it can inspect an adjacent assignment and return. Before lowering an SSA statement normally, it checks the direct-return pattern:

1. Require a generated `t` target, an `SsaExpr::Call` value, an exact `Return(Some(Variable(target)))` successor, and a use set equal to `{ UseSite::new(block, return_index) }`.
2. Emit one typed `Stmt::Return(Some(Expr::Call { ... }))` and consume both SSA statements.

`StructCtx` will precompute the variables referenced by every SSA statement expression and phi operand. `StructCtx::emit_ssa_stmt` will separately guard the existing assignment arm: an assigned generated call temp emits the lowered call as `Stmt::ExprStmt` only when it has neither an indexed use nor a typed structural reference. Keeping this rule in the common single-statement lowering automatically applies it to both ordinary and branch-condition blocks.

The existing branch-condition suppression still runs before `emit_ssa_stmt`. The condition-defining assignment is therefore never reclassified as a dropped call.

## Evaluation Order

No call is moved across another emitted statement. Direct-return elision combines two adjacent statements, and unused-call conversion keeps the call in its original statement position. A call is never substituted into call arguments, binary operands, control-flow conditions, named slots, or later blocks.

## Error Handling

Missing or inconsistent use-site data disables direct-return elision. Because `SsaForm`'s public construction API does not guarantee rebuilt indexes, an absent use entry is interpreted as unused only when the generated `t` call target is also absent from all typed statement and phi references. This keeps public hand-built and stale-index forms valid while matching the optimizer's retained side-effecting-call behavior. Every other shape falls back to the existing assignment emission.

## Tests

Structurer unit tests will prove:

1. an adjacent sole-use call temp becomes a direct return;
2. an unused call temp becomes an expression statement;
3. a multi-use call temp remains assigned;
4. an unused named-slot call remains assigned;
5. an unused non-call temp remains assigned;
6. a call temp consumed as another call's argument remains assigned.
7. a same-block return reference remains assigned when the use index is missing;
8. a cross-block return reference remains assigned when the use index is missing.

End-to-end syscall tests will require `System.Runtime.CheckWitness` to render directly in a return and will verify that dropping its result preserves both the visible syscall expression statement and an ambient return value.

## Non-Goals

- General single-use expression substitution.
- Cross-block call inlining.
- Inlining calls into arguments or control-flow expressions.
- Removing source-level named-slot assignments.
- Changing Neo VM argument order or C# evaluation semantics.
