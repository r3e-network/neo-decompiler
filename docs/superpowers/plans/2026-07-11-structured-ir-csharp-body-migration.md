# Structured IR C# Body Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every instruction-bearing Rust C# method body render from fidelity-checked, optimized structured IR, then remove the legacy text-lifting and line-reparsing path after compile and corpus gates prove the cutover complete.

**Architecture:** Keep the existing C# envelope, emitted signatures, events, overload behavior, synthetic entries, and exact `inferred_method_starts` slices. Add a renderer-neutral exact-slice method lowerer with typed call provenance, neutral symbol metadata, instruction-level fidelity, and source origins; feed exact/conservative results into a C# declaration planner and direct typed visitor, while incomplete methods temporarily use the whole-method legacy backend. Close semantic gaps in the shared lowering, add trace provenance and compile/coverage gates, then delete fallback-only code.

**Tech Stack:** Rust 2021, existing CFG/SSA/structured IR, Neo N3 opcodes, existing `tempfile` dev dependency, `dotnet`/Roslyn against an explicitly supplied Neo.SmartContract.Framework assembly. No production dependency additions.

---

## File Structure

- Create `src/decompiler/cfg/method_body.rs`: exact-slice CFG normalization, `StructuredMethodBody`, neutral symbols, fidelity reports, opcode classifications, and source maps.
- Modify `src/decompiler/cfg/method_view.rs`: consume `lower_method_body` for the existing structured-IR envelope; remove its private duplicate lowering once callers migrate.
- Modify `src/decompiler/cfg/ssa/context.rs`: carry semantic call identity and neutral method metadata.
- Modify `src/decompiler/cfg/ssa/form.rs`: preserve typed calls and semantic statements through SSA.
- Modify `src/decompiler/cfg/ssa/builder.rs`: return instruction coverage/issues and lower failure, collection, conversion, catch, and call semantics without loss.
- Modify `src/decompiler/cfg/ssa/effects.rs`: correct stack effects for type operations and collection packing.
- Modify `src/decompiler/cfg/ssa/optimize.rs`: preserve typed call identity, effect ordering, and statement origins.
- Modify `src/decompiler/cfg/ssa/to_ir.rs`: lower typed SSA calls and semantic statements to typed IR.
- Modify `src/decompiler/cfg/structure.rs`: retain semantic failures/transfers, emit irreducible labels/gotos, and attach statement origins.
- Modify `src/decompiler/cfg/phi_lowering.rs`: accept de-versioned C# source-slot names without exposing identity copies.
- Create `src/decompiler/ir/semantic.rs`: language-neutral call provenance and VM intrinsic identity shared by SSA and IR.
- Modify `src/decompiler/ir/expression/expr.rs`: replace name-only calls with `SemanticCallTarget`.
- Modify `src/decompiler/ir/statement.rs`: add throw, abort, assert, break, continue, label, and goto statements.
- Modify `src/decompiler/ir/render/*`: keep the generic structured-IR output exhaustive after shared IR additions.
- Create `src/decompiler/csharp/render/structured/mod.rs`: typed-body eligibility, plan, and render entry point.
- Create `src/decompiler/csharp/render/structured/plan.rs`: C# symbol names, lexical scopes, declarations, use counts, return policy, and static-field planning.
- Create `src/decompiler/csharp/render/structured/expr.rs`: direct typed C# expressions, literals, calls, types, and collections.
- Create `src/decompiler/csharp/render/structured/stmt.rs`: direct blocks, control flow, switch termination, catch/failure, labels, and trace comments.
- Create `src/decompiler/csharp/render/structured/tests.rs`: direct planner/visitor unit tests.
- Modify `src/decompiler/csharp/render/body.rs`: select typed rendering or temporary whole-method fallback before writing body text.
- Modify `src/decompiler/csharp/render/methods.rs`: build one canonical `CSharpMethodPlan` per emitted declaration while retaining every existing slicing branch.
- Modify `src/decompiler/csharp/render.rs`: build shared method/static plans and emit class-level static fields.
- Modify `src/decompiler/csharp/helpers.rs`: retain string conversion only while fallback exists, then delete fallback-only helpers.
- Create `src/decompiler/helpers/vm_values.rs`: shared type-tag, printable-byte, and signed little-endian decoding moved out of the legacy emitter.
- Modify `src/decompiler/tests/csharp.rs`: lock envelope behavior, vertical typed output, deterministic fallback warnings, and final forbidden-source behavior.
- Create `src/decompiler/tests/csharp_coverage.rs`: crate-internal artifact/corpus backend and forbidden-source gates.
- Modify `tests/ir_pipeline.rs`: lock shared semantic lowering and exact/conservative/incomplete classification.
- Create `tests/csharp_compile.rs`: opt-in Roslyn harness using `std::process::Command` and `tempfile`.
- Create `tools/ci/csharp_compile.sh`: required CI wrapper for the framework assembly and compile test.
- Modify `tools/ci/artifact_sweep.sh`: run zero-fallback and forbidden-source gates before accepting regenerated artifacts.

**Dirty-worktree constraint:** This checkout already contains overlapping prerequisite changes. Before every commit, inspect `git status --short` and the complete diff for each listed path. The `git add` commands below describe the intended task-owned paths; they do not authorize staging unrelated pre-existing hunks. Never reset or revert those hunks. If ownership cannot be proven for an overlapping file, leave that file unstaged and record the pending commit boundary rather than sweeping it into the task commit.

### Task 1: Replace Name-Only Calls With Semantic Targets

**Files:**
- Create: `src/decompiler/ir/semantic.rs`
- Modify: `src/decompiler/ir/mod.rs`
- Modify: `src/decompiler/ir/expression/expr.rs`
- Modify: `src/decompiler/cfg/ssa/context.rs`
- Modify: `src/decompiler/cfg/ssa/form.rs`
- Modify: `src/decompiler/cfg/ssa/builder.rs`
- Modify: `src/decompiler/cfg/ssa/optimize.rs`
- Modify: `src/decompiler/cfg/ssa/to_ir.rs`
- Modify: `src/decompiler/cfg/structure.rs`
- Modify: `src/decompiler/ir/render/expr.rs`
- Test: `tests/ir_pipeline.rs`

- [ ] **Step 1: Add failing collision and provenance tests**

Add builder and pipeline tests that use a user method named `append`, a real VM `APPEND`, a known syscall, and a method token. Match semantic targets rather than debug text:

```rust
assert!(matches!(
    internal_call,
    SsaExpr::Call {
        target: SemanticCallTarget::Internal { offset: 12, ref name },
        ..
    } if name == "append"
));
assert!(matches!(
    mutation,
    SsaStmt::Expr(SsaExpr::Call {
        target: SemanticCallTarget::Intrinsic(Intrinsic::Opcode(OpCode::Append)),
        ..
    })
));
assert!(matches!(
    syscall,
    SsaExpr::Call {
        target: SemanticCallTarget::Syscall { hash: 0x627D_5B52, .. },
        ..
    }
));
```

- [ ] **Step 2: Run the tests and verify name-only calls fail the contract**

Run:

```bash
cargo test --lib decompiler::cfg::ssa::builder::tests::preserves_semantic_call_targets -- --exact --nocapture
cargo test --test ir_pipeline structured_ir_distinguishes_user_append_from_vm_append -- --exact --nocapture
```

Expected: compile failure because `SemanticCallTarget`, `Intrinsic`, and the `target` field do not exist.

- [ ] **Step 3: Add the shared call identity types and constructors**

Define the types in `ir/semantic.rs` and re-export them from `ir/mod.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Intrinsic {
    Opcode(OpCode),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticCallTarget {
    Internal { offset: usize, name: String },
    MethodToken { index: usize, name: String },
    Syscall { hash: u32, name: Option<String> },
    Intrinsic(Intrinsic),
    Unresolved { display_name: String },
}

impl SemanticCallTarget {
    pub fn display_name(&self) -> &str {
        match self {
            Self::Internal { name, .. } | Self::MethodToken { name, .. } => name,
            Self::Syscall { name: Some(name), .. } => name,
            Self::Syscall { name: None, .. } => "syscall",
            Self::Intrinsic(Intrinsic::Opcode(opcode)) => opcode.mnemonic(),
            Self::Unresolved { display_name } => display_name,
        }
    }
}
```

Change both expression enums to `Call { target: SemanticCallTarget, args: Vec<_> }`. Provide `call(target, args)` and `unresolved_call(name, args)` constructors so analysis tests can remain concise without discarding provenance in production lowering.

- [ ] **Step 4: Carry identity through call contracts and all expression walkers**

Change `CallContract` to:

```rust
pub(crate) struct CallContract {
    pub(crate) target: SemanticCallTarget,
    pub(crate) argument_count: usize,
    pub(crate) returns_value: bool,
}
```

Construct internal targets from resolved offsets, method-token targets from token indexes, syscall targets from operand hashes, and VM helper/effect calls from `Intrinsic::Opcode(instr.opcode)`. Update optimizer traversal, display rendering, use collection, DCE effect checks, `ssa_expr_to_ir_with_source_names`, and structured temporary handling to copy `target` unchanged.

- [ ] **Step 5: Run provenance and existing call suites**

Run:

```bash
cargo test --lib decompiler::cfg::ssa::builder::tests -- --nocapture
cargo test --lib decompiler::cfg::ssa::optimize::tests -- --nocapture
cargo test --test ir_pipeline -- --nocapture
```

Expected: all tests pass; internal, token, syscall, and intrinsic calls remain distinguishable after optimization and structuring.

- [ ] **Step 6: Commit with Lore context**

```bash
git add src/decompiler/ir src/decompiler/cfg/ssa src/decompiler/cfg/structure.rs tests/ir_pipeline.rs
git commit -m "Preserve call identity across structured lowering

String call names could collide with VM intrinsics and make a typed C# visitor misrender user methods, so call kind and stable identity now survive SSA and IR lowering.

Constraint: Existing generic structured output still needs a display name
Rejected: Match append/syscall/assert by rendered name | user methods can legally use those names
Confidence: high
Scope-risk: moderate
Tested: SSA builder, optimizer, and structured IR call suites"
```

### Task 2: Record Instruction-Level Fidelity During SSA Construction

**Files:**
- Create: `src/decompiler/cfg/method_body.rs`
- Modify: `src/decompiler/cfg/mod.rs`
- Modify: `src/decompiler/cfg/ssa/builder.rs`
- Modify: `src/decompiler/cfg/ssa/mod.rs`
- Test: `src/decompiler/cfg/method_body.rs`
- Test: `tests/ir_pipeline.rs`

- [ ] **Step 1: Add failing ASSERT, PACK, unresolved-call, unknown-value, and budget tests**

Require deterministic offset/opcode diagnostics:

```rust
assert_eq!(report.status, Fidelity::Incomplete);
assert_eq!(report.instruction_count, instructions.len());
assert_eq!(report.covered_offsets, BTreeSet::from([0, 1, 2, 3]));
assert!(report.issues.iter().any(|issue| {
    issue.offset == 1
        && issue.opcode == OpCode::Assert
        && issue.kind == LoweringIssueKind::UnsupportedOpcode
}));
```

Add a clean `PUSH1; RET` test expecting `Exact`, no issues, and complete coverage. Add a size-guard test expecting `BudgetExceeded` at the first offset.

- [ ] **Step 2: Run focused tests and verify the report API is absent**

Run:

```bash
cargo test --lib decompiler::cfg::method_body::tests -- --nocapture
cargo test --test ir_pipeline structured_ir_reports_assert_semantic_loss -- --exact --nocapture
```

Expected: compile failure because the fidelity types and `build_with_report` do not exist.

- [ ] **Step 3: Define fidelity, diagnostics, and the SSA build report**

Add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Fidelity { Exact, Conservative, Incomplete }

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum LoweringIssueKind {
    UnsupportedControl,
    UnsupportedOpcode,
    LostStackValue,
    MissingOperandMetadata,
    UnresolvedCall,
    MissingProvenance,
    BudgetExceeded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoweringIssue {
    pub(crate) offset: usize,
    pub(crate) opcode: OpCode,
    pub(crate) kind: LoweringIssueKind,
    pub(crate) fidelity: Fidelity,
    pub(crate) detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FidelityReport {
    pub(crate) status: Fidelity,
    pub(crate) issues: Vec<LoweringIssue>,
    pub(crate) covered_offsets: BTreeSet<usize>,
    pub(crate) instruction_count: usize,
}

pub(crate) struct SsaBuildOutput {
    pub(crate) ssa: SsaForm,
    pub(crate) fidelity: FidelityReport,
}
```

Keep `SsaBuilder::build()` as `self.build_with_report().ssa` for existing analysis callers.

- [ ] **Step 4: Aggregate diagnostics only from the final deterministic pass**

Extend `BlockExecution` with `covered_offsets` and `issues`. In `execute_block`, insert every visited instruction offset, emit an issue at the exact instruction when required operands/metadata are unavailable or semantics are discarded, and return diagnostics only from the final post-fixpoint execution. Sort and deduplicate by `(offset, opcode, kind, detail)`, and derive method fidelity as the maximum issue fidelity.

Use `Conservative` only for semantics-preserving low-level renderings such as a known syscall wrapper; use `Incomplete` for lost values/effects, unresolved call identity, unknowns reaching output, or unsupported control.

- [ ] **Step 5: Add exhaustive known-opcode classification**

Define `OpcodeFidelity::{Exact, Conservative, Incomplete(LoweringIssueKind)}` and enumerate every generated known variant. The only data-carrying catch-all is `Unknown`:

```rust
pub(crate) fn classify_opcode(opcode: OpCode) -> OpcodeFidelity {
    match opcode {
        OpCode::Unknown(_) => OpcodeFidelity::Incomplete(LoweringIssueKind::UnsupportedOpcode),
        OpCode::Abort | OpCode::Abortmsg | OpCode::Assert | OpCode::Assertmsg
        | OpCode::Throw | OpCode::Pack | OpCode::Packmap | OpCode::Packstruct
        | OpCode::Unpack | OpCode::NewarrayT | OpCode::Istype | OpCode::Convert
        | OpCode::Pushint128 | OpCode::Pushint256 => {
            OpcodeFidelity::Incomplete(LoweringIssueKind::UnsupportedOpcode)
        }
        OpCode::CallA | OpCode::Xdrop | OpCode::Pick | OpCode::Roll
        | OpCode::Reversen | OpCode::Try | OpCode::TryL | OpCode::Endtry
        | OpCode::EndtryL | OpCode::Endfinally => {
            OpcodeFidelity::Incomplete(LoweringIssueKind::MissingProvenance)
        }
        OpCode::Syscall => OpcodeFidelity::Conservative,
        OpCode::Pushint8 | OpCode::Pushint16 | OpCode::Pushint32 | OpCode::Pushint64
        | OpCode::PushT | OpCode::PushF | OpCode::PushA | OpCode::PushNull
        | OpCode::Pushdata1 | OpCode::Pushdata2 | OpCode::Pushdata4 | OpCode::PushM1
        | OpCode::Push0 | OpCode::Push1 | OpCode::Push2 | OpCode::Push3
        | OpCode::Push4 | OpCode::Push5 | OpCode::Push6 | OpCode::Push7
        | OpCode::Push8 | OpCode::Push9 | OpCode::Push10 | OpCode::Push11
        | OpCode::Push12 | OpCode::Push13 | OpCode::Push14 | OpCode::Push15
        | OpCode::Push16 | OpCode::Nop | OpCode::Jmp | OpCode::Jmp_L
        | OpCode::Jmpif | OpCode::Jmpif_L | OpCode::Jmpifnot | OpCode::Jmpifnot_L
        | OpCode::JmpEq | OpCode::JmpEq_L | OpCode::JmpNe | OpCode::JmpNe_L
        | OpCode::JmpGt | OpCode::JmpGt_L | OpCode::JmpGe | OpCode::JmpGe_L
        | OpCode::JmpLt | OpCode::JmpLt_L | OpCode::JmpLe | OpCode::JmpLe_L
        | OpCode::Call | OpCode::Call_L | OpCode::CallT | OpCode::Ret
        | OpCode::Depth | OpCode::Drop | OpCode::Nip | OpCode::Clear | OpCode::Dup
        | OpCode::Over | OpCode::Tuck | OpCode::Swap | OpCode::Rot
        | OpCode::Reverse3 | OpCode::Reverse4 | OpCode::Initsslot | OpCode::Initslot
        | OpCode::Ldsfld0 | OpCode::Ldsfld1 | OpCode::Ldsfld2 | OpCode::Ldsfld3
        | OpCode::Ldsfld4 | OpCode::Ldsfld5 | OpCode::Ldsfld6 | OpCode::Ldsfld
        | OpCode::Stsfld0 | OpCode::Stsfld1 | OpCode::Stsfld2 | OpCode::Stsfld3
        | OpCode::Stsfld4 | OpCode::Stsfld5 | OpCode::Stsfld6 | OpCode::Stsfld
        | OpCode::Ldloc0 | OpCode::Ldloc1 | OpCode::Ldloc2 | OpCode::Ldloc3
        | OpCode::Ldloc4 | OpCode::Ldloc5 | OpCode::Ldloc6 | OpCode::Ldloc
        | OpCode::Stloc0 | OpCode::Stloc1 | OpCode::Stloc2 | OpCode::Stloc3
        | OpCode::Stloc4 | OpCode::Stloc5 | OpCode::Stloc6 | OpCode::Stloc
        | OpCode::Ldarg0 | OpCode::Ldarg1 | OpCode::Ldarg2 | OpCode::Ldarg3
        | OpCode::Ldarg4 | OpCode::Ldarg5 | OpCode::Ldarg6 | OpCode::Ldarg
        | OpCode::Starg0 | OpCode::Starg1 | OpCode::Starg2 | OpCode::Starg3
        | OpCode::Starg4 | OpCode::Starg5 | OpCode::Starg6 | OpCode::Starg
        | OpCode::Newbuffer | OpCode::Memcpy | OpCode::Cat | OpCode::Substr
        | OpCode::Left | OpCode::Right | OpCode::Invert | OpCode::And | OpCode::Or
        | OpCode::Xor | OpCode::Equal | OpCode::Notequal | OpCode::Sign
        | OpCode::Abs | OpCode::Negate | OpCode::Inc | OpCode::Dec | OpCode::Add
        | OpCode::Sub | OpCode::Mul | OpCode::Div | OpCode::Mod | OpCode::Pow
        | OpCode::Sqrt | OpCode::Modmul | OpCode::Modpow | OpCode::Shl
        | OpCode::Shr | OpCode::Not | OpCode::Booland | OpCode::Boolor
        | OpCode::Nz | OpCode::Numequal | OpCode::Numnotequal | OpCode::Lt
        | OpCode::Le | OpCode::Gt | OpCode::Ge | OpCode::Min | OpCode::Max
        | OpCode::Within | OpCode::Newarray0 | OpCode::Newarray
        | OpCode::Newstruct0 | OpCode::Newstruct | OpCode::Newmap | OpCode::Size
        | OpCode::Haskey | OpCode::Keys | OpCode::Values | OpCode::Pickitem
        | OpCode::Append | OpCode::Setitem | OpCode::Reverseitems | OpCode::Remove
        | OpCode::Clearitems | OpCode::Popitem | OpCode::Isnull => OpcodeFidelity::Exact,
    }
}
```

Add `all_known_opcodes_have_an_explicit_classification` over `OpCode::all_known()` and assert no known opcode takes an unknown/default path. This table is the initial eligibility ceiling: the final instruction result can still downgrade for missing operands, unresolved targets, stack loss, or control shape. Later semantic tasks promote their named incomplete groups and update the table/test in the same commit. Do not use `_` for known variants.

- [ ] **Step 6: Run fidelity and regression suites**

Run:

```bash
cargo test --lib decompiler::cfg::method_body::tests -- --nocapture
cargo test --lib decompiler::cfg::ssa::builder::tests -- --nocapture
cargo test --test ir_pipeline -- --nocapture
```

Expected: clean methods are exact, current ASSERT/PACK gaps are incomplete with exact offsets, and every known opcode has a tested classification.

- [ ] **Step 7: Commit the completeness boundary**

```bash
git add src/decompiler/cfg tests/ir_pipeline.rs
git commit -m "Make structured lowering prove semantic completeness

Final IR cannot reveal effects that were already discarded, so SSA construction now records instruction coverage and typed fidelity issues at the point of loss.

Constraint: Fidelity must be deterministic across SSA fixpoint passes
Rejected: Scan the final IR for question marks | ASSERT and PACK can disappear before structuring
Confidence: high
Scope-risk: moderate
Directive: New opcode handling must update the exhaustive classification and its tests
Tested: fidelity units, SSA builder suite, structured IR integrations"
```

### Task 3: Extract Exact-Slice Structured Method Lowering And Neutral Symbols

**Files:**
- Modify: `src/decompiler/cfg/method_body.rs`
- Modify: `src/decompiler/cfg/method_view.rs`
- Modify: `src/decompiler/cfg/mod.rs`
- Modify: `src/decompiler/analysis/types.rs`
- Modify: `src/decompiler/cfg/phi_lowering.rs`
- Test: `src/decompiler/cfg/method_body.rs`
- Test: `tests/ir_pipeline.rs`

- [ ] **Step 1: Add failing exact-slice and source-slot tests**

Cover a cross-range jump, manifest argument names, local/static versions, loop phi copies, and an unknown source symbol. Assert:

```rust
assert_eq!(body.symbols["amount"].origin, SymbolOrigin::Parameter(0));
assert_eq!(body.symbols["loc0"].origin, SymbolOrigin::Local(0));
assert_eq!(body.symbols["static1"].origin, SymbolOrigin::Static(1));
assert!(!render_block(&body.body, 0).contains("loc0_"));
assert!(!render_block(&body.body, 0).contains("static1_"));
assert_eq!(body.return_behavior, ReturnBehavior::Value);
```

- [ ] **Step 2: Run tests and verify the method-body API is incomplete**

Run:

```bash
cargo test --lib decompiler::cfg::method_body::tests -- --nocapture
cargo test --test ir_pipeline structured_ir_deversions_source_slots_before_phi_lowering -- --exact --nocapture
```

Expected: compile failure because request/body/symbol types and `lower_method_body` are absent.

- [ ] **Step 3: Add the renderer-neutral request/result types**

```rust
pub(crate) struct MethodIrRequest<'a> {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) instructions: &'a [Instruction],
    pub(crate) context: MethodContext,
    pub(crate) symbol_types: MethodSymbolTypes,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct MethodSymbolTypes {
    pub(crate) parameters: Vec<ValueType>,
    pub(crate) locals: Vec<ValueType>,
    pub(crate) statics: Vec<ValueType>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SymbolOrigin {
    Parameter(usize), Local(usize), Static(usize), Temporary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SymbolInfo {
    pub(crate) origin: SymbolOrigin,
    pub(crate) value_type: ValueType,
}

pub(crate) struct StructuredMethodBody {
    pub(crate) body: Block,
    pub(crate) symbols: BTreeMap<String, SymbolInfo>,
    pub(crate) return_behavior: ReturnBehavior,
    pub(crate) fidelity: FidelityReport,
    pub(crate) source_map: SourceMap,
}
```

- [ ] **Step 4: Move exact-slice CFG normalization and implement the lowering pipeline**

Move `build_method_cfg` and `control_transfer_leaves_method` from `method_view.rs` without semantic changes. `lower_method_body` must filter the request's exact `[start, end)` slice and then perform:

```rust
let cfg = build_method_cfg(&slice, request.start, request.end);
let built = SsaBuilder::new(&cfg, &slice)
    .with_method_context(&request.context)
    .build_with_report();
let mut ssa = built.ssa;
optimize_ssa(&mut ssa);
let (source_names, symbols) = allocate_source_symbols(&request, &ssa);
let body = structure_cfg_with_source_names(&ssa, &source_names);
let fidelity = validate_renderable(body, symbols, built.fidelity);
```

Return `BudgetExceeded` before CFG construction when the slice exceeds `MAX_HIGH_LEVEL_METHOD_INSTRUCTIONS`.

- [ ] **Step 5: De-version source families before phi copy scheduling**

Map every SSA variable whose base parses as `argN`, `locN`, or `staticN` to its exact emitted source name before calling `structure_cfg_with_source_names`. Parameters use `MethodContext.argument_names`; locals/statics use stable `locN`/`staticN`; temporary names remain versioned and collision-safe. Unknown variables that survive to output append `LostStackValue` and make fidelity incomplete.

- [ ] **Step 6: Switch `method_view` to the shared lowerer**

Keep `extract_method_cfgs` and the structured envelope behavior, but replace its inline SSA/optimize/name/structure sequence with `lower_method_body`. Preserve current manifest association and return rendering tests.

- [ ] **Step 7: Run method, phi, and structured integration tests**

Run:

```bash
cargo test --lib decompiler::cfg::method_body::tests -- --nocapture
cargo test --lib decompiler::cfg::phi_lowering::tests -- --nocapture
cargo test --lib decompiler::cfg::method_view::tests -- --nocapture
cargo test --test ir_pipeline -- --nocapture
```

Expected: exact C#-selected slices can use the same lowerer, source slot families are mutable names without SSA suffixes, and phi semantics remain intact.

- [ ] **Step 8: Commit the shared method-body contract**

```bash
git add src/decompiler/cfg src/decompiler/analysis/types.rs tests/ir_pipeline.rs
git commit -m "Share exact-slice structured method lowering

C# needs its mature method partitions while consuming the same optimized IR, so exact-slice CFG normalization and neutral symbol metadata now live behind one renderer-neutral method-body API.

Constraint: C# detached chunks differ from MethodTable spans
Rejected: Iterate MethodTable from the C# renderer | it omits presentation-only detached method starts
Confidence: high
Scope-risk: moderate
Directive: C# remains the owner of start/end selection
Tested: method-body, method-view, phi-lowering, and IR integration suites"
```

### Task 4: Make C# Method Names And Call Sites Share One Plan

**Files:**
- Create: `src/decompiler/csharp/render/structured/mod.rs`
- Create: `src/decompiler/csharp/render/structured/plan.rs`
- Modify: `src/decompiler/csharp/render.rs`
- Modify: `src/decompiler/csharp/render/methods.rs`
- Modify: `src/decompiler/csharp/render/body.rs`
- Test: `src/decompiler/csharp/render/structured/tests.rs`
- Test: `src/decompiler/tests/csharp.rs`

- [ ] **Step 1: Add failing overload/collision tests**

Cover two valid overloads, two duplicate identical signatures, a call to the suffixed duplicate, and an ambiguous multi-declaration offset:

```rust
assert_eq!(plans[0].emitted_name, "transfer");
assert_eq!(plans[1].emitted_name, "transfer");
assert_eq!(plans[2].emitted_name, "transfer_2");
assert_eq!(
    plans[2].method_context.calls_by_offset[&42].target.display_name(),
    "transfer_2"
);
assert!(ambiguous.planning_issues.iter().any(|issue| {
    issue.kind == LoweringIssueKind::UnresolvedCall
}));
```

- [ ] **Step 2: Run tests and verify declarations/calls use separate naming paths**

Run:

```bash
cargo test --lib decompiler::csharp::render::structured::tests::plans_overloads_and_calls_together -- --exact --nocapture
cargo test --lib decompiler::tests::csharp::csharp_internal_call_uses_duplicate_signature_suffix -- --exact --nocapture
```

Expected: failures because declarations still call `make_unique_method_name` while call labels come from `build_method_labels_by_offset`.

- [ ] **Step 3: Define `CSharpMethodPlan` and precompute plans**

```rust
pub(super) struct CSharpMethodPlan {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) raw_name: String,
    pub(super) emitted_name: String,
    pub(super) parameters: Vec<CSharpParameter>,
    pub(super) return_type: String,
    pub(super) return_behavior: ReturnBehavior,
    pub(super) method_context: MethodContext,
    pub(super) symbol_types: MethodSymbolTypes,
    pub(super) planning_issues: Vec<LoweringIssue>,
}
```

Build all manifest, offset-less entry, inferred, and synthetic method plans before writing the first declaration. Reuse the existing `(sanitized name, parameter-type signature)` uniqueness algorithm. Build internal `CallContract` targets from the final offset-to-emitted-name map; ambiguous offsets produce `Unresolved` plus an incomplete diagnostic.

- [ ] **Step 4: Preserve every existing slicing branch while passing plans to bodies**

Keep the five current body call sites and their exact slice construction, but replace separate labels/return booleans with `&CSharpMethodPlan`. Do not alter attributes, visibility, event order, offset-less stubs, or synthetic entry selection.

- [ ] **Step 5: Run envelope and naming regressions**

Run:

```bash
cargo test --lib decompiler::csharp::render::structured::tests -- --nocapture
cargo test --lib decompiler::tests::csharp -- --nocapture
```

Expected: all current envelope tests pass and definitions/calls agree for overloads and suffixes.

- [ ] **Step 6: Commit canonical C# method planning**

```bash
git add src/decompiler/csharp/render src/decompiler/tests/csharp.rs
git commit -m "Unify emitted C# method and call names

Overload-aware declarations and global call labels used different collision rules, so one precomputed method plan now owns both emitted definitions and internal targets.

Constraint: Existing overload and synthetic-entry behavior is public output
Rejected: Reuse generic identifier labels | generic labels cannot represent C# overload signatures
Confidence: high
Scope-risk: moderate
Tested: structured planner units and full C# renderer tests"
```

### Task 5: Plan C# Declarations, Source Slots, And Static Fields

**Files:**
- Modify: `src/decompiler/csharp/render/structured/plan.rs`
- Modify: `src/decompiler/csharp/render/structured/mod.rs`
- Modify: `src/decompiler/csharp/render.rs`
- Modify: `src/decompiler/csharp/render/header.rs`
- Test: `src/decompiler/csharp/render/structured/tests.rs`
- Test: `tests/typed_declarations.rs`

- [ ] **Step 1: Add failing lexical-scope and static-field tests**

Build typed blocks directly and assert exact declaration decisions for single definitions, branch merges, loop-carried values, parameters, and statics:

```rust
assert_eq!(plan.declarations["t_0"].scope, root_scope);
assert_eq!(plan.declarations["loc0"].kind, DeclarationKind::HoistedAssignment);
assert!(!plan.declarations.contains_key("@class"));
assert_eq!(contract.static_fields[1].name, "static1");
assert_eq!(contract.static_fields[1].csharp_type, "BigInteger");
```

- [ ] **Step 2: Run tests and verify there is no structured declaration planner**

Run:

```bash
cargo test --lib decompiler::csharp::render::structured::tests::plans_declarations -- --nocapture
cargo test --test typed_declarations -- --nocapture
```

Expected: structured planner tests fail to compile; legacy typed-declaration tests remain green.

- [ ] **Step 3: Implement stable scope collection and declaration placement**

Define `ScopeId`, `ScopeTree`, `DeclarationKind::{Inline, HoistedAssignment}`, and `DeclarationPlan`. Traverse every nested `Block`, assign stable preorder scope IDs, record assignment scopes and use scopes, and place each non-parameter/non-static symbol at the nearest common ancestor of all definitions and uses. Select `Inline` only when one definition dominates every use in the same scope; otherwise emit a concrete declaration at the selected scope followed by assignments on original paths.

Unknown/untyped uninitialized symbols must return a `LostStackValue` issue rather than produce `var name;` or `?`.

- [ ] **Step 4: Centralize neutral-to-C# type spelling**

Use one total function:

```rust
fn csharp_type(value_type: ValueType, typed: bool) -> &'static str {
    match (typed, value_type) {
        (true, ValueType::Integer) => "BigInteger",
        (true, ValueType::Boolean) => "bool",
        (true, ValueType::ByteString) => "ByteString",
        (true, ValueType::Buffer) => "byte[]",
        (true, ValueType::Array | ValueType::Struct) => "object[]",
        (true, ValueType::Map) => "Map<object, object>",
        (_, ValueType::Unknown | ValueType::Any | ValueType::Null
            | ValueType::InteropInterface | ValueType::Pointer) => "object",
        (false, _) => "dynamic",
    }
}
```

C# keyword escaping occurs after source-slot de-versioning and is applied exactly once.

- [ ] **Step 5: Emit referenced static slots once at class scope**

Build `CSharpContractSymbols` from `TypeInfo.statics` and the union of `SymbolOrigin::Static` references. Emit `private static <type> staticN;` immediately after `write_contract_open` and before events. Never declare a static slot inside a method.

- [ ] **Step 6: Run planner, typed declaration, and C# envelope tests**

Run:

```bash
cargo test --lib decompiler::csharp::render::structured::tests -- --nocapture
cargo test --test typed_declarations -- --nocapture
cargo test --lib decompiler::tests::csharp -- --nocapture
```

Expected: declarations are legal across branches/loops, parameters are not redeclared, and statics appear once at class scope without changing unrelated envelope output.

- [ ] **Step 7: Commit C# symbol planning**

```bash
git add src/decompiler/csharp/render tests/typed_declarations.rs src/decompiler/tests/csharp.rs
git commit -m "Plan legal C# declarations from neutral symbols

Phi-free structured IR still needs language-specific lexical placement, so C# now de-versions source slots, hoists cross-scope variables, and emits VM statics as class fields.

Constraint: Shared IR must remain declaration-language neutral
Rejected: Restore Stmt::VarDecl | declaration scope and concrete types are renderer policy
Confidence: high
Scope-risk: moderate
Tested: planner units, typed declarations, and C# envelope regressions"
```

### Task 6: Render Typed C# Expressions Without String Reparsing

**Files:**
- Create: `src/decompiler/csharp/render/structured/expr.rs`
- Modify: `src/decompiler/csharp/render/structured/mod.rs`
- Test: `src/decompiler/csharp/render/structured/tests.rs`

- [ ] **Step 1: Add table-driven failing expression tests**

Cover every `Literal`, operator, index/member, cast, array/map, ternary, internal/token/syscall target, and intrinsic used by current lowering. Include nested calls and the user-method/intrinsic collision. Exact expected examples:

```rust
assert_eq!(render_expr(&Expr::Literal(Literal::BigInt("18446744073709551616".into())), &ctx),
           "BigInteger.Parse(\"18446744073709551616\")");
assert_eq!(render_expr(&Expr::Literal(Literal::Bytes(vec![0, 255])), &ctx),
           "new byte[] { 0x00, 0xFF }");
assert_eq!(render_expr(&Expr::Array(vec![Expr::int(1), Expr::int(2)]), &ctx),
           "new object[] { 1, 2 }");
assert_eq!(render_expr(&user_append_call, &ctx), "append(items)");
assert_eq!(render_expr(&vm_append_call, &ctx), "items.Add(value)");
```

- [ ] **Step 2: Run tests and verify the visitor is absent**

Run:

```bash
cargo test --lib decompiler::csharp::render::structured::tests::renders_all_expression_variants -- --exact --nocapture
```

Expected: compile failure because `structured::expr::render_expr` does not exist.

- [ ] **Step 3: Implement precedence-aware recursive expression rendering**

Use `render_expr_prec(expr, parent_precedence, context)` so binary/unary/ternary parentheses are structural. Escape strings/control characters directly. Render wide numeric decimal strings with `BigInteger.Parse`, bytes with target-typed arrays, maps with `new Map<object, object> { [key] = value }`, and arrays/structs with explicit element type.

Match `SemanticCallTarget`, never `display_name()` alone. Internal and method-token calls use planned names; known syscalls map by hash; unknown syscalls use an explicit supported wrapper and retain `Conservative`; VM intrinsics match `Intrinsic::Opcode(opcode)`.

- [ ] **Step 4: Implement effect-safe temporary inlining**

Compute typed use counts from the planned block. Inline only expressions classified pure and used once. Calls, collection mutations, allocations with observable identity, and unknown targets are effectful and cannot be duplicated, discarded, or moved.

- [ ] **Step 5: Run all typed expression units**

Run:

```bash
cargo test --lib decompiler::csharp::render::structured::tests -- --nocapture
```

Expected: all expression variants render compile-oriented C# and nested semantics require no `csharpize_expression` call.

- [ ] **Step 6: Commit the direct expression visitor**

```bash
git add src/decompiler/csharp/render/structured
git commit -m "Render structured expressions directly as C#

Typed calls and literals no longer need pseudo-source reparsing, so the C# visitor now renders expression trees with explicit precedence, types, and effect-safe inlining.

Constraint: No new parser or production dependency
Rejected: Render generic IR then csharpize text | it loses provenance and nested type information
Confidence: high
Scope-risk: moderate
Tested: exhaustive structured expression units"
```

### Task 7: Render Typed Statements And Structured Control Flow

**Files:**
- Create: `src/decompiler/csharp/render/structured/stmt.rs`
- Modify: `src/decompiler/csharp/render/structured/mod.rs`
- Test: `src/decompiler/csharp/render/structured/tests.rs`

- [ ] **Step 1: Add failing statement/control-flow tests**

Cover assignments/declarations, returns, comments, if/else, while, do/while, for, switch/default, try/catch/finally, and recursive termination. Require case `break;` only when the case can fall through:

```rust
assert_eq!(render_block(&switch_block, &plan), expected_switch);
assert!(terminates(&Block::from(vec![Stmt::Return(Some(Expr::int(1)))])));
assert!(terminates(&both_arms_return));
assert!(!terminates(&one_arm_returns));
```

- [ ] **Step 2: Run tests and verify no typed statement visitor exists**

Run:

```bash
cargo test --lib decompiler::csharp::render::structured::tests::renders_all_control_flow_variants -- --exact --nocapture
```

Expected: compile failure because the statement visitor and typed termination analysis are absent.

- [ ] **Step 3: Implement direct block and statement rendering**

Render from `DeclarationPlan`: inline definitions use `<type-or-var> name = value;`, hoisted symbols emit one declaration at their planned scope and all definitions become assignments. Remove a final `return;` from declared void methods. Render a bare return in a non-void method as `return default;` only when the VM supplies no stronger value.

Use four-space indentation relative to the existing method-body indentation and return a string before writing it, so backend selection remains whole-method atomic.

- [ ] **Step 4: Implement recursive termination and switch break insertion**

`terminates` returns true for return/throw/abort/goto, for an `if` only when both arms terminate, and for a block when its final reachable statement terminates. Append `break;` to each nonterminating case/default body; never infer termination from rendered text.

- [ ] **Step 5: Run statement and expression units**

Run:

```bash
cargo test --lib decompiler::csharp::render::structured::tests -- --nocapture
```

Expected: exact C# snapshots pass for all currently represented control flow, declarations, returns, and switch cases.

- [ ] **Step 6: Commit the direct statement visitor**

```bash
git add src/decompiler/csharp/render/structured
git commit -m "Render structured statements with typed control policy

C# switch termination and declarations depend on semantic structure, so blocks and control flow now render directly with recursive termination analysis.

Constraint: Existing method braces and signatures remain owned by methods.rs
Rejected: Reuse line-oriented switch tracking | rendered suffixes cannot prove nested termination
Confidence: high
Scope-risk: moderate
Tested: exhaustive structured statement and control-flow units"
```

### Task 8: Enable The Vertical Typed Body Path With Atomic Fallback

**Files:**
- Modify: `src/decompiler/csharp/render/structured/mod.rs`
- Modify: `src/decompiler/csharp/render/body.rs`
- Modify: `src/decompiler/csharp/render/methods.rs`
- Modify: `src/decompiler/csharp/render.rs`
- Modify: `src/decompiler/tests/csharp.rs`
- Modify: `tests/ir_pipeline.rs`

- [ ] **Step 1: Add failing vertical and fallback tests**

Require:

- `MultiMethod.helper` renders `return 2;` through the typed path.
- `LoopIf.main` has one mutable `loc0`, no `loc0_` suffix, and no adjacent copy temporary.
- ASSERT, PACK, unknown source symbols, unresolved calls, and trace mode select the legacy backend before body text is written.
- One deterministic warning contains method name/start plus primary opcode/offset/reason.
- Exact and conservative bodies never call the legacy writer.

Use an internal coverage record assertion rather than inferring backend choice from formatting:

```rust
assert_eq!(coverage.methods[&helper_start].backend, BodyBackend::Structured);
assert_eq!(coverage.methods[&assert_start].backend, BodyBackend::LegacyFallback);
assert!(warnings.iter().any(|warning| warning ==
    "csharp: main at 0x0000 used legacy body: ASSERT at 0x0001: unsupported opcode"));
```

- [ ] **Step 2: Run focused tests and verify every C# body is still legacy**

Run:

```bash
cargo test --lib decompiler::tests::csharp::csharp_multimethod_uses_structured_constant_fold -- --exact --nocapture
cargo test --lib decompiler::tests::csharp::csharp_assert_uses_deterministic_whole_method_fallback -- --exact --nocapture
cargo test --lib decompiler::tests::csharp::csharp_loopif_deversions_local_slot -- --exact --nocapture
```

Expected: failures because `write_lifted_body` always constructs `HighLevelEmitter` and no backend coverage exists.

- [ ] **Step 3: Select and render into a temporary string before writing output**

Add:

```rust
pub(super) enum BodyBackend { Structured, LegacyFallback, ThrowingStub }

pub(super) struct BodyRenderResult {
    pub(super) source: String,
    pub(super) backend: BodyBackend,
    pub(super) fidelity: FidelityReport,
    pub(super) warnings: Vec<String>,
}
```

`render_method_body(slice, plan, context)` calls `lower_method_body`; exact/conservative clean-mode bodies call the typed planner/visitor, incomplete bodies or `emit_trace_comments=true` call a renamed `render_legacy_body`, and backend failure for a non-void/resource-guarded method returns `throw new NotImplementedException();`. Only after one complete result exists does the caller append source to the method.

- [ ] **Step 4: Retain structured fallback coverage in `CSharpRender`**

Add an internal `CSharpCoverage` map keyed by emitted method start/name with backend, fidelity, and primary issue. Keep public output shapes unchanged. Warnings remain in the existing warning vector.

- [ ] **Step 5: Run the vertical, envelope, and artifact suites**

Run:

```bash
cargo test --lib decompiler::tests::csharp -- --nocapture
cargo test --test ir_pipeline -- --nocapture
cargo test --test typed_declarations -- --nocapture
cargo test --test decompile_artifacts -- --nocapture
```

Expected: representative clean methods use optimized typed bodies, incomplete methods use one visible whole-method fallback, and the mature envelope remains unchanged.

- [ ] **Step 6: Commit the guarded cutover**

```bash
git add src/decompiler/csharp/render src/decompiler/tests/csharp.rs tests/ir_pipeline.rs
git commit -m "Route fidelity-clean C# bodies through structured IR

The typed lowering is now complete for a useful vertical slice, so exact and conservative methods use the direct visitor while known semantic gaps retain deterministic whole-method fallback.

Constraint: No method may mix typed and legacy statements
Rejected: Fall back per unsupported statement | mixed backends can reorder effects and declarations
Confidence: high
Scope-risk: broad
Directive: Legacy fallback is migration scaffolding and must be deleted after the zero-fallback gates
Tested: C# units, IR integrations, typed declarations, and artifact suite"
```

### Task 9: Preserve Assert, Throw, And Abort Semantics

**Files:**
- Modify: `src/decompiler/ir/statement.rs`
- Modify: `src/decompiler/ir/render/stmt/mod.rs`
- Modify: `src/decompiler/cfg/ssa/form.rs`
- Modify: `src/decompiler/cfg/ssa/builder.rs`
- Modify: `src/decompiler/cfg/ssa/to_ir.rs`
- Modify: `src/decompiler/cfg/structure.rs`
- Modify: `src/decompiler/csharp/render/structured/stmt.rs`
- Test: `tests/ir_pipeline.rs`
- Test: `src/decompiler/tests/csharp.rs`

- [ ] **Step 1: Add failing semantic and C# tests**

Test `ASSERT`, `ASSERTMSG`, `THROW`, `ABORT`, and `ABORTMSG` independently. Assert the condition/message operand order, catchability distinction in IR, exact fidelity, and compile-valid C#:

```rust
assert_eq!(stmt, Stmt::Assert {
    condition: Expr::var("condition"),
    message: Some(Expr::var("message")),
});
assert!(matches!(throw_stmt, Stmt::Throw(Some(_))));
assert!(matches!(abort_stmt, Stmt::Abort(Some(_))));
assert_eq!(coverage.methods[&0].backend, BodyBackend::Structured);
```

- [ ] **Step 2: Run tests and verify semantics are currently consumed/dropped**

Run:

```bash
cargo test --test ir_pipeline structured_ir_preserves_assert_and_message -- --exact --nocapture
cargo test --lib decompiler::tests::csharp::csharp_assert_uses_structured_body -- --exact --nocapture
```

Expected: failures because the builder consumes failure operands and the structurer emits a comment for failure terminators.

- [ ] **Step 3: Add shared semantic statement variants and SSA preservation**

Add:

```rust
Stmt::Throw(Option<Expr>),
Stmt::Abort(Option<Expr>),
Stmt::Assert { condition: Expr, message: Option<Expr> },
```

Add corresponding `SsaStmt::Throw(Option<SsaExpr>)`, `SsaStmt::Abort(Option<SsaExpr>)`, and `SsaStmt::Assert { condition: SsaExpr, message: Option<SsaExpr> }` variants. Ensure optimizer use collection sees every operand and DCE cannot remove them. Lower these variants to the matching IR statements and replace the throw/abort comment in `structure.rs` with the exact semantic statement.

- [ ] **Step 4: Render distinct compile-valid C# failure forms**

Render `Assert` as an explicit conditional throw, `Throw` as `throw new Exception(<message>);`, and `Abort` as a deterministic `InvalidOperationException` spelling. Mark throw/abort terminating; assertion itself terminates only on its failing branch. Because a C# exception cannot exactly reproduce Neo VM's uncatchable abort, classify this first typed abort spelling `Conservative` and emit its specific semantic warning.

- [ ] **Step 5: Run semantic, C#, and artifact tests**

Run:

```bash
cargo test --test ir_pipeline -- --nocapture
cargo test --lib decompiler::tests::csharp -- --nocapture
cargo test --test decompile_artifacts -- --nocapture
```

Expected: assertion/failure methods are exact and structured, no semantic fallback remains for these opcodes, and existing catch/return output stays valid.

- [ ] **Step 6: Commit failure semantics**

```bash
git add src/decompiler/ir src/decompiler/cfg src/decompiler/csharp/render/structured src/decompiler/tests/csharp.rs tests/ir_pipeline.rs
git commit -m "Preserve VM failure semantics in structured bodies

Assertions and failures were consumed before structuring, so distinct typed statements now carry their operands through optimization and direct C# rendering.

Constraint: Throw, abort, and assert differ semantically despite similar first-pass C# spellings
Rejected: Model all failures as one generic throw | catchability and conditionality would be lost
Confidence: high
Scope-risk: moderate
Tested: structured IR, C# renderer, and artifact suites"
```

### Task 10: Correct Typed Collections, Type Tags, And Wide Literals

**Files:**
- Modify: `src/decompiler/cfg/ssa/effects.rs`
- Modify: `src/decompiler/cfg/ssa/builder.rs`
- Modify: `src/decompiler/analysis/types.rs`
- Modify: `src/decompiler/ir/expression/expr.rs`
- Modify: `src/decompiler/ir/expression/literal.rs`
- Create: `src/decompiler/helpers/vm_values.rs`
- Modify: `src/decompiler/helpers.rs`
- Modify: `src/decompiler/high_level/emitter/helpers.rs`
- Modify: `src/decompiler/csharp/render/structured/expr.rs`
- Test: `src/decompiler/cfg/ssa/builder.rs`
- Test: `tests/ir_pipeline.rs`
- Test: `src/decompiler/tests/csharp.rs`

- [ ] **Step 1: Add failing operand and collection tests**

Cover `CONVERT`, `ISTYPE`, `NEWARRAY_T`, `PACK`, `PACKSTRUCT`, `PACKMAP`, `UNPACK`, signed little-endian `PUSHINT128/256`, printable PUSHDATA, and raw byte data. Require no `?` and no fallback for valid constant packs.

```rust
assert_eq!(pack, SsaExpr::Array(vec![
    SsaExpr::lit(Literal::Int(1)),
    SsaExpr::lit(Literal::Int(2)),
]));
assert_eq!(pack_struct, SsaExpr::Struct(vec![
    SsaExpr::lit(Literal::Int(1)),
    SsaExpr::lit(Literal::Int(2)),
]));
assert_eq!(wide, Literal::BigInt("-1".to_string()));
assert!(matches!(
    is_type,
    SsaExpr::IsType { target: ValueType::Integer, .. }
));
```

- [ ] **Step 2: Run focused tests and verify current unknowns/wrong effects**

Run:

```bash
cargo test --lib decompiler::cfg::ssa::builder::tests::pack_preserves_elements -- --exact --nocapture
cargo test --lib decompiler::cfg::ssa::builder::tests::convert_consumes_one_value -- --exact --nocapture
cargo test --test ir_pipeline structured_ir_decodes_signed_wide_integer -- --exact --nocapture
```

Expected: failures because PACK emits unknown, CONVERT is modeled with the wrong pop count, and wide bytes are hex-encoded as decimal content.

- [ ] **Step 3: Add neutral typed operand expressions and correct stack effects**

Mirror these variants in `Expr` and `SsaExpr` and update every exhaustive walker and renderer before lowering into them:

```rust
Convert { value: Box<Expr>, target: ValueType },
IsType { value: Box<Expr>, target: ValueType },
NewArray { length: Box<Expr>, element_type: Option<ValueType> },
Struct(Vec<Expr>),
```

Make CONVERT/ISTYPE consume one value. Move type-tag decoding into `helpers/vm_values.rs` as `value_type_from_operand`, use it from both SSA and type analysis, and preserve `NEWARRAY_T` element tags on `NewArray`.

- [ ] **Step 4: Lower PACK families from the symbolic stack**

Build a deterministic definition-fact map during the stabilized final SSA execution so the count variable can resolve to a nonnegative literal. Pop exactly that many elements, reverse VM pop order into source order, and emit `SsaExpr::Array`, `SsaExpr::Struct`, or `SsaExpr::Map` pairs. Lower UNPACK exactly only when its source resolves to an unmodified PACK/PACKSTRUCT definition: replay those elements in reverse VM stack order and push the literal element count. Dynamic or aliased collection sources emit `MissingProvenance` instead of using the legacy forward-scan heuristic or fabricating an unknown.

- [ ] **Step 5: Move and reuse canonical byte/literal helpers**

Move the already tested `format_int_bytes_as_decimal` and `try_decode_string_literal` logic from `high_level/emitter/helpers.rs` into `helpers/vm_values.rs` as `signed_le_bytes_to_decimal` and `printable_utf8`. Reuse those helpers from both legacy high-level output and SSA literal lowering. Convert PUSHDATA to `Literal::String` only when `printable_utf8` succeeds; otherwise keep `Literal::Bytes`.

- [ ] **Step 6: Run builder, type, IR, C#, and artifact regressions**

Run:

```bash
cargo test --lib decompiler::cfg::ssa::builder::tests -- --nocapture
cargo test --lib decompiler::tests::core::analysis -- --nocapture
cargo test --test ir_pipeline -- --nocapture
cargo test --lib decompiler::tests::csharp -- --nocapture
cargo test --test decompile_artifacts -- --nocapture
```

Expected: valid collection/type/literal methods are exact and structured; unsupported dynamic unpack shapes remain incomplete with precise diagnostics.

- [ ] **Step 7: Commit typed collection and literal recovery**

```bash
git add src/decompiler/cfg src/decompiler/analysis/types.rs src/decompiler/ir src/decompiler/csharp/render/structured src/decompiler/tests tests/ir_pipeline.rs
git commit -m "Recover typed collection and operand semantics

PACK, type tags, and wide integers previously lost values or encoded bytes as display text, so lowering now preserves source order, neutral types, and canonical signed literals.

Constraint: No new bigint or parser dependency
Rejected: Keep legacy PACK text for parity | the legacy collection expression can fail Roslyn target typing
Confidence: high
Scope-risk: broad
Tested: SSA builder, type analysis, structured IR, C#, and artifacts"
```

### Task 11: Preserve Loop Transfers, Catch State, And Irreducible Control

**Files:**
- Modify: `src/decompiler/ir/statement.rs`
- Modify: `src/decompiler/ir/render/stmt/mod.rs`
- Modify: `src/decompiler/cfg/ssa/builder.rs`
- Modify: `src/decompiler/cfg/graph/core.rs`
- Modify: `src/decompiler/cfg/structure.rs`
- Modify: `src/decompiler/csharp/render/structured/stmt.rs`
- Test: `src/decompiler/cfg/structure.rs`
- Test: `tests/ir_pipeline.rs`
- Test: `src/decompiler/tests/csharp.rs`

- [ ] **Step 1: Add failing early-transfer, catch, and irreducible tests**

Require `Break`, `Continue`, a typed catch-entry exception variable, and deterministic `Label(BlockLabel)`/`Goto(BlockLabel)` only for an irreducible CFG. Assert reducible loops contain no labels/gotos.

- [ ] **Step 2: Run focused tests and verify transfers/catch values are unavailable**

Run:

```bash
cargo test --lib decompiler::cfg::structure::tests::structures_early_break_and_continue -- --exact --nocapture
cargo test --test ir_pipeline structured_ir_seeds_catch_exception_value -- --exact --nocapture
cargo test --lib decompiler::cfg::structure::tests::irreducible_region_uses_typed_labels -- --exact --nocapture
```

Expected: failures because shared statements do not represent loop transfers/labels and structured SSA does not seed the catch stack.

- [ ] **Step 3: Add shared transfer statements and typed labels**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BlockLabel(pub usize);

Stmt::Break,
Stmt::Continue,
Stmt::Label(BlockLabel),
Stmt::Goto(BlockLabel),
```

Teach use collection, generic rendering, and termination analysis about these variants.

- [ ] **Step 4: Recover loop exits and irreducible regions during structuring**

When an edge targets the active loop follow block, emit `Break`; when it targets the active loop header/latch, emit `Continue`. For a valid region that cannot be reduced, allocate deterministic labels from block IDs and emit typed gotos rather than an incomplete comment. Reducible control must continue to use `If`/loop/switch forms.

- [ ] **Step 5: Seed catch-entry exception state**

Add `Cfg::edge_kind(from, to) -> Option<EdgeKind>`. At a target reached through an `EdgeKind::Exception`, ignore predecessor exit stacks, seed exactly one `SsaVariable::initial("exception")`, attach neutral interop/exception metadata, and expose the chosen catch symbol in `ControlFlow::TryCatch`. Ensure no underflow issue is emitted for the implicit VM exception value.

- [ ] **Step 6: Render transfers and catches directly in C#**

Render `break;`, `continue;`, `label_<id>:`, `goto label_<id>;`, `catch (Exception <escaped>)`, and `finally` with typed termination analysis. Labels never become declaration scopes. Extend declaration planning so any symbol live across a goto/label boundary is hoisted to the nearest common containing scope before the label.

- [ ] **Step 7: Run structurer, IR, C#, and artifact tests**

Run:

```bash
cargo test --lib decompiler::cfg::structure::tests -- --nocapture
cargo test --test ir_pipeline -- --nocapture
cargo test --lib decompiler::tests::csharp -- --nocapture
cargo test --test decompile_artifacts -- --nocapture
```

Expected: reducible loops stay structured, early transfers are semantic, catch bodies receive their exception value, and valid irreducible bytecode no longer requires legacy fallback.

- [ ] **Step 8: Commit control completeness**

```bash
git add src/decompiler/ir src/decompiler/cfg src/decompiler/csharp/render/structured src/decompiler/tests/csharp.rs tests/ir_pipeline.rs
git commit -m "Represent nonlocal VM control transfers explicitly

Early loop exits, catch entry state, and irreducible edges cannot be reconstructed by a renderer after CFG structure is discarded, so shared IR now preserves them as typed control semantics.

Constraint: Reducible control must remain structured and readable
Rejected: Mark every irregular edge incomplete | valid irreducible bytecode would prevent zero fallback
Confidence: medium
Scope-risk: broad
Directive: Labels/gotos are reserved for irreducible regions
Tested: structurer, IR integration, C#, and artifact suites"
```

### Task 12: Carry Statement Origins And Render Typed Trace Comments

**Files:**
- Modify: `src/decompiler/cfg/method_body.rs`
- Modify: `src/decompiler/cfg/ssa/form.rs`
- Modify: `src/decompiler/cfg/ssa/builder.rs`
- Modify: `src/decompiler/cfg/ssa/optimize.rs`
- Modify: `src/decompiler/cfg/structure.rs`
- Modify: `src/decompiler/csharp/render/structured/stmt.rs`
- Modify: `src/decompiler/tests/csharp.rs`

- [ ] **Step 1: Add failing source-map and trace tests**

Require optimized/folded statements to retain the union of contributing offsets and trace mode to keep the structured backend:

```rust
assert_eq!(body.source_map.statement_origins[&StatementId(0)], BTreeSet::from([0, 1, 2, 3]));
assert_eq!(coverage.methods[&0].backend, BodyBackend::Structured);
assert!(csharp.contains("// 0000: PUSH1"));
assert!(csharp.contains("return 2;"));
```

- [ ] **Step 2: Run tests and verify trace mode still falls back**

Run:

```bash
cargo test --lib decompiler::cfg::method_body::tests::source_map_survives_constant_folding -- --exact --nocapture
cargo test --lib decompiler::tests::csharp::csharp_trace_comments_use_structured_body -- --exact --nocapture
```

Expected: failures because structured statements have no origin sidecar and trace mode is routed to legacy.

- [ ] **Step 3: Track SSA statement origins as a sidecar**

Assign stable `SsaStatementId`s during the final SSA build pass and store offset sets separately from semantic nodes. When optimization substitutes/folds expressions, union the origins of consumed definitions into the surviving statement; when it removes a statement, remove its origin entry. Phi edge copies inherit the union of the phi and selected operand definition origins.

- [ ] **Step 4: Assign structured statement IDs and build `SourceMap`**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct StatementId(pub(crate) u32);

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SourceMap {
    pub(crate) statement_origins: BTreeMap<StatementId, BTreeSet<usize>>,
}
```

During structuring, allocate IDs in deterministic preorder and carry the source set for assignments, conditions, semantic failures, transfers, and synthetic phi copies. Keep the map sidecar; do not add trace text to shared IR.

- [ ] **Step 5: Render trace comments from origins**

When `emit_trace_comments` is enabled, look up each statement's sorted origins and render the original instruction mnemonic comment immediately before the typed statement. Remove trace mode from the fallback eligibility list.

- [ ] **Step 6: Run source-map, trace, optimizer, and C# suites**

Run:

```bash
cargo test --lib decompiler::cfg::method_body::tests -- --nocapture
cargo test --lib decompiler::cfg::ssa::optimize::tests -- --nocapture
cargo test --lib decompiler::tests::csharp -- --nocapture
```

Expected: trace and clean modes share identical typed semantics; only comments differ, and no trace-only fallback remains.

- [ ] **Step 7: Commit source provenance**

```bash
git add src/decompiler/cfg src/decompiler/csharp/render/structured src/decompiler/tests/csharp.rs
git commit -m "Carry instruction origins into typed C# traces

Trace mode previously depended on legacy emitted lines, so statement origins now survive optimization and drive comments as a sidecar to structured semantics.

Constraint: Trace metadata must not change shared IR meaning
Rejected: Store comments as semantic statements | optimization and rendering would conflate provenance with behavior
Confidence: medium
Scope-risk: broad
Tested: source-map, optimizer, and C# trace suites"
```

### Task 13: Add Corpus Coverage, Forbidden-Source, And Opcode Gates

**Files:**
- Modify: `src/decompiler/csharp/render.rs`
- Create: `src/decompiler/tests/csharp_coverage.rs`
- Modify: `src/decompiler/tests/mod.rs`
- Modify: `tests/decompile_artifacts/parity.rs`
- Modify: `tools/ci/artifact_sweep.sh`

- [ ] **Step 1: Add failing coverage and forbidden-source tests**

Aggregate every successfully decompiled repository NEF and configured corpus item. Require no unclassified opcode, no unknown fallback reason, and a histogram entry for every incomplete method during migration. Add a final zero-fallback assertion initially marked with a clear test name, not ignored:

```rust
assert!(report.unclassified_opcodes.is_empty(), "{report:#?}");
assert!(report.unknown_reasons.is_empty(), "{report:#?}");
assert_eq!(report.legacy_fallback_methods, 0, "{report:#?}");
```

Scan generated C# bodies for semantic placeholders: standalone `?`, `phi(`, Unicode phi, `**`, raw undefined intrinsic helper calls, and identifiers absent from parameters/fields/declarations.

- [ ] **Step 2: Run gates and capture every remaining classified fallback**

Run:

```bash
cargo test --lib decompiler::tests::csharp_coverage::csharp_corpus_has_zero_structured_fallback -- --exact --nocapture
cargo test --test decompile_artifacts generated_csharp_has_no_semantic_placeholders -- --exact --nocapture
```

Expected: any remaining fallback fails with a deterministic histogram grouped by `LoweringIssueKind`, opcode, and fixture path. Fix each classified semantic gap in the owning earlier task before continuing; do not weaken the gate or whitelist a fallback.

- [ ] **Step 3: Keep coverage crate-internal and aggregate it from unit tests**

Keep public Rust and JSON result shapes unchanged. Add a crate-visible renderer function that returns source, warnings, and per-method backend/fidelity/issue records; call it from `src/decompiler/tests/csharp_coverage.rs`, which recursively loads repository artifacts and configured corpus while retaining access to crate-private coverage. Return a deterministic aggregate with counts by opcode/reason. `tools/ci/artifact_sweep.sh` must invoke both zero-fallback and forbidden-source tests after artifact regeneration.

- [ ] **Step 4: Run the complete corpus and artifact gates until zero**

Run:

```bash
cargo test --lib decompiler::tests::csharp_coverage -- --nocapture
cargo test --test decompile_artifacts -- --nocapture
bash tools/ci/artifact_sweep.sh
```

Expected: zero legacy fallback methods, zero unclassified opcodes/reasons, and zero forbidden generated-source constructs across repository fixtures and configured corpus.

- [ ] **Step 5: Commit measurable cutover gates**

```bash
git add src/decompiler/csharp/render.rs src/decompiler/tests/csharp_coverage.rs src/decompiler/tests/mod.rs tests/decompile_artifacts tools/ci/artifact_sweep.sh
git commit -m "Require measurable structured C# coverage

Fixture success alone cannot prove semantic eligibility, so corpus replay now reports typed backend use and rejects fallback, unclassified opcodes, and invalid source placeholders.

Constraint: Coverage reporting remains internal and deterministic
Rejected: Keep a fallback allowlist | the accepted architecture requires deleting the legacy backend
Confidence: high
Scope-risk: moderate
Directive: New opcodes require classification and corpus-visible reasons
Tested: corpus replay, artifact parity, and artifact sweep"
```

### Task 14: Compile Representative Generated C# With Roslyn

**Files:**
- Create: `tests/csharp_compile.rs`
- Create: `tools/ci/csharp_compile.sh`
- Modify: `README.md`

- [ ] **Step 1: Add the opt-in compile harness and representative cases**

Use `NEO_SMARTCONTRACT_FRAMEWORK_DLL` and `dotnet` from `PATH`. Generate one temporary SDK project per case with a direct assembly reference:

```rust
let framework = std::env::var_os("NEO_SMARTCONTRACT_FRAMEWORK_DLL")
    .expect("NEO_SMARTCONTRACT_FRAMEWORK_DLL is required for csharp_compile");
let status = Command::new("dotnet")
    .args(["build", "--nologo", "--verbosity", "quiet"])
    .current_dir(project.path())
    .status()
    .expect("run dotnet build");
assert!(status.success(), "Roslyn rejected generated C# for {case_name}");
```

Cases must include straight-line returns, LoopIf, switch, assert/abort/throw, PACK arrays/maps, internal calls/overloads, typed declarations, class statics, events, catches, irreducible labels, wide literals, and reserved identifiers.

- [ ] **Step 2: Add a CI wrapper that cannot silently skip**

`tools/ci/csharp_compile.sh` must fail when `dotnet` or the framework DLL is unavailable, then run:

```bash
cargo test --locked --test csharp_compile -- --ignored --nocapture
```

Ordinary `cargo test` portability is preserved by guarding the Rust integration test with `#[ignore = "requires dotnet and NEO_SMARTCONTRACT_FRAMEWORK_DLL"]`; the CI wrapper invokes it with `--ignored` and treats absence as setup failure.

- [ ] **Step 3: Run Roslyn validation against the cached framework**

Run:

```bash
NEO_SMARTCONTRACT_FRAMEWORK_DLL=/absolute/path/Neo.SmartContract.Framework.dll \
  cargo test --test csharp_compile -- --ignored --nocapture
```

Expected: every representative generated contract builds with zero C# errors. Warnings are printed for review and cannot hide an error exit.

- [ ] **Step 4: Document the local/CI command**

Add the exact environment variable and command to the contributor verification section without changing the production dependency list.

- [ ] **Step 5: Commit compile validation**

```bash
git add tests/csharp_compile.rs tools/ci/csharp_compile.sh README.md
git commit -m "Compile generated C# as a release gate

Text snapshots cannot detect undeclared symbols or invalid target typing, so representative typed output now builds through Roslyn against the Neo framework assembly.

Constraint: Framework acquisition remains an explicit CI/environment responsibility
Rejected: Add a Rust Roslyn dependency | no production dependency is needed for process-based validation
Confidence: high
Scope-risk: narrow
Tested: representative generated contracts via dotnet build"
```

### Task 15: Delete The Legacy C# Body Backend

**Files:**
- Delete or reduce: `src/decompiler/csharp/render/body.rs`
- Modify: `src/decompiler/csharp/render.rs`
- Modify: `src/decompiler/csharp/render/methods.rs`
- Modify: `src/decompiler/csharp/helpers.rs`
- Modify: `src/decompiler/tests/csharp.rs`
- Modify: `tools/ci/artifact_sweep.sh`

- [ ] **Step 1: Add a source fence against legacy C# body dependencies**

Add a test/script scan requiring the C# renderer tree to contain none of:

```text
HighLevelEmitter
csharpize_statement(
csharpize_statement_typed(
csharpize_expression(
LegacyFallback
render_legacy_body
```

Keep generic high-level output free to use `HighLevelEmitter`; the fence is scoped to `src/decompiler/csharp/` and C# body call sites.

- [ ] **Step 2: Run the fence and verify migration scaffolding is still present**

Run:

```bash
cargo test --lib decompiler::tests::csharp::csharp_renderer_has_no_legacy_body_backend -- --exact --nocapture
```

Expected: failure naming `render/body.rs` and fallback-only helpers.

- [ ] **Step 3: Remove fallback selection and line-oriented conversion**

Make the structured renderer the only instruction-bearing body implementation. Replace the post-cutover unrecoverable non-void/resource-guard case with the already tested warning-backed throwing stub. Delete `LiftedBodyContext`, `HighLevelEmitter` configuration from C#, `BodyBackend::LegacyFallback`, fallback coverage branches, and helper/parser tests used only by C# string conversion.

Retain shared helpers still used by headers, names, metadata, or other output formats; verify with `rg` before deleting each function.

- [ ] **Step 4: Run the completion fence**

Run:

```bash
cargo test --all-features
cargo test --test corpus_replay -- --nocapture
cargo test --test decompile_artifacts -- --nocapture
bash tools/ci/artifact_sweep.sh
NEO_SMARTCONTRACT_FRAMEWORK_DLL=/absolute/path/Neo.SmartContract.Framework.dll \
  cargo test --test csharp_compile -- --ignored --nocapture
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
git diff --check
rg -n 'HighLevelEmitter|csharpize_statement|csharpize_expression|LegacyFallback|render_legacy_body' src/decompiler/csharp
```

Expected: all Rust/corpus/artifact/Roslyn checks pass; the final `rg` exits 1 with no matches; `Cargo.toml` has no new production dependency; JavaScript/web tests remain unchanged unless shared fixture expectations were intentionally updated.

- [ ] **Step 5: Commit final legacy removal**

```bash
git add src/decompiler/csharp src/decompiler/tests/csharp.rs tools/ci/artifact_sweep.sh
git commit -m "Finish the structured C# body cutover

Opcode classification, corpus replay, forbidden-source scans, and Roslyn compilation now prove the typed path complete, so the legacy C# text lifter and reparsing helpers are no longer retained.

Constraint: HighLevelEmitter remains available to non-C# output formats
Rejected: Keep dormant fallback for safety | it would preserve an unmeasured second semantic backend
Confidence: high
Scope-risk: broad
Directive: Future C# semantics must enter through typed method bodies and the direct visitor
Tested: all features, corpus, artifacts, Roslyn, format, clippy, dependency scan, and forbidden-source scan"
```

## Final Verification Record

Before declaring the migration complete, capture the following evidence in the final report:

- Exact counts for focused C# tests, structured-IR tests, typed-declaration tests, artifact tests, and full `cargo test --all-features`.
- `OpCode::all_known()` classification count with zero unclassified variants.
- Corpus method count, backend histogram, and zero fallback/unknown reason count.
- Roslyn case count and Neo.SmartContract.Framework assembly version/path used.
- Forbidden-source scan with zero C# renderer matches and zero generated semantic placeholders.
- `cargo fmt`, strict clippy, dependency diff, and `git diff --check` results.
- Changed files, deleted fallback code, simplifications made, and any remaining risk outside the selected fixtures/corpus.
