# Structured IR to C# Body Migration Design

**Date:** 2026-07-11
**Status:** Approved direction; specification ready for review

## Goal

Make the Rust C# renderer consume the shared, optimized, phi-free structured IR
instead of lifting pseudo-source text and reparsing it line by line. Preserve the
mature C# contract envelope while moving method semantics onto the same typed
spine used by structured IR.

The completed migration has four observable properties:

1. every successfully lowered instruction-bearing C# method body comes from
   structured IR, and no body uses the legacy text lifter;
2. generated C# uses readable source names and valid declarations rather than
   exposed SSA versions;
3. generated contracts compile against Neo.SmartContract.Framework, using
   explicit warning-backed throwing stubs only when bytecode is unavailable or
   an explicit resource guard prevents safe recovery;
4. the legacy `HighLevelEmitter` and `csharpize_statement` body path can be
   deleted after a measured zero-fallback gate.

This is infrastructure for later contract-pattern and compiler-pattern
recognition. Those recognizers should operate on typed structured methods, not
on rendered strings.

## Current Evidence

The existing C# envelope is mature and independent of body lifting. It already
owns:

- Neo framework imports, namespace, contract declaration, and metadata;
- manifest attributes and events;
- overload-aware signatures and C# keyword escaping;
- synthetic script-entry behavior and offset-less manifest stubs;
- inferred private helper signatures from shared method contracts;
- exact C# method slicing, including detached post-terminator chunks.

The replacement seam is therefore `csharp::render::body::write_lifted_body`.
Replacing the complete C# renderer with `cfg::method_view::render_envelope`
would regress envelope behavior and method discovery.

Fresh executable evidence establishes both the opportunity and the gaps:

- `LoopIf.nef` structures to a phi-free `while`, but exposes `loc0_0`,
  `loc0_3`, and an avoidable `t_4` copy;
- `MultiMethod.nef` folds the helper body from `return 1 + 1;` to `return 2;`;
- a synthetic `ASSERT` contract emits a compile-valid C# check in the legacy
  path but structured IR silently emits only `return 1;`;
- a synthetic `PACK` contract preserves the collection in the legacy path but
  structured IR emits `return ?;`;
- the legacy PACK C# fails Roslyn with `CS9176` because `[t1, t0]` has no target
  type, proving that parity alone is not a sufficient quality gate.

Baseline verification passed:

- 45 focused C# unit tests;
- 31 structured-IR integration tests;
- 3 typed-declaration tests;
- 23 artifact parity tests;
- `cargo fmt --all -- --check`;
- `cargo clippy --all-targets --all-features -- -D warnings`;
- `git diff --check HEAD`;
- Roslyn builds for representative simple and assertion contracts against
  Neo.SmartContract.Framework 3.10.0.

## Approaches Considered

### Phased typed-body cutover (selected)

Keep the C# envelope and exact method partitions, introduce a renderer-neutral
method lowering result, render exact/conservative typed bodies directly, and temporarily
fall back per method while semantic gaps are closed. Track fallback reasons and
make zero fallback a required deletion gate.

This provides early production value without publishing silently incomplete
source. The fallback is migration scaffolding, not the intended architecture.

### Full parity before any cutover

Implement every missing SSA/IR behavior, trace provenance, declarations, and the
C# visitor behind an unused path, then switch all methods at once. This avoids
mixed backends but creates a large unexercised branch and delays feedback on the
new renderer. The current ASSERT and PACK gaps make the one-shot change too
broad to review safely.

### Render generic IR text and run `csharpize_statement`

This is rejected. It preserves the parsing dependency the migration is meant to
remove, cannot place declarations safely across branches, loses typed call
provenance, and would continue converting language semantics through string
patterns.

## Architectural Boundary

The contract envelope remains in:

- `src/decompiler/csharp/render.rs`;
- `src/decompiler/csharp/render/header.rs`;
- `src/decompiler/csharp/render/events.rs`;
- `src/decompiler/csharp/render/methods.rs`.

The existing methods renderer continues to choose the exact instruction slice,
signature, parameter labels, return type, and visibility. It delegates only the
contents between the method braces.

Do not replace C# slicing with a global `MethodTable` iteration. C# currently
includes presentation-only detached chunks that `MethodTable` intentionally
does not treat as stable method starts. Instead, expose the existing
cross-range transfer normalization from `cfg::method_view` so it can lower the
exact slice selected by the C# renderer.

## Renderer-Neutral Method Lowering

Factor the SSA-to-structured-block portion of `cfg::method_view` into an
internal API shaped as follows:

```rust
pub(crate) struct MethodIrRequest<'a> {
    pub start: usize,
    pub end: usize,
    pub instructions: &'a [Instruction],
    pub context: MethodContext,
    pub symbol_types: MethodSymbolTypes,
}

pub(crate) struct MethodSymbolTypes {
    pub parameters: Vec<ValueType>,
    pub locals: Vec<ValueType>,
    pub statics: Vec<ValueType>,
}

pub(crate) struct StructuredMethodBody {
    pub body: ir::Block,
    pub symbols: BTreeMap<String, SymbolInfo>,
    pub return_behavior: ReturnBehavior,
    pub fidelity: FidelityReport,
    pub source_map: SourceMap,
}

pub(crate) struct SymbolInfo {
    pub origin: SymbolOrigin,
    pub value_type: ValueType,
}

pub(crate) enum SymbolOrigin {
    Parameter(usize),
    Local(usize),
    Static(usize),
    Temporary,
}

pub(crate) enum Fidelity {
    Exact,
    Conservative,
    Incomplete,
}

pub(crate) struct FidelityReport {
    pub status: Fidelity,
    pub issues: Vec<LoweringIssue>,
    pub covered_offsets: BTreeSet<usize>,
    pub instruction_count: usize,
}

pub(crate) struct LoweringIssue {
    pub offset: usize,
    pub opcode: OpCode,
    pub kind: LoweringIssueKind,
    pub detail: String,
}

pub(crate) enum LoweringIssueKind {
    UnsupportedControl,
    UnsupportedOpcode,
    LostStackValue,
    MissingOperandMetadata,
    UnresolvedCall,
    MissingProvenance,
    BudgetExceeded,
}

pub(crate) struct StatementId(u32);

pub(crate) struct SourceMap {
    pub statement_origins: BTreeMap<StatementId, BTreeSet<usize>>,
}
```

`Exact` means all observed effects and values have an exact typed
representation. `Conservative` means semantics are preserved but the source is
less abstract, for example a known syscall rendered through an explicit
low-level wrapper. `Incomplete` means information required for correct source
was lost or cannot be represented. Exact and conservative methods use the typed
renderer; only incomplete methods fall back.

The lowerer performs, in order:

1. method-local CFG construction with cross-range transfer normalization;
2. `SsaBuilder` using the supplied `MethodContext`;
3. SSA optimization;
4. source-name allocation;
5. phi lowering and CFG structuring;
6. fidelity aggregation and completeness validation.

Fidelity diagnostics must be emitted while each instruction is lowered. They
cannot be reconstructed by scanning the final `Block`: ASSERT is currently
consumed without a statement, PACK loses its elements before structuring, and
throw/abort terminators collapse to the same comment. `SsaBuilder` therefore
returns an SSA build report containing instruction coverage and semantic-loss
issues, and the method lowerer carries those issues forward through optimization
and structuring.

`ValueType` remains language-neutral. C# type names such as `BigInteger` and
`ByteString` do not enter shared IR.

## C# Symbol And Call Planning

Build one `CSharpMethodPlan` per emitted method before lowering. It is the source
of truth for:

- the exact emitted method name;
- exact emitted parameter names and types;
- return type and `ReturnBehavior`;
- whether arguments begin in slots or on the entry stack;
- internal and method-token `CallContract` values;
- inferred local/static `ValueType` metadata.

The same offset-to-name map must feed method declarations and internal call
expressions. This prevents overload/collision logic from producing a definition
name that differs from its call sites.

C# keyword escaping stays in the C# layer. Generic structured-IR sanitization
must not turn a valid `@class` parameter back into the invalid identifier
`class`.

## Source Names And SSA Destruction

Source slot families are rendered as mutable source variables:

- all SSA versions of local slot `loc0` become `loc0`;
- all SSA versions of static slot `static1` become `static1`;
- argument versions map to the exact emitted C# parameter name;
- temporary and merge-only values retain unique generated identities.

This mapping happens before phi copy scheduling. The existing phi lowering
already removes identity copies and schedules parallel copies with collision
safe temporaries, so collapsing versions of one VM slot produces ordinary
imperative assignments without reintroducing a `phi(...)` call.

The C# body planner then analyzes lexical definition and use scopes:

- dominance-safe single definitions render inline as `var name = value;` or an
  inferred concrete type;
- loop-carried, merge, and cross-scope values are declared once in the nearest
  common containing scope and assigned on their incoming paths;
- parameters are never redeclared;
- uninitialized `var` is never emitted, because C# forbids it;
- unknown symbols make the body incomplete rather than becoming `?` or an
  undeclared identifier.

Static VM slots are contract-level state, not method locals. Build a
`CSharpContractSymbols` plan from `TypeInfo` and referenced static slots, emit
private static fields immediately inside the contract class, and have every
method reference those fields. This is a targeted extension of the existing
envelope, not a replacement for it.

Variable declarations remain renderer metadata. Do not restore `Stmt::VarDecl`
to shared IR.

## Direct Typed C# Visitor

Add a focused structured-body renderer under
`src/decompiler/csharp/render/structured/`. It recursively renders `ir::Block`,
`Stmt`, `ControlFlow`, and `Expr` without first producing pseudo-source text.

The visitor owns C#-specific policy:

- literal escaping and `BigInteger.Parse` for wide integers;
- typed byte arrays, arrays, structs, and map initializers;
- `BigInteger` and Neo `Helper` mappings for numeric intrinsics;
- `is null`, type tests, and casts;
- collection mutations such as index assignment, `Add`, `Remove`, `Clear`, and
  `Reverse`;
- known syscall and method-token API names;
- `if`, `while`, `do`, `for`, `switch`, `try`, `catch`, and `finally` syntax;
- required switch `break;` insertion using typed termination analysis;
- trailing void-return removal and compile-valid non-void fallback behavior;
- optional pure temporary inlining based on typed use counts.

Effectful calls are never duplicated, discarded, or moved by temporary
inlining.

Call rendering must not dispatch on the displayed name. A user method named
`append`, `syscall`, or `assert` must not be mistaken for a VM intrinsic. Replace
the name-only call target with a typed discriminator before promoting such calls
to structured C#:

```rust
pub(crate) enum SemanticCallTarget {
    Internal { offset: usize, name: String },
    MethodToken { index: usize, name: String },
    Syscall { hash: u32, name: Option<String> },
    Intrinsic(Intrinsic),
    Unresolved,
}

Expr::Call {
    target: SemanticCallTarget,
    args: Vec<Expr>,
}
```

The rendered C# name remains metadata; call kind and identity remain semantic.

The existing string helper remains only for methods using the transitional
legacy backend. The structured visitor must not call `csharpize_statement` or
`csharpize_expression`.

## Shared Semantic Additions

Shared statement forms are justified when they represent VM/control semantics,
not C# syntax:

```rust
Stmt::Throw(Option<Expr>)
Stmt::Abort(Option<Expr>)
Stmt::Assert { condition: Expr, message: Option<Expr> }
Stmt::Break
Stmt::Continue
Stmt::Label(BlockLabel)
Stmt::Goto(BlockLabel)
```

Throw, abort, and assert remain distinct because they differ in catchability and
conditionality even if their first C# spelling uses exceptions. `Break` and
`Continue` preserve early loop transfers. Typed labels and gotos are a last
resort for irreducible CFG regions; reducible control flow must still render as
structured constructs. Without this escape hatch, a zero-fallback final state
would be impossible for valid irreducible bytecode.

Do not add source-language declarations to shared IR.

## Completeness Gate

The structured path is eligible only when it can prove that the method body is
semantically represented and renderable. `LoweringIssue` is deterministic and
includes at least:

- unsupported or irreducible control transfer;
- missing assertion/failure/loop-transfer semantics;
- unknown stack value reaching source output;
- unsupported dynamic stack operation;
- unresolved or collision-ambiguous call target;
- missing catch-entry exception state;
- unsupported conversion or type-test operand;
- missing collection elements from PACK/UNPACK;
- requested trace provenance that is not yet available.

During migration, `Fidelity::Incomplete` selects the legacy renderer for the
entire method before any body text is written. A method never contains a mixture
of typed and legacy statements. `Fidelity::Conservative` remains on the typed
path and emits a specific semantic warning when user interpretation could be
affected.

Fallback reasons are retained in an internal coverage report and tests. Each
fallback also emits one deterministic warning containing the method name,
method offset, offending opcode and instruction offset, and primary typed
reason. This makes backend use visible without adding comments to the generated
source. Failure of both backends remains a separate semantic-degradation
warning.

The final cutover requires zero structured fallback across the repository
fixtures and configured corpus. At that point the fallback branch and legacy
string conversion code are deleted. Keeping fallback after the zero gate is not
an accepted end state.

## Migration Sequence

### Phase 1: Vertical typed-body path

- extract method-slice lowering and neutral symbol metadata;
- add instruction-origin fidelity diagnostics and the completeness result;
- introduce typed call targets before intrinsic-specific C# rendering;
- implement direct rendering for assignments, returns, calls, expressions,
  conditionals, loops, switches, and collection mutations already represented
  losslessly;
- implement declaration/scope planning and source-slot de-versioning;
- route exact and conservative clean-mode methods through the typed visitor;
- retain whole-method fallback for incomplete and trace-enabled methods.

This phase must visibly improve representative output: `MultiMethod.helper`
renders `return 2;`, and `LoopIf.main` renders one mutable `loc0` without SSA
suffixes or the adjacent copy temporary.

### Phase 2: Semantic parity

- lower assertions, throw, and abort without losing operands;
- recover early loop `break` and `continue`;
- emit typed label/goto fallbacks for valid irreducible control flow;
- seed catch-entry exception state in structured SSA;
- correct CONVERT/ISTYPE stack and operand modeling;
- lower PACK/PACKMAP/PACKSTRUCT/UNPACK into existing array/map/struct
  expressions;
- preserve NEWARRAY_T, ISTYPE, and CONVERT operand type tags;
- decode PUSHINT128/PUSHINT256 little-endian signed values into a canonical
  numeric literal rather than treating their bytes as a display string;
- retain printable PUSHDATA string recovery without changing byte-string
  semantics.

Each correction removes its corresponding `LoweringIssue` and increases typed
coverage. No opcode becomes eligible merely because its output happens to look
plausible.

### Phase 3: Provenance and legacy removal

- attach instruction-origin metadata to structured statements;
- render trace comments from that sidecar without changing IR semantics;
- require zero fallback on all repository fixtures and the configured corpus;
- delete C# use of `HighLevelEmitter`;
- delete obsolete line-oriented C# conversion helpers and their parser-only
  tests;
- retain direct typed-renderer and generated-source compilation tests.

## Failure And Return Policy

- Empty void bodies render an explanatory comment and rely on C#'s implicit
  return.
- Empty or incomplete non-void bodies never emit a comment-only method. During
  migration they fall back; after cutover an unrecoverable body emits a clear
  warning and a compile-valid throwing stub.
- Bare returns in declared/inferred non-void methods render `return default;`
  only when the VM provides no value and no stronger recovery exists.
- Unknown stack values do not become C# identifiers. They make the body
  incomplete.
- The existing 16,384-instruction resource budget remains during migration.
  Budget exhaustion is a typed `LoweringIssue`; it preserves the existing
  warning and uses a compile-valid throwing stub when no backend can lower the
  method.
- `inline_single_use_temps=true` enables effect-safe typed inlining;
  `false` preserves every temporary with valid declarations.
- `emit_trace_comments=true` selects legacy fallback until structured origin
  metadata is available, then uses the same typed body with trace comments.
- `typed_declarations=true` uses neutral `ValueType` evidence; false keeps
  declaration spelling conservative without losing required static/local scope.

## Verification Strategy

### Typed renderer units

Construct `ir::Block` values directly and assert exact C# for:

- declarations versus reassignment across branches and loops;
- every `Expr` and `ControlFlow` variant;
- nested intrinsic calls without string reparsing;
- wide integers, strings, byte arrays, arrays, and maps;
- switch termination and break insertion;
- catch syntax and failure termination;
- reserved identifiers and symbol collisions;
- effect-safe temporary inlining.

### Vertical integration regressions

Use real NEF containers to prove:

- MultiMethod constant folding;
- LoopIf source names and loop structure;
- manifest parameter names and typed declarations;
- private void/value calls and ambient stack preservation;
- method-token call names and contracts;
- switch recovery;
- collection mutation ordering;
- ASSERT preservation;
- PACK/UNPACK collection recovery;
- trace-mode backend behavior during migration.

### Compile validation

Generate representative C# files into a temporary project and invoke Roslyn
against Neo.SmartContract.Framework. The harness uses `std::process::Command`
and an explicitly supplied framework assembly path, adding no Rust dependency.

The required compile set includes straight-line returns, loops, switches,
assert/failure paths, arrays/maps, internal calls, typed declarations, events,
and reserved identifiers. Compilation failures are release blockers, not
snapshot updates.

When the local framework assembly is unavailable, ordinary Rust tests remain
portable, but the dedicated C# validation job must provide it and may not skip.

### Completion fence

- focused C# and structured-IR tests;
- full Rust test suite;
- artifact parity and configured corpus replay;
- zero structured fallback coverage report;
- known-opcode coverage matrix showing each opcode as exact, conservative, or
  incomplete with a tested reason;
- corpus fallback histogram with no unclassified reason;
- generated C# compile validation;
- forbidden-source scan for `?`, `phi`, `**`, undefined raw intrinsic helpers,
  and undeclared identifiers;
- `cargo fmt --all -- --check`;
- `cargo clippy --all-targets --all-features -- -D warnings`;
- dependency policy and `git diff --check`;
- unchanged JavaScript and web suites unless shared fixtures or public result
  shapes are intentionally updated.

## JavaScript Boundary

The JavaScript implementation does not currently emit C#. No JavaScript C#
backend is introduced in this migration. Shared NEF fixtures and method-contract
expectations remain aligned, and later pattern-recognition work should define a
language-neutral result shape before either implementation renders it.

## Non-Goals

- Replacing the C# contract envelope.
- Inlining called methods.
- Inferring source variable names not supported by bytecode or manifest data.
- Adding speculative contract-pattern recognizers before the typed body path is
  stable.
- Treating a temporary legacy fallback as the completed architecture.
- Emitting textual goto placeholders in reducible control flow; typed labels are
  reserved for irreducible regions.
