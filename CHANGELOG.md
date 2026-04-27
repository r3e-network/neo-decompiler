# Changelog

All notable changes to this project will be documented in this file. This
project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.7.0] - 2026-04-28 (Rust) / [1.4.0] - 2026-04-28 (JS)

This release surfaces a large body of metadata that previously lived
only in the rendered comment text — `compiler`, `source`,
`manifest.groups`, NEF method tokens, and `script_hash` are now
addressable as structured fields across the high-level / C# headers,
the CLI / web JSON reports, and the bundled JSON schemas. Plus
significant rendering polish (DUP/OVER/TUCK skip-on-literal,
K&R `} else {` join, untranslated-opcode warnings) and CLI / `--help` /
README ergonomics. All changes are additive; no breaking surface.

### Changed

- **CLI `info` text: skip the `Compiler:` line when the NEF carries
  no compiler string (parity with how `Source:` is handled).** A
  NEF with an all-zero 64-byte compiler field would render as
  `Compiler: \n` (a trailing-space line) — visually noisy. Now
  silently skipped when empty, matching the existing `Source:`
  behaviour. The JSON path still emits the field always (schema
  contract) so programmatic consumers see a stable shape.

- **Apply `cargo fmt` to the whole crate.** A series of small
  hand-edits accumulated rustfmt drift across ~25 files (collapsed
  `format!()` arguments, `\n` continuation alignment, etc.). Ran
  `cargo fmt` to bring the tree back to a clean
  `cargo fmt --check`. No behaviour change. Useful so reviewers see
  only semantic diffs, not formatter noise.

### Documentation

- **CLI `--help` and crate metadata describe the full feature set.**
  The clap top-line was `"Inspect Neo N3 NEF bytecode"` and the
  `Cargo.toml` description was `"Minimal tooling for inspecting
  Neo N3 NEF bytecode"` — neither hinted that the binary actually
  decompiles, lifts to high-level pseudocode and C#, exports CFG
  as DOT, or surfaces JSON reports. Updated both: `--help` now
  carries a `long_about` listing each subcommand's role and noting
  manifest auto-discovery; `Cargo.toml` description and keywords
  (`+decompiler`, `+smart-contract`) match the actual scope.
  Improves crates.io discoverability and `neo-decompiler --help`
  for new users.

### Changed

- **`.gitignore`: ignore `/fuzz/corpus/` and `/fuzz/artifacts/`.**
  These are runtime fuzz output directories created by `cargo
  fuzz`; they're per-developer scratch space, not committed
  corpora. Without them in `.gitignore`, `git status` would
  surface stale untracked entries every time a developer ran the
  fuzz harness locally. (`/fuzz/target/` and `/fuzz/Cargo.lock`
  were already ignored.)

### Documentation

- **README: refreshed CLI examples to reflect the new JSON surfaces
  added in this Unreleased cycle.** `info --format json` now lists
  `compiler` and `source` alongside the existing fields; `disasm`
  and `tokens` JSON now mention the top-level script hash; `cfg`
  callout documents the new graph label; `decompile --format json`
  callout updated to enumerate the actual top-level fields.
  No-op-changed code, but stops sending users to the old README
  spec.

- **JS port: JSDoc comments on the top-level decompile entry points
  (`decompileBytes`, `analyzeBytes`, `decompileBytesWithManifest`,
  `decompileHighLevelBytes`, `decompileHighLevelBytesWithManifest`).**
  Previously the public functions had no JSDoc; IDE consumers
  relying on JS source (no TypeScript) saw nothing on hover. Added
  short JSDoc with parameter notes and reference to `DecompileOptions`
  in the d.ts. Also tightened d.ts comments that referenced
  unexported helpers (`formatPermissionEntry`, `renderExtraScalar`)
  to describe the behaviour directly instead.

- **CLI: every subcommand's `<PATH>` argument now has a `--help`
  description; the global `--manifest` flag documents auto-discovery.**
  `--help` previously printed bare `<PATH>` for every subcommand
  with no description, leaving users to guess what kind of file to
  pass. Each NEF-consuming subcommand now describes the path; the
  global manifest flag advertises the `<NEF>.manifest.json` next-to-
  the-NEF auto-discovery rule that `resolve_manifest_path` already
  implemented but never surfaced in the help text.

### Changed

- **CLI `cfg`: graph carries a `label` attribute showing the contract
  name (when manifest known), script hash, and instruction count.**
  Multi-CFG dumps to a single graphviz canvas were unidentifiable
  — every box just said `BB0`/`BB1`/etc. Added a top-anchored
  `label="<name> (<hash>, <N> instr)"` (or `label="<hash> (<N>
  instr)"` without a manifest) at the digraph level. The `cfg`
  runner now also auto-loads the manifest (parity with `info` /
  `decompile`) so the title surfaces the contract name when one
  exists. Test in `decompile.rs` covers both branches; pre-existing
  `&PathBuf` clippy warning on the cfg runner cleaned up to
  `&Path` while in the area.

- **Web API: `WebDisasmReport` now surfaces `script_hash_le` /
  `script_hash_be` (parity with `WebInfoReport`/
  `WebDecompileReport`).** The browser disassembly surface dropped
  the script hash entirely — page UIs had to call `disasm_report`
  *and* `info_report` to render an "explorer link beside the
  instruction stream" view. `disasm_report` now parses the NEF
  directly so the script hash is available without a second round
  trip; `build_disasm_report` takes `&NefFile` for the same
  reason. Assertion in `web_api.rs` covers the new field.

- **CLI JSON: `disasm` and `tokens` reports now surface
  `script_hash_le` and `script_hash_be` at the top level (parity
  with `info` and `decompile`).** Without these fields, callers
  using the JSON output for cross-referencing (against an
  explorer URL, an `info` report, or a database row) had to parse
  the NEF themselves to recover the script hash. Added the two
  fields as required across both schemas; bumped shared
  `SCHEMA_VERSION` 1.2.0 → 1.3.0. Smoke assertions in
  `cli_smoke/disasm.rs` and `cli_smoke/tokens.rs` cover the new
  surface.

- **JS port: tighten TypeScript declarations for `manifest.groups`,
  `manifest.permissions`, and `manifest.trusts`.** `index.d.ts`
  previously typed these fields as `unknown[]`/`unknown[] | null` —
  technically correct but useless for downstream TypeScript users.
  Replaced with proper interfaces: `ManifestGroup` (`pubkey`,
  `signature`), `ManifestPermission` (`contract`, `methods` with
  wildcard / structured variants), and a `ManifestTrusts` union
  covering all four runtime shapes (`null` / `"*"` / `string[]` /
  `{hashes?: string[]; groups?: string[]}`). Added doc comments
  pointing readers at the shape they need. Tested with `tsc
  --noEmit` (no errors).

- **JSON schema: `decompile` schema now declares `compiler` and
  `source` fields (matching the runtime report); schema version
  bumped 1.1.0 → 1.2.0.** Last iteration added `compiler`/`source`
  to `DecompileReport` at the implementation level, but the
  bundled JSON schema (`docs/schema/decompile.schema.json`) didn't
  describe them, so `assert_schema(SchemaKind::Decompile, ...)`
  validators couldn't enforce the surface. Marked `compiler` as
  required (matches the always-present runtime field, parallel to
  `info` schema); `source` is `["string", "null"]`. Bumped the
  shared `SCHEMA_VERSION` to 1.2.0; smoke-test version assertions
  updated.

- **CLI `info` text format: split combined `ABI methods: N events: M`
  line into two parallel rows.** The packed form mixed sentence
  capitalisation (uppercase `ABI methods`, lowercase `events`) and
  forced grep callers to anchor on a single line containing two
  numbers. Now renders as `ABI methods: 1` and `ABI events: 0` on
  separate lines, parallel to the other `Key: Value` rows in the
  block. Smoke test in `cli_smoke/info.rs` asserts both lines.

- **Web API + CLI JSON: surface NEF `compiler` and `source` at the
  top level of `decompile` reports.** `info --format json` already
  exposed both fields, but `decompile --format json` and the
  browser-facing `WebDecompileReport` only embedded them inside
  the rendered text. Programmatic consumers had to parse the
  rendered comment lines to recover the values. Added `compiler:
  String` and `source: Option<String>` fields to both
  `DecompileReport` (CLI) and `WebDecompileReport` (web), populated
  from `nef.header.*` (with `compiler` trimmed of trailing NULs to
  match `info`). Empty `source` serializes as `null`. New
  assertion in `web_api.rs` covers the surface; the CLI surface is
  trivially exercised through the existing JSON consumers.

- **Both ports: surface NEF `compiler` and `source` header fields in
  the contract header (high-level + C#).** The NEF header carries
  a 64-byte compiler string and a varlen `source` string (commonly
  a repo URL or commit hash). Both are visible via the `info`
  command but were silently dropped from decompiled outputs,
  forcing readers to run a separate command to learn what
  produced the bytecode. Now rendered as `// compiler: <value>`
  and `// source: <value>` comment lines under the script hash,
  with empty fields skipped. Same shape in both Rust and JS
  ports; JS context is extended with `compiler` / `source` from
  `nef.header.*`. Regression tests in `csharp.rs` and
  `decompiler.test.mjs` cover the present-compiler + absent-source
  case (both ports).

- **C# emitter: surface NEF method tokens as `// method tokens
  declared in NEF:` comment block in the contract header (parity
  with high-level).** The high-level renderer has long emitted
  `// method tokens declared in NEF` followed by one line per
  token (with native-contract label, hash, params, returns, and
  call flags). The C# header silently dropped the table — readers
  had to scrape the NEF separately to figure out what each CALLT
  in the body was calling. Mirrors the high-level layout under
  the C# 8-space header indent (`//   <name> (<contract>::<method>)
  hash=... params=N returns=true flags=0xNN (Flag1|Flag2|...)`).
  When the contract hash is recognised but the method name isn't
  exposed, emits a `//   warning: native contract X does not
  expose method Y` follow-up. Empty token tables emit nothing.
  Two new tests in `csharp.rs` exercise the block (with StdLib's
  Serialize) and the empty-tokens skip.

- **C# emitter: render `manifest.groups` as a `// groups:` comment
  block in the contract header (parity with high-level).** The
  high-level summary already opens a `groups { pubkey=... }` block
  for non-empty groups, but the C# header dropped the field
  silently. Neo SmartContract Framework has no source-level
  attribute for `groups` (the pubkey/signature pairs are set at
  deployment time, not declared in code), so the right surface is
  a comment block — same shape as the existing `// permissions:`
  and `// trusts:` blocks. Signatures are intentionally elided
  (opaque base64, no human value); only the pubkey is shown.
  Empty `groups` arrays still emit nothing. Regression test in
  `csharp.rs` extends the existing groups test with assertions on
  the C# output.

- **C# emitter: extended literal-cast strip to all helper rewrites
  with `int_cast_args` (`pow`, `left`, `right`, `substr`, etc.).**
  Iteration 99 added `wrap_int_cast_unless_literal` for
  `new_array`/`new_buffer`. The same `(int)(arg)` pattern was
  duplicated in `format_helper_with_casts` (used by every helper
  rule with non-empty `int_cast_args`). Routed those through the
  same helper so `pow(2, 8)` lifts to `BigInteger.Pow(2, 8)` instead
  of `BigInteger.Pow(2, (int)(8))`. Variable / expression operands
  still get the defensive cast. Test coverage in csharp.rs now
  exercises `pow`/`left`/`substr` with literal arguments.

- **C# emitter: skip redundant `(int)` cast on bare integer literals
  in `new T[N]` constructors.** `new_array(3)` and `new_buffer(8)`
  lifted from NEWARRAY/NEWBUFFER were rewritten to
  `new object[(int)(3)]` / `new byte[(int)(8)]`. The defensive
  `(int)` cast is necessary for any expression that could carry
  BigInteger semantics, but is redundant noise for bare integer
  literals (which are unambiguously `int` to the C# parser). New
  `wrap_int_cast_unless_literal()` helper strips the cast for pure
  decimal literals (`3`, `-5`, `100`) while keeping it for variable
  / expression operands. Output is now `new object[3]` instead of
  `new object[(int)(3)]`.

### Documentation

- **README: refreshed stale syscall count and library example version
  pin.** The JS port description claimed "44 syscalls supported"; both
  Rust and JS now ship 41 syscalls (verified by hash-set diff).
  Updated to 41 with an explicit parity note. The library example's
  `Cargo.toml` snippet pinned to `0.6.0`, while the crate is at 0.6.3
  — broadened to `"0.6"` to keep the example accurate without further
  patch-bump churn.

- **README: refreshed CLI `decompile` flag examples to match the
  clean-by-default behaviour.** The example block still positioned
  `--clean` as the "maximum-readability" command and described
  `--inline-single-use-temps` as "experimental" — both held before
  the CLI default flip in iteration 36, but no longer true. Replaced
  with: a description of the new default rendering (no trace
  comments, inlined temps, dead let-bindings dropped); the new
  opt-in `--trace-comments` and `--no-inline-temps` flags for
  bytecode-correlation work; and a note that the legacy aliases
  (`--clean`, `--no-trace-comments`, `--inline-single-use-temps`)
  remain accepted but redundant.

### Changed

- **JS port: extracted shared `extractContractName` helper.** The
  contract-name fallback (`manifest.name?.trim() → sanitize →
  "NeoContract"`) was inlined in `high-level.js` and a different
  inconsistent variant (`manifest ? sanitizeIdentifier(name) :
  "Contract"`) lived in `grouped-pseudocode.js`. Pulled both into a
  single `extractContractName` export in `manifest.js` mirroring
  Rust's `decompiler::helpers::extract_contract_name` semantics.
  Both renderers now share the same fallback (`"NeoContract"` for
  null/empty manifest names; `"param"` when the trimmed name
  sanitises to empty, since the underlying sanitiser substitutes
  `"param"` for an empty result). Direct unit tests cover the
  fallback variants.

### Changed

- **Rust: added RET to `detect_implicit_else` terminator list.**
  Iteration 113 first explored this but reverted because the
  resulting `}\nelse {` form diverged from JS's K&R `} else {`. With
  the new `join_close_brace_with_chain` final-pass cleanup
  (above), RET can now safely join ABORT/ABORTMSG/THROW as a
  noreturn terminator that triggers an explicit else block.
  `if (cond) { return X; } else { return Y; }` patterns now render
  byte-identical between ports — matches the original C# source
  intent more faithfully than the early-return form.

- **Rust: final-pass `} else {` formatting cleanup (K&R join).** The
  emitter pushed close-braces and chain-continuation headers (`else
  {`, `else if cond {`, `catch (...) {`, `finally {`) as separate
  statements, rendering as multi-line `}\n    else {`. JS port has
  always produced single-line `} else {` (K&R style). Added a
  final-pass `join_close_brace_with_chain` postprocess that collapses
  the pair into a single line. Runs after all other passes so
  intermediate-state line-vector assertions in `postprocess.rs` tests
  remain unaffected. Throw-implicit-else now byte-identical between
  ports for the same NEF.

- **Rust: extended skip-on-literal optimization to OVER and TUCK.**
  Iteration 100 added `is_simple_literal_or_identifier` to skip
  temp materialization on `DUP` of bare literals/identifiers.
  Promoted the helper from `basic.rs` to `manipulation.rs` (shared
  `pub(super)`) and applied the same skip to `OVER` (`over_second`)
  and `TUCK` (`emit_tuck`). All three stack-duplicating opcodes
  now share consistent behaviour: complex expressions still get
  materialized so any side-effecting evaluation runs exactly once,
  bare literals just push another copy of the expression string.

- **Rust: skip temp materialization on DUP of simple literals /
  identifiers (JS parity).** `dup_top` always pushed
  `let tN = value; // duplicate top of stack` — even for bare
  literals like `5` where re-using the same expression string is
  semantically equivalent. The trailing comment also blocked the
  inliner from collapsing the temp. JS's `materialiseStackTopForDup`
  has long used a `SIMPLE_IDENT_OR_LITERAL_RE` check that skips
  materialization for bare literals (decimal, hex, true/false/null,
  strings, plain identifiers). Ported the same check to Rust so
  `PUSH5; DUP; ADD; RET` cleanly lifts to `return 5 + 5;` instead of
  `let t0 = 5; return t0 + t0;`. Complex expressions still get
  materialized so any side-effecting evaluation runs exactly once.

- **Both ports: unified warning-comment prefix to `// warning:`.**
  Rust's `warn()` emitted `// XXXX: <message>` (offset-prefixed) and
  JS used a mix of `// warning: <message>` and `// XXXX: <message>`.
  The mixed convention made warning lines look indistinguishable
  from trace comments (`// XXXX: OPCODE`). Aligned both ports on the
  single semantic prefix `// warning: <message>` for all inline
  warning annotations, since "warning" is a clear semantic indicator
  distinct from per-instruction tracing. Structured warnings (the
  `warnings: Vec<String>` channel surfaced to programmatic
  consumers) still carry the offset hex.

### Fixed

- **Rust: unknown-syscall annotation rendered as trailing
  `// unknown syscall`, blocking inline-temp collapse.** The
  unknown-syscall code path emitted `let t0 = syscall(0xHASH); //
  unknown syscall` (trailing comment) plus a manually-pushed
  structured warning, while all other `warn()` callers used a
  leading `// warning:` line. The trailing comment also acted as a
  barrier to the single-use-temps pass, so `return t0;` couldn't
  collapse into `return syscall(0xHASH);`. Routed the annotation
  through `warn()` so it emits a leading
  `// warning: unknown syscall 0xHASH` line (consistent with the
  iteration 97 unification + JS port). The inliner can now run on
  the unknown-syscall temp, yielding tighter clean-mode output.

- **Rust: missing-syscall-args inline comment was suppressed in
  clean mode.** The syscall handler routed the inline
  `// warning: missing syscall argument values for X (substituted
  ???)` annotation through `note(...)`, which gates on
  `emit_trace_comments`. In clean mode the annotation was dropped
  while the structured warning kept firing — same pattern as
  iteration 96 (untranslated-opcode comment). Updated to use
  `warn(...)` so the inline annotation always surfaces. JS has
  always emitted this comment unconditionally; both ports now
  byte-identical here.

- **Rust: CALLT was dropped entirely from rendered source on stack
  underflow.** When CALLT was reached without enough values on the
  evaluation stack to satisfy the declared parameter count, Rust
  early-returned after `stack_underflow(...)`, leaving the lifted
  source missing the call entirely (only the structured warning
  channel surfaced the hazard). A reader of the rendered source saw
  the following RET as if the token call had never happened. JS has
  always substituted `???` for missing args and emitted the call
  shape; Rust now does the same so both ports surface the call
  attempt at the call site.

- **JS port: CALLT method-token label not qualified through native
  contract describe table.** JS populated `calltLabels` from raw
  `token.method` (e.g. `Transfer`), while Rust runs each token
  through `native_contracts::describe_method_token` to produce
  contract-qualified labels (`GasToken::Transfer`). Calls into
  native contracts therefore rendered as bare `Transfer(...)` in JS
  versus `GasToken::Transfer(...)` in Rust. Curated test artifacts
  declared method tokens but never invoked them via CALLT, so the
  differential test didn't catch this. Updated JS index.js to call
  `describeMethodToken(...)` when populating `calltLabels`.

- **JS port: PUSHA pushed bare integer instead of `&fn_0xNNNN`
  function-pointer expression.** `pushImmediate`'s PUSHA branch in
  `high-level-slots.js` pushed the absolute target as a plain integer
  string (e.g. `123`), conflating PUSHA with PUSHINT and losing the
  function-pointer semantics. Rust's `resolve_pusha_display` formats
  the same operand as `&{label}` (when the absolute target is a
  known method, e.g. `&main` / `&sub_0x000C`) or `&fn_0x{:04X}` as
  a fallback (uppercase hex matching the Rust format). JS now
  consults `state.context.methodLabelsByOffset` and emits
  `&fn_0xNNNN` (uppercase) for unresolved targets. Curated test
  artifacts didn't exercise PUSHA at all, so the differential never
  tripped on it; new regression test in `decompiler.test.mjs`.

- **JS port: lowercase hex in `sub_0xNNNN` helper labels and
  unresolved-call labels (parity bug).** Two related labelling
  divergences caught via synthetic-NEF probing: (1) inferred-helper
  definitions in `methods.js` used
  `.toString(16).padStart(4, "0")`, lowercasing hex letters
  (`sub_0x000a` vs Rust's `sub_0x000A` from
  `format!("sub_0x{:04X}")`); (2) the unresolved-call fallback in
  `high-level-calls.js` used `sub_0x` prefix with lowercase hex,
  while Rust uses a distinct `call_0x{target:04X}` prefix to signal
  "out-of-range / unresolved internal call". Both now route through
  `hex16()` (uppercase) and the JS unresolved-call fallback uses the
  `call_` prefix. Curated test artifacts never tripped on this
  because no helper sits at an offset with hex letters; new
  regression test covers offsets 0x000A and 0x000B.

- **Rust: untranslated-opcode inline comment was suppressed in clean
  mode, hiding real holes in the lifted source.** `warn()` delegated
  to `note()`, which gated on `emit_trace_comments`. In clean mode
  (the CLI default) the `// XXXX: <opcode> (not yet translated)`
  marker was silently dropped from the rendered source, even though
  the opcode wasn't actually translated; only the structured
  `warnings: Vec<String>` channel kept firing. A reader of the lifted
  source had no in-place signal that the output was incomplete. The
  JS port has always emitted the comment unconditionally — the
  source-level marker is a real correctness signal, not a debugging
  trace. Updated Rust `warn()` to always push the inline comment;
  both ports now byte-identically surface untranslated-opcode markers
  regardless of trace mode. Regression test in core/unknowns.rs.

- **JS port: syscall operand rendered as bare `0xHASH` regardless of
  whether the syscall was known.** Rust's `Display for
  Operand::Syscall` prefixes the resolved name (`System.Storage.Get
  (0x12345678)`); JS dropped that prefix. Existing testing artifacts
  don't exercise `SYSCALL`, which is why the byte-identical
  differential never caught it. Updated `formatOperand` to look up
  `SYSCALLS.get(value)` and emit `Name (0xHASH)` for known syscalls,
  falling back to bare hex for unknown / reserved hashes (mirrors
  the Rust `if let Some(info) = lookup(*hash)` branch). Direct unit
  test in `decompiler.test.mjs` covers known + unknown.

- **JS port: extra blank line between `// manifest not provided` and
  `// method tokens declared in NEF`.** When decompiling a NEF that
  has method tokens but no manifest, JS pushed an unconditional blank
  line after `// manifest not provided` then started the method-tokens
  block, while Rust ran the two comments flush. Removed the eager
  blank from the no-manifest branch and pushed a single trailing
  blank unconditionally at the end of the header (mirroring Rust's
  `writeln!(output)` at the end of `write_contract_header`).
  Differential test confirms byte-identical parity restored on the
  no-manifest + method-tokens path.

- **JS port: `sanitizeIdentifier` boolean precedence parity bug.**
  The JS port wrote `(character === "_" || /\s|-/.test(character))
  && !ident.endsWith("_")`, which silently collapsed leading
  consecutive underscores (`__foo` → `_foo`, `___bar` → `_bar`) and
  diverged from Rust's `decompiler::helpers::sanitize_identifier`
  (`_ || ((ws || dash) && !ends_with("_"))`). Rewrote the JS guard
  to mirror Rust precedence so explicit `_` is always preserved;
  whitespace and `-` still collapse into a single `_` separator
  when the previous char isn't already `_`. Regression test covers
  `__foo` / `___bar` / `_foo_` / `foo bar` / `foo  bar` / `foo--bar`
  / empty / digit-leading inputs.

### Changed

- **CLI `info` text format: surface manifest `Extra:` block.** The
  text-format `info` output already showed supported_standards, ABI
  counts, features, groups, permissions, and trusts — but dropped
  `manifest.extra` entirely. The JSON form had it (added in iteration
  80), so machine readers saw Author/Email/Description while humans
  running `neo-decompiler info <file>.nef` did not. Added an `Extra:`
  section listing string/number/boolean entries (objects/arrays/null
  skipped — the JSON surface still exposes the raw structure).
  Existing CLI smoke test extended to assert the new lines.

- **Both ports: render `manifest.groups` block in high-level
  summary.** Manifest groups (pubkey/signature pairs authorising
  signed contract updates) were dropped entirely from the high-level
  summary. The high-level format is meant to be a complete inspection
  view of the manifest, so missing groups was a real coverage gap.
  Both Rust (`high_level/render/manifest_summary.rs`) and JS
  (`high-level.js`) now emit a `groups { pubkey=02... }` block in the
  same shape as the `permissions {` block. The base64 signature is
  intentionally elided (opaque, no human-readable value). The C#
  emitter is unchanged — manifest groups have no idiomatic
  source-level expression in `Neo.SmartContract.Framework`; they're
  signed at deployment time, not declared in source.

- **Both ports: render non-string scalars (number, boolean) in
  `manifest.extra`.** The Rust high-level and C# emitters and the JS
  high-level emitter all gated `extra` rendering on
  `value.as_str()` / `typeof === "string"`, silently dropping
  numeric and boolean entries. Real-world manifests sometimes use
  `"Version": 2` or `"Verified": true`. Centralised the policy in a
  new `decompiler::helpers::render_extra_scalar` (Rust) and mirrored
  `renderExtraScalar` in JS: strings, numbers, and booleans render
  verbatim; null/objects/arrays still drop because they have no
  canonical short form. Regression tests in `csharp.rs` and
  `decompiler.test.mjs` lock in the expected emit shape.

- **Web API + CLI JSON: surfaced manifest `extra` metadata.** Both
  `src/cli/reports/manifest/model.rs` and `src/web/report.rs`'s
  `ManifestSummary` structs previously dropped the manifest's `extra`
  field, so the `info --format json` output and the browser/Web API
  decompile reports lost Author/Email/Description metadata that the C#
  emitter renders as `[ManifestExtra("...", "...")]` decorators.
  Both structs now carry an `Option<Value>` `extra` field
  (`skip_serializing_if = Option::is_none` so absent extras stay
  absent in the JSON), populated from `manifest.extra.clone()`. New
  Web-API regression test asserts Author/Email survive the JSON
  round-trip.

- **C# emitter: collapsed double blank line before inferred helper
  methods.** `write_inferred_methods` emitted a leading blank line
  before each helper signature, but the previous method (the synthetic
  `ScriptEntry` from `write_fallback_entry` or the last manifest
  method from `write_manifest_methods`) already terminates with its
  own trailing blank line. The two blanks compounded into a
  double-spaced separator between methods and the first inferred
  helper, and between consecutive inferred helpers. Dropping the
  redundant `writeln!(output)` lets each method own its trailing
  separator, matching the single-blank cadence used between manifest
  methods.

- **Differential test: aligned JS invocation with the Rust CLI default
  (clean mode).** `js/test/differential.test.mjs` runs the Rust binary
  with `decompile <nef>` (no flags → clean defaults: single-use temps
  inlined, no trace comments) and compared the output line-by-line
  against `decompileHighLevelBytesWithManifest(bytes, manifestJson)`.
  The JS call lacked `{ clean: true }`, so JS kept un-inlined temps
  while Rust inlined them — producing spurious "INFO: N lines differ"
  diagnostics on every artifact even when the lifted shapes were
  identical. Both calls now pass `{ clean: true }`; the seven
  testing-artifact NEFs render byte-identical between ports, and the
  test no longer logs noise for legitimate parity. (Also dropped an
  unused `basename` import the diagnostic surfaced.)

- **Persisted artifacts: regenerated with the CLI's clean defaults
  (single-use temps inlined, trace comments off).** The artifact
  pipeline previously used the bare `Decompiler::new()` (verbose:
  trace comments on, no inlining) so the committed `.csharp.cs` /
  `.high-level.cs` / `.pseudocode.txt` files in
  `TestingArtifacts/decompiled/` showed VM-debugger-style output
  rather than the production-ready human-readable rendering users
  actually get from `neo-decompiler decompile` or the Web API. The
  pipeline now constructs the decompiler with
  `.with_inline_single_use_temps(true).with_trace_comments(false)`
  so the committed sample outputs match what end users see. The
  library `Decompiler::new()` default is unchanged — downstream
  embedders that assert on the trace-form output continue to get
  the existing behaviour.

- **Persisted artifacts: regenerated set now includes the C# format
  (`<id>.csharp.cs`) alongside the existing `<id>.high-level.cs` and
  `<id>.pseudocode.txt`.** Previously the artifacts test computed
  `result.csharp` but discarded it, so users browsing the repo
  couldn't see what the C# rendering looked like for the testing
  fixtures. The C# pipeline now also gets a regression sentinel: any
  rewrite/lift change that affects the C# output shows up in the
  diff.

- **Web API: `WebDecompileOptions` defaults flipped to clean output.**
  Mirrors the CLI default flip described below. `inline_single_use_temps`
  now defaults to `true`, and a new `emit_trace_comments: bool` field
  defaults to `false`. Callers that want the un-inlined or
  trace-annotated form continue to opt in by setting the
  corresponding flags explicitly. WASM `JsDecompileOptions` honors
  the new defaults via a custom `Default` impl + `serde(default)`,
  so missing keys in the JS-side options object now produce clean
  output. New `web_decompile_report_emit_trace_comments_re_enables_per_instruction_comments`
  test asserts the opt-in path; the existing default test now also
  asserts trace comments are absent in default output.

- **CLI: default `decompile --format high-level` to clean output.**
  The lifted high-level view is now rendered without per-instruction
  `// XXXX: OPCODE` trace comments and with single-use temps inlined
  by default — the form a user actually wants to read as source.
  Added `--trace-comments` (opt back into trace noise for cross-
  referencing against bytecode) and `--no-inline-temps` (keep
  `let tN = ...` lines visible) opposite-flag escape hatches. The
  pre-flip flags `--no-trace-comments`, `--inline-single-use-temps`,
  and `--clean` are kept as hidden no-op aliases so existing
  scripts and CI configurations continue to work. Brings the Rust
  CLI in line with the JS port, which already defaulted to clean.

### Fixed

- **JS port: also sanitise method names and method parameter
  names in the ABI summary (parity with Rust).** Companion fix to
  the event-name sanitisation: a manifest method named
  `"do-thing"` with a parameter `"in-arg"` previously rendered as
  `fn do-thing(in-arg: int) -> void;` (uncompilable identifiers).
  Now sanitises to `fn do_thing(in_arg: int) -> void; // manifest
  "do-thing"`. The `manifest "..."` annotation is added when the
  sanitised name differs, and joins the existing `safe` /
  `offset N` meta with `, ` separators (matching Rust's
  `manifest_summary.rs`).

- **JS port: sanitise event names and parameter names with
  non-identifier characters (parity with Rust).** A manifest
  with an event named `"Trans-fer"` (hyphen) or a parameter
  named `"from-acc"` previously rendered as
  `event Trans-fer(from-acc: ...);` in the JS port —
  syntactically broken (hyphens aren't valid identifier chars).
  Rust's `sanitize_identifier` converts to `Trans_fer` and
  `from_acc` and emits a `// manifest "Trans-fer"` annotation
  when the sanitised form differs. JS now does the same. Most
  real-world manifests use plain identifiers so this rarely
  fires, but anomalous inputs no longer produce uncompilable
  output.

- **Both ports: handle `PublicKey` manifest type.** NEO N3
  manifests can specify a parameter as `PublicKey` (33-byte
  compressed ECDSA public key), but neither `format_manifest_type`
  (Rust) nor `formatManifestType` (JS) had a case for it — the
  type fell through to the unknown-kind path and rendered as the
  raw input verbatim. Added the missing arm: high-level
  `publickey` (lowercase, parallel to `hash160`/`hash256`/
  `signature`/etc.); C# `ECPoint` (Neo cryptography type from
  `Neo.Cryptography.ECC`). 1 new Rust unit-test assertion + 1
  new JS unit-test assertion in the existing
  `formatManifestType` parity tests.

- **C# emitter: extend `Exception(string)` coercion to
  `assert(cond, msg)` message too.** The throw/abort message path
  applied the `$"{value}"` interpolation wrap, but the assert
  message path was missing it — so `assert(x, code)` produced
  `throw new Exception(code)` (uncompilable if `code` is
  non-string). Plumbed `wrap_exception_operand_for_csharp` into
  the assert message rewrite. New regression test verifies the
  non-string-message wrap; the string-concat-message case is
  already covered.

- **C# emitter (refinement of throw/abort wrap): only wrap
  operands that don't already contain a `"`.** The original
  fix wrapped any non-self-contained-literal operand in
  `$"{value}"`, which made the common string-concat case
  (`abort("err" + code)`) come out as ugly
  `throw new Exception($"{"err" + code}")`. Now any operand
  containing a `"` somewhere is treated as string-typed and
  passes through unwrapped, leaving the cleaner
  `throw new Exception("err" + code);`. Numeric / identifier /
  helper-call operands (no `"`) still get the
  `$"{value}"` coercion. Test assertion updated to expect the
  cleaner form.

- **C# emitter: `throw(non-string)` now wraps the operand in
  `$"{value}"` so `Exception(string)` constructor is satisfied.**
  `throw(1);` previously emitted `throw new Exception(1);` —
  uncompilable because `System.Exception` has no `(int)` /
  `(BigInteger)` / etc. overload, only `(string)`. Now the
  rewriter detects whether the operand is a self-contained
  `"…"` string literal (already valid) and otherwise wraps it
  in C# string interpolation, which calls `ToString()`
  implicitly on any type. Same coercion applies to
  `abort(value);`. The four affected test assertions were
  updated to expect the wrapped form.

- **JS port: silently consume unstructured ENDFINALLY (parity
  with Rust).** Rust's `Endfinally` arm consumes the opcode (only
  emitting a verbose-mode trace comment when applicable). JS port
  had no special-case, so an unstructured ENDFINALLY (one not
  absorbed by a try-block lift) fell through to
  `renderUntranslatedInstruction` and produced
  `// XXXX: ENDFINALLY (not yet translated)` plus a structured
  warning entry. New explicit early-return in `executeStraightLine`:
  no statement, no warning. New regression test verifies both the
  rendered output and the warnings array stay clean.

- **C# emitter: synthetic `ScriptEntry` now exposes
  INITSLOT-declared arguments as parameters.** Companion to the
  preceding `void` → `object` return-type fix. The lifted body
  uses `arg0`, `arg1`, etc. from INITSLOT's argument count, but
  the synthetic signature was hardcoded to `()` (no params),
  leaving the body's identifiers unresolved and the C# uncompilable.
  Now the signature is `object ScriptEntry(object arg0, object arg1, ...)`
  with `object` widest-type fallback (the user can tighten to
  `BigInteger`/`UInt160`/etc. once the manifest is available).
  Both `write_fallback_entry` (no manifest) and
  `write_script_entry_if_needed` (manifest present but doesn't
  cover the bytecode entry) get the fix. New regression test
  exercises an INITSLOT-1-arg + ISNULL + NOT + RET script and
  asserts the parameter is declared and the body references it.

- **C# emitter: synthetic `ScriptEntry` now uses `object` return
  type when no manifest is provided.** Previously the fallback
  signature was hardcoded to `void` — which made the high-level
  emitter discard any value the bytecode's RET was returning.
  For a script as simple as `LDARG0; ISNULL; NOT; RET`, this
  produced `void ScriptEntry() { }` (empty body, lifted return
  silently dropped). Now the same script lifts to
  `object ScriptEntry() { return !(arg0 is null); }`. The
  user can tighten the return type once the manifest is
  available; `object` is the safe widest-type fallback. Two
  existing tests updated to assert the new signature.

- **Rust inliner + dead-temp elimination: allow inlining of known
  pure NEO helper calls.** Both `simplify::is_pure_rhs` (dead-temp
  pass) and `inline::single_use::util::is_safe_to_inline` (the
  inliner) used to reject any RHS containing `(` — too coarse,
  blocking inlining of pure math/buffer/type-check helpers
  (`abs`, `min`, `max`, `pow`, `modpow`, `modmul`, `within`,
  `sign`, `sqrt`, `left`, `right`, `substr`, `is_null`,
  `is_type_*`, `convert_to_*`, plus collection accessors
  `keys`/`values`/`has_key`/`len`). Now the predicate walks the
  expression and only rejects when a call site names something
  outside this whitelist (syscalls, internal/indirect/token-calls,
  manifest method names, etc.). For `let t0 = abs(x); return min(t0, y);`
  the temp is now inlined to `return min(abs(x), y);` — matching
  the JS port's stack-only emit and producing tighter source.
  Cross-port byte-identical parity preserved on all 7
  testing-artifact NEFs.

- **Rust trace comments: ENDFINALLY now renders as uppercase
  `// XXXX: ENDFINALLY` (matching every other opcode mnemonic).**
  Same fix pattern as the NOP fix below — the fallback note path
  used `note(instruction, "endfinally")` which produced lowercase
  output amid the surrounding uppercase trace comments. Switched
  to `push_comment(instruction)` for the standard mnemonic
  rendering. Only fires when ENDFINALLY isn't already absorbed
  by the structured try/finally lift.

- **Rust trace comments: NOP now renders as uppercase `// XXXX: NOP`
  (matching every other opcode mnemonic).** Previously NOP was
  special-cased through `note(instruction, "noop")` and produced
  `// XXXX: noop` lowercase, which clashed visually with all the
  `// XXXX: PUSH2`, `// XXXX: ADD`, etc. trace comments in
  verbose-mode output. Switch to `push_comment(instruction)` for
  the standard mnemonic. The persisted artifacts now show the
  consistent uppercase form.

- **JS port: surface `safe` annotation in ABI method declarations
  (parity with Rust).** Method declarations in the high-level
  header lifted from a manifest's `methods[].safe = true` flag now
  show ` // safe, offset N` (with `safe` first, matching the Rust
  port's `manifest_summary.rs` emit). Previously JS only showed
  `// offset N`, dropping the safety hint. New regression test
  verifies both the safe-method annotation and the no-annotation
  path for non-safe methods.

- **C# emitter: bug fix follow-up — helper rewrites now also
  apply inside `throw(...)`, `abort(...)`, and `assert(...)`
  operands.** Continuation of the `let` initialiser fix in the
  preceding entry. Same bug class — these branches extracted
  their operand text but didn't route it through
  `csharpize_expression`. So `throw(min(a, b));` came out as
  `throw new Exception(min(a, b));` (uncompilable). Now all four
  forms (`throw`, `abort`, `assert(cond)`, `assert(cond, msg)`)
  apply the expression rewriter to every operand. 4 new
  csharpize_statement test assertions cover throw / abort with
  cat-operator / assert with helper-call condition / assert with
  helper-call cond + cat-operator message.

- **C# emitter: bug fix — helper rewrites now apply inside `let`
  initialisers.** The `let X = expr;` → `var X = expr;` branch in
  `csharpize_statement` early-returned before
  `csharpize_expression` ran. So `let t0 = min(x, y);` came out as
  `var t0 = min(x, y);` (uncompilable; `min` doesn't exist in C#).
  Now the body runs through the expression rewriter — same fix
  pattern as iteration #41's if/while/etc. control-flow branches.
  Verified with: `BigInteger.Min(x, y)` (helper), `(loc0 is null)`
  (unary pattern), `a + b` (cat operator) — all of these now work
  inside `let` initialisers. 3 new csharpize_statement test
  assertions.

- **C# emitter: rewrite `append(arr, item)` and `has_key(c, k)`
  to `.Add(item)` / `.ContainsKey(k)`.** Continuation of the
  collection-helper rewrite work — these were the remaining
  unambiguous two-arg method calls. Both work for `List<T>` and
  Neo's `Map<TKey, TValue>` via standard collection interfaces.
  Refactored `match_unary_pattern`'s two-arg dispatch to a
  `METHOD_CALL_TABLE` so `remove_item`, `append`, and `has_key`
  share one driver. 3 new csharpize_statement test assertions.

- **C# emitter: rewrite NEO collection helpers to standard
  .NET / Neo Map / List APIs.** The high-level lift emits CLEARITEMS
  as `clear_items(c)`, REMOVE as `remove_item(c, k)`, KEYS as
  `keys(c)`, VALUES as `values(c)`, and REVERSEITEMS as
  `reverse_items(c)`. None compile in C#. Rewrite to `c.Clear()`,
  `c.Remove(k)`, `c.Keys`, `c.Values`, and `c.Reverse()` —
  standard collection-interface accessors that work for both
  `Map<TKey, TValue>` and `List<T>`. New `match_method_call`
  helper handles the receiver-as-first-arg shape (`prefix(arg0,
  arg1, ...)` → `arg0.Method(arg1, ...)`); `match_simple_unary`
  covers the others. 6 new csharpize_statement test assertions.

- **C# emitter: rewrite typed `convert_to_X(x)` / `is_type_X(x)`
  lifts to C# casts and pattern matches.** The high-level lift
  emits one form per NEO stack-item type (`convert_to_bool`,
  `convert_to_integer`, `convert_to_bytestring`,
  `convert_to_buffer`; same-shape `is_type_T` for ISTYPE). None
  are valid C# function names. Rewrite the safe subset to
  `(T)(x)` casts and `(x is T)` pattern matches: `bool`,
  `BigInteger`, `ByteString`, `byte[]`. The other variants
  (any, pointer, array, struct, map, interopinterface) need
  more context — left as the lifted identifier so the user gets
  a clear "fix this manually" signal. 10 new csharpize_statement
  test assertions across both rewrite tables.

- **C# emitter: rewrite size-operand constructors `new_buffer(n)`
  and `new_array(n)` to compilable `new byte[(int)(n)]` /
  `new object[(int)(n)]`.** NEWBUFFER and NEWARRAY (the
  size-operand variants) lifted to bare `new_buffer(n)` /
  `new_array(n)` calls — neither is a real C# function. Now both
  rewrite to explicit `new T[...]` allocations. The `(int)` cast on
  the size matches the int-typed length parameter required by C#
  array allocation. `new_struct(n)` (NEWSTRUCT-with-size) is
  intentionally not rewritten — Neo's `Struct` doesn't expose an
  N-element constructor, so silently rewriting would be wrong;
  the bare lifted form gives the user a clear "fix this manually"
  signal. New `match_simple_unary` helper allows extending the
  unary-pattern table by adding a `(needle, render-fn)` row;
  identifier-boundary aware. 4 new csharpize_statement test cases.

- **C# emitter: rewrite empty `Map()`, `[]`, and `Struct()`
  constructors to compilable C# `new` forms.** The high-level lift
  emits `let t0 = Map();` for NEWMAP, `let t0 = [];` for NEWARRAY0,
  and `let t0 = Struct();` for NEWSTRUCT0. None of these compile
  in C# (Map is a generic type that needs type params; `[]` is a
  collection literal without target type; `Struct` needs `new`).
  New `match_collection_constructor` pre-pass rewrites them to
  `new Map<object, object>()`, `new object[0]`, and
  `new Struct()` respectively. `object` defaults are conservative —
  without key/value type info from the lift, this is the safest
  C# fallback. Identifier-boundary aware (`MyMap()` is left alone)
  and string-aware (`"Map()"` literals stay verbatim).
  PACK / PACKMAP / PACKSTRUCT non-empty forms (`Map(k, v, ...)`)
  are deferred — they need collection-initialiser rendering. 5 new
  csharpize_statement test assertions cover the rewrite paths and
  boundary cases.

- **JS port: emit a structured warning when encountering an
  untranslated opcode.** The fall-through path in `executeStraightLine`
  pushed an inline `// XXXX: OPCODE (not yet translated)` line but
  left the structured `warnings` array silent — only Rust's
  `self.warn(...)` did both. Closes the third structured-warning
  parity gap (call-arg / unknown-syscall / untranslated-opcode).
  New regression test verifies both the inline rendering and the
  warnings entry; cross-port byte-identical parity preserved on
  all 7 testing-artifact NEFs.

- **Rust: preserve original case for unknown manifest ABI types.**
  `format_manifest_type` in the high-level emitter previously
  lowercased the input string for any type outside the standard
  Neo vocabulary (Void / Boolean / Integer / String / Hash160 /
  Hash256 / ByteArray / Signature / Array / Map / InteropInterface /
  Any). So `MyCustomType` came out as `mycustomtype`. JS's
  `formatManifestType` preserves the original via `String(kind)`.
  Match the JS behaviour — return `kind.to_string()` for the
  fallback so the user's chosen casing survives. New unit tests in
  `helpers::types::tests` cover the known-kind normalisation and
  unknown-kind preservation paths.

- **Rust: push a structured warning when emitting an unknown
  syscall.** The unknown-syscall path emitted only an inline
  `// unknown syscall` trailing comment — the structured `warnings`
  array stayed silent, which made the hazard invisible to
  programmatic callers (CI, IDE integration). Now both ports
  surface unknown syscalls through both channels. Existing
  `decompile_unknown_syscall_keeps_unknown_annotation` test
  extended to also assert the warnings-array entry.

- **JS port: replaced all remaining `/* stack_underflow */`
  placeholders with `???`.** Follow-up to the call-arg
  unification: STLOC, STARG, STSFLD, DUP / OVER on an empty stack,
  and binary operator missing operands now all use `???` for the
  fallback rendering. The previous form was a C-style block
  comment in expression position — awkward, inconsistent with the
  syscall and call paths' `???` convention. 8 sites updated; the
  call-site path's structured warning emission is unchanged.

- **JS port: unify missing call-arg placeholder to `???` (drop the
  `/* stack_underflow */` comment form) and emit a structured
  warning.** The internal/indirect/token call paths previously
  substituted a `/* stack_underflow */` block-comment when the
  stack underflowed — awkward in argument position and inconsistent
  with the syscall path's `???` marker. Both forms now emit `???`
  and push a `missing call argument values for {callee}
  (substituted ???)` entry to both the inline `// warning:` line
  and the structured `warnings` array, so a programmatic caller
  iterating warnings sees the hazard. New regression test verifies
  the placeholder, the warnings entry, and the absence of the old
  C-comment marker.

- **Both ports: strip `// rotate top three stack values`,
  `// tuck top of stack`, `// reverse top N stack values`, and
  `// clear stack` VM-narration comments from output.** These
  describe the VM-internal stack rearrangement that ROT, TUCK,
  REVERSE3/REVERSE4/REVERSEN, and CLEAR perform — but the actual
  data flow is already captured in the subsequent variable
  references. The annotation just adds noise to the lifted source.
  Extended both Rust's `strip_stack_comments` and JS's
  `stripStackComments` with the same prefix matchers used for the
  existing `// drop ...`, `// xdrop stack ...`, and `// swapped top`
  patterns. 7 Rust tests and 5 JS tests that asserted the
  presence of these comments were rewritten to assert the
  substantive lift outcome instead (e.g. `return 1;` after
  REVERSE3 of `[1,2,3]`).

- **JS port: strip `// xdrop stack[...]` annotations from clean output.**
  XDROP lifts emit a `// xdrop stack[N] (removed X)` annotation
  documenting which stack item the VM dropped. The Rust port has
  always stripped these; the JS port was leaking them through.
  Extended `stripStackComments` to drop `// xdrop stack` lines too,
  closing the parity gap. The corresponding test was updated to
  assert the comment's absence (the meaningful "no `unsupported
  dynamic XDROP` placeholder" check stays).

- **JS port: cleanup — `trySlotDeclarations` no longer emits the
  `// declare ...` comment that always got stripped.** Follow-up to
  the previous fix: removed the always-emit-then-strip round-trip
  and deleted the now-unused `stripSlotDeclarationComments` pass
  entirely, since nothing else emits that pattern.

- **JS port: drop `// declare N locals, M arguments` comment by default.**
  The JS port has no verbose-mode opt-in (the always-on lift is the
  Rust port's clean mode), but it was still emitting the
  informational `// declare 1 locals, 0 arguments` line from
  INITSLOT plus the `// declare N static slots` line from INITSSLOT.
  These read as noise alongside lifted source and create a
  cross-port parity gap (Rust strips them in clean mode). Move
  `stripSlotDeclarationComments` out of the `options.clean` gate so
  it runs unconditionally. Two existing JS tests that asserted the
  comment's presence have been updated to assert it's absent (and
  the lifted store/load behaviour they really cared about is still
  verified). Cross-port parity on all 7 testing-artifact NEFs is
  byte-identical again.

- **C# header: typed multi-entry trusts list renders as a `// trusts:`
  block.** Previously a contract with several entries in a structured
  `{"hashes": [...], "groups": [...]}` trust value rendered as
  `// trusts = [hash:0x..., hash:0x..., group:02..., group:02..., ...]`
  — a single line that grows unboundedly. The C# header now breaks
  multi-entry typed trusts onto their own lines (parallel to the
  existing `// permissions:` block) so a real contract with many
  trust entries reads naturally. Single-entry forms and the special
  `*` / `[]` values stay on one line — they have no internal
  structure worth breaking out. 2 new csharp tests cover both the
  multi-line and single-line paths.

- **C# emitter: rewrite `is_null(x)` to the idiomatic `(x is null)` pattern.**
  The high-level lift emits NEO's ISNULL opcode as `is_null(x)`. There
  is no top-level `is_null` function in C#, so the call failed to
  resolve. The idiomatic .NET form is the pattern match `x is null`,
  which works against any reference-typed argument including the NEO
  runtime stack types. Identifier-boundary aware so user-defined
  `my_is_null(x)` is left alone. Side fix: control-flow branches
  (`if`/`while`/`else if`/`for`/`switch`) now run their condition
  through the same expression rewriter as statement bodies, so a
  helper call inside `if abs(x) > 0 {` (or `if is_null(loc0) {`)
  also gets rewritten — previously the early-return for the
  control-flow header skipped the rewrite pass entirely. 4 new
  csharpize_statement test assertions.

- **C# emitter: extended helper rewrites to `sign/sqrt/modmul/modpow/within/left/right/substr`.**
  Follow-up to the previous `abs/min/max/pow` rewrite. The high-level
  lift emits each of these as a bare function call too, none of which
  exist as top-level functions in C#. We rewrite to `BigInteger.ModPow`
  for ModPow (pure .NET) and `Helper.X` for the rest (Neo
  `SmartContract.Framework.Helper` is in scope via the preamble).
  `Left`/`Right` need their `n` arg cast to `int`; `Substr` needs both
  `start` and `length` cast. Generalised the rewrite engine to support
  any `int`-cast position via a `&[usize]` mask, so future helper
  additions just append a row to the table. 8 new csharpize_statement
  test assertions cover every entry in the new table.

- **C# emitter: rewrite `abs/min/max/pow(...)` to `BigInteger.X(...)`
  forms.** The high-level lift renders the corresponding NEO opcodes
  (`ABS`, `MIN`, `MAX`, `POW`) as bare function calls, but C# has no
  top-level `abs/min/max/pow` in scope, so the emitted file failed
  to compile against the standard NEO SmartContract Framework.
  `using System.Numerics;` is already in the preamble, so the
  rewrite targets `BigInteger.Abs`, `BigInteger.Min`, `BigInteger.Max`,
  and `BigInteger.Pow`. `BigInteger.Pow`'s second parameter is `int`
  (not `BigInteger`), so the rewrite wraps the exponent in `(int)(...)`
  to keep the call type-correct. Identifier-boundary aware
  (`mypow(x)` is left alone) and string-aware (`"min(a, b)"` inside a
  literal is preserved verbatim). 7 new csharpize_statement test
  cases (basic, pow with int cast, identifier boundary, string
  preservation, nesting).

- **Manifest: structured `trusts` object flattens to a typed list.**
  An N3 manifest is allowed to specify `trusts` as
  `{"hashes": [...], "groups": [...]}` for "trust this set of
  contract hashes / public-key groups". Previously the high-level
  view rendered that verbatim as JSON (`trusts = {"groups":["02ab.."]};`),
  which is awkward when sitting next to the existing
  `permissions { contract=hash:... contract=group:... }` block.
  Both ports now flatten to a typed list (`trusts = [hash:0x..., group:02...];`)
  matching the permission-contract format. Anomalous shapes
  (unexpected keys, non-string array entries) still fall back to
  raw JSON so data is never silently dropped. 7 new Rust unit tests
  in `manifest::describe::tests`; 2 new JS regression tests in
  `decompiler.test.mjs`.

- **JS port: hoist DUP'd / OVER'd call-result expressions into temps
  before duplicating.** When a side-effecting expression like
  `syscall("System.Storage.GetContext")` was DUP'd and consumed twice,
  each consumer received an independent copy of the syscall string —
  the lifted output then contained the syscall twice, which
  observably re-runs the side effect. `materialiseStackTopForDup`
  detects non-trivial stack tops (anything that isn't a plain
  identifier, integer, hex, bool, null, or quoted string) and emits a
  one-line `let tN = <expr>;` so the DUP / OVER push references the
  temp instead. Verified on a real deposit-style script (one
  `GetContext` call, two consumers; both now read from the same
  `t0`).

- **C# emitter: translate `assert(cond);` / `assert(cond, msg);` to
  compilable `if (!(cond)) throw new Exception(...);`.** C# has no
  built-in `assert` function in scope, so the previous output failed
  to compile. The new form is universal — no helper imports required
  — and reads naturally as a runtime guard. The comma-split helper is
  paren / bracket / string aware so `assert(foo(a, b));` correctly
  treats the inner `,` as part of the condition rather than an
  argument separator.

- **C# emitter: convert `abort();` / `abort(msg);` to
  `throw new Exception(...);`.** Same rationale as the previous
  `throw(value);` → `throw new Exception(value);` change. NEO's ABORT
  is uncatchable but `throw new Exception(...)` is the closest C#
  analogue and reads naturally for a post-decompile reader. 3 new
  csharpize_statement test cases (bare, literal-string message,
  identifier message).

- **JS port: surface `// warning: unknown syscall 0xHASH` for
  unrecognised syscall hashes.** The Rust port already annotated the
  lifted line with `// unknown syscall`; JS rendered a bare
  `syscall(0xHASH)` with no hint that the hash wasn't in the bundled
  table. The user now gets the same heads-up in both ports, plus a
  matching entry in the structured `warnings` array.

- **Rust: emit `MIN` / `MAX`, `LEFT` / `RIGHT`, and `POW` as function
  calls for parity with JS.** The Rust port was rendering
  these as pseudo-operator forms (`"hello" left 3`) — a NEO bytecode
  visualisation convention that doesn't lower into either Rust or C#
  source. The JS port already used the function-call form. Both ports
  now agree, and the output is a one-step rewrite away from the NEO
  devpack helpers (`Helper.Left(s, n)` / `Helper.Right(s, n)`).

- **JS port: `NEWARRAY` / `NEWBUFFER` / `NEWSTRUCT` / `NEWARRAY_T`
  materialise into a temp before any DUP / mutation.** Same shape as
  the previous NEWMAP / NEWARRAY0 / NEWSTRUCT0 fix: pushing the
  `new_array(size)` / `new_buffer(size)` / `new_struct(value)` call
  expression onto the operand stack meant DUP duplicated the *string*,
  so a `PUSH3 NEWARRAY DUP PUSH0 PUSH8 SETITEM RET` lift rendered as
  `new_array(3)[0] = 8; return new_array(3);` (two independent
  allocations). Each constructor now emits
  `let tN = new_array(...);` (and friends) as a statement and pushes
  the temp identifier, so DUP'd references all resolve to the same
  allocation and the mutation lands on the same object.

- **Collapse `((expr))` double parens after single-use-temp inlining.**
  The Rust inliner unconditionally wraps multi-token substitutions in
  parens for precedence safety; when the substitution lands inside an
  already-parenthesised context (e.g. an `assert((x > 0))` call
  argument or `if (((cond)))`), the result was doubly-parenthesised.
  New `reduce_double_parens` postprocess pass strips matched
  back-to-back redundant pairs while preserving the precedence-safe
  outer parens. Function-call argument parens around *different*
  operands (e.g. `foo((x), (y))`) are correctly left alone.

- **C# emitter: `throw(value);` becomes `throw new Exception(value);`.**
  The high-level emitter renders NEO's `THROW` opcode as
  `throw(value);` (NEO can throw any stack value); C# requires an
  `Exception` subtype, so the previous output failed to compile. The
  C# emitter now wraps the operand in `new Exception(...)`. Two new
  csharpize_statement tests pin literal-string and identifier
  payloads.

- **JS port: don't pre-populate the operand stack with arg labels when
  the method begins with `INITSLOT`.** Without a manifest, JS state
  init pre-populates the stack with `arg0..argN` (one per inferred
  arg) so subsequent opcodes have something to consume. But INITSLOT
  itself pops args off the stack into the arg slots, so for a script
  that begins with INITSLOT the pre-population creates a phantom
  `arg0` that never gets consumed and surfaces as a bare-expression
  statement after RET — e.g. an `if arg0 <= 10 { return 1; arg0; }`
  body where the trailing `arg0;` is unreachable junk. Now the
  pre-population only fires when the program does NOT start with
  INITSLOT (mirrors Rust's `set_argument_labels` guard). New
  regression test pins the if-else lift shape.

- **Rust: strip residual `// swapped top two stack values` and
  `// xdrop stack[N] (removed M)` comments from clean output.**
  `strip_stack_comments` already cleared `// drop ...`,
  `// remove second ...`, and trailing `// duplicate top of stack` /
  `// copy second stack value` annotations, but two more SWAP-/XDROP-
  specific patterns were leaking through. Both are pure VM-bookkeeping
  notes the surrounding lifted statements already convey. Now stripped
  unconditionally so JS and Rust outputs no longer differ on these.
  One existing xdrop test updated to assert the comment is gone (the
  return-value assertion still pins the post-XDROP stack shape).

- **JS port: decode `PUSHINT128` / `PUSHINT256` to decimal literals.**
  These two opcodes carry a 16- / 32-byte little-endian
  two's-complement integer payload, which the JS lift was treating as
  an unhandled opcode (rendered as
  `// 0000: PUSHINT128 0x... (not yet translated)`). New
  `decodeSignedLeBigInt` helper in `high-level-slots.js` mirrors the
  Rust `format_int_bytes_as_decimal` math; output now matches Rust
  byte-for-byte (`return 291;` for `0x123`, `return -1;` for the
  all-0xff 32-byte payload). Two new regression tests pin the positive
  and negative branches.

### Changed

- **CLI: `--format` and `--output-format` now both accept `csharp`.**
  The two flags previously used different spellings for the same C#
  output (`--format csharp` vs `--output-format c-sharp`). Both now
  accept `csharp`, and the legacy `c-sharp` form stays as an alias for
  back-compat with any scripts that pinned the old spelling. New
  `decompile_output_format_accepts_csharp_and_legacy_alias` smoke test
  pins both spellings.

### Fixed

- **Re-run fallthrough-goto + orphan-label elimination after the
  inliner / dead-temp passes** in both ports. The inliner can collapse
  the body that was previously sitting between a `leave/goto LABEL;`
  and its `LABEL:` target (e.g. `let t1 = 2;` followed by
  `label_X: return t1;` becomes `(blank) label_X: return 2;`). The
  initial elimination pass bails on intervening code, and without a
  re-run the now-eliminable transfer plus its orphan label both stick
  around. Try-catch lifts that previously rendered as
  ```text
  catch { leave label_X; let t1 = 2; label_X: return t1; }
  ```
  now collapse cleanly to `catch { return 2; }`.

- **C# emitter: translate the `cat` (CAT) operator to `+`.** The
  high-level pseudo-language uses `a cat b` for string concatenation
  (lifted from NEO's `CAT` opcode); C# uses `+`. The C# emitter was
  passing `cat` through verbatim, so any output that used CAT —
  e.g. a storage-key lookup like `syscall("System.Storage.Get", ctx,
  "b:" cat addr)` — was invalid C#. The translation now fires for
  every ` cat ` token outside string literals; string contents like
  `"says cat ok"` are preserved.

- **JS port: equality / not-equal use `==` / `!=` instead of
  JavaScript's `===` / `!==`.** Lifted output now matches the Rust
  port's choice and lowers cleanly into both Rust and C# without
  further rewriting. The change covers both the binary-op operator
  table (EQUAL / NOTEQUAL / NUMEQUAL / NUMNOTEQUAL) and the comparison
  jumps (`JMPEQ` / `JMPNE` / `_L` variants) negated/original tables.
  Two affected tests updated.

- **JS port: string and `0x...` hex literal operands no longer get
  redundant outer parens.** Bare literals like `"key"` and `0xDEADBEEF`
  were missing from `wrapExpression`'s atomic-value regex, so they
  came out as `("key") + suffix` and `(0xDEADBEEF) + 1` whenever they
  appeared as a binary-op operand. Both shapes are now recognised as
  self-contained and skipped. Combined with the earlier
  self-contained-call check, expressions like
  `"Hello, " cat "World!"` and `syscall("Storage.Get", ctx, "key")`
  render in their natural form.

- **JS port: NEWMAP / NEWARRAY0 / NEWSTRUCT0 now materialise into a
  temp instead of pushing a literal `{}`/`[]` onto the stack.** The
  literal-on-stack model was broken under DUP: a script like
  `NEWMAP DUP "k" "v" SETITEM RET` lifted as
  `{}["k"] = "v"; return {};` — two *separate* empty maps where the
  bytecode created one and mutated it. Rendering now emits
  `let t0 = {}; t0["k"] = "v"; return t0;`, so DUP'd references all
  resolve to the same identifier and helper calls (`append`,
  `pop_item`, `keys`, `values`, `len`, `has_key`, `remove_item`,
  `clear_items`, `reverse_items`, `[idx]` reads, `[idx] = ...` writes)
  receive the materialised temp rather than a fresh literal. Eight
  affected tests updated; cross-port testing-artifact parity
  preserved.

- **Rust: surface INITSLOT-declared args in the synthesised
  `script_entry` signature.** Without a manifest, Rust was rendering
  the entry method as bare `fn script_entry()` even when the lifted
  body referenced `arg0` (e.g. for an INITSLOT 2/1 prologue). The
  signature now lists `arg0..argN` so the parameter the body uses is
  actually declared. Only `INITSLOT`-declared arg counts are surfaced
  — purely stack-depth-inferred args (where the body consumes a value
  no opcode pushed) keep the bare signature so the
  `missing syscall argument` warning remains visible. Matches the JS
  port's existing behaviour and keeps testing-artifact cross-port
  parity.
- **JS port: drop redundant parens around self-contained call
  operands.** `wrapExpression` was wrapping any non-primitive operand
  in `(...)`, so `1 + sub_0x0006()` came out as
  `1 + (sub_0x0006())` and `syscall("Storage.Get", ctx, "k")` ended up
  as `(syscall("Storage.Get", ctx, "k"))` whenever it appeared as a
  binary-op operand. Added an `isSelfContainedCall` check that
  recognises the `identifier(...)`-with-balanced-parens shape (closing
  paren is the last char, no top-level paren re-open) and skips the
  wrap. Operand expressions that genuinely need parens (e.g.
  `(a + b) * c`) still get them.
- **JS port: manifest-less header now matches Rust (`NeoContract`
  default + `// manifest not provided`).** When invoked without a
  manifest, JS was emitting `contract Contract { ... }` with no
  manifest-absence comment, while Rust emits `contract NeoContract {
  ... // manifest not provided\n\n }`. The two ports now produce
  byte-identical headers in this branch (same default name, same
  comment, same trailing blank line). Affects every manifest-less
  decompile through the JS API or web demo.
- **JS try / try-catch / try-finally lifts now propagate the operand
  stack across the try boundary.** The structured try lifter cloned
  the resume state from the *prefix* state instead of from whichever
  branch had just finished, so any value the try / catch / finally
  body left on the stack (the conventional way contracts hand a result
  back through `RET` after exception handling) was dropped, and a
  trailing `return X;` reverted to whatever the prefix had pushed —
  often `return;` or the wrong value. Resume state now clones from
  `finallyState` if a finally ran, otherwise `tryBodyState`, otherwise
  `prefixState`, so e.g. `PUSH1 TRY { PUSH2 } finally { PUSH3 } RET`
  correctly lifts as `try {} finally {} return 3;`. Combined with the
  separate fix to slice the resume past `ENDFINALLY` (rather than
  re-running the finally bytes), the spurious `// XXXX: ENDFINALLY
  (not yet translated)` artifact is gone too.

- **`eliminate_fallthrough_gotos` now sees through close-brace runs.**
  Previously the pass only stripped a `goto/leave LABEL;` whose target
  appeared on the very next code line. When the transfer was the last
  statement of a block (commonly a catch body) and the closing `}` was
  immediately followed by `LABEL:`, the pass missed it and the dead
  `leave label_0xNNNN;` plus its orphaned label both stuck around. The
  pass now walks past blank/comment lines AND `}` lines while looking
  for the target label, so a try-catch like
  ```text
  try { } catch { leave label_0x0009; } label_0x0009:
  ```
  collapses cleanly to `try { } catch { }`. If any executable statement
  appears between the closing brace and the label, the transfer is
  preserved (eliminating it would skip that code and change semantics).
  Mirrored in both Rust (`while_loops.rs`) and JS (`postprocess.js`); 2
  new Rust unit tests + the existing 967/969-test suites still green.

### Added

- **JS port: PUSHDATA byte payloads decode as quoted strings when
  printable.** Mirrors the Rust `format_pushdata` helper: bytes whose
  every code unit is printable ASCII (0x20..=0x7E) or common whitespace
  (\\n, \\r, \\t) render as `"keystring"` instead of the raw `0x6B6579…`
  hex. Binary / non-printable payloads keep the unambiguous hex form. A
  storage-key access like
  `syscall("System.Storage.Get", ctx, "balance")` now reads the same in
  both ports. Two affected `assert*`/`abort*` tests updated to expect
  the quoted form for the ASCII bytes 0x41 / 0x42; two new regression
  tests pin both branches.
- **JS port: contract-header parity with Rust.** Three gaps closed:
  1. ABI method declarations now show the explicit return type for void
     methods (`fn main() -> void; // offset 0`) so the manifest contract
     surface matches the Rust output exactly.
  2. New `// method tokens declared in NEF` section reproduces Rust's
     per-token line — method name, native-contract label
     (`Serialize (StdLib::Serialize)`), 20-byte hash, parameter count,
     return flag, and `|`-joined call-flag breakdown
     (`flags=0x0F (ReadStates|WriteStates|AllowCall|AllowNotify)`).
     Required porting `src/native_contracts*.rs` to
     `js/src/native-contracts.js` (script-hash → contract-name lookup
     with `formattedLabel(method)` and `hasExactMethod()` helpers) plus
     adding `describeCallFlags` to `js/src/nef.js`.
  3. Methods bodies are now separated by a blank line — matches the
     Rust renderer's `writeln!(output)` between definitions. Previously
     JS emitted `}\nfn next ...` with no separator.

  All seven testing-artifact NEFs (LoopIf, MethodToken, Events,
  MetaEvent, MultiMethod, Permissions, EmbeddedSample) now produce
  byte-identical high-level output between JS and Rust under
  `--clean`.

### Fixed

- **JS port: drop inferred helpers whose body decodes to nothing.** The
  post-terminator method-detection heuristic (added in v1.3.0) was
  treating runs of NOP padding between manifest methods as standalone
  helpers and rendering `fn sub_0xNNNN() { // no instructions decoded
  }`. The Rust port already skips these; the JS port now does the same.
  Manifest-declared methods that genuinely lift to nothing still emit
  the placeholder (so the user can see the ABI is honoured). New
  regression test in `js/test/decompiler.test.mjs`.
- **JS port: bring `eliminateFallthroughGotos` to parity with Rust.** The
  pass had two latent bugs that combined to make it a near-silent no-op:
  (1) it only matched `goto label_X;`, missing the try-context `leave
  label_X;` form lifted from `ENDTRY`; (2) it called `nextCodeLine` with
  the goto's own index rather than `index + 1`, so the "next code line"
  search returned the goto itself, the equality check failed, and no
  elimination happened. Output was littered with `goto label_X;
  label_X:` and `leave label_X; label_X:` pairs that the C# / Rust
  backends would render as identical control flow either way. Two
  affected tests (`uses label-style gotos for generic jump fallbacks`,
  `uses label-style leave fallbacks for ENDTRY transfers`) updated to
  assert the cleaner post-elimination output, matching the Rust-side
  changes from earlier in this Unreleased section.
- **Suppress the redundant syscall-hash trailing comment in clean mode.**
  Known syscalls were rendering as `syscall("System.Runtime.GetTime"); //
  0x0388C3B7` — the name in the call already identifies the call, so the
  trailing 32-bit hash adds nothing and clutters output even after
  `--clean`. The hash comment now only appears when trace comments are
  on (default / non-clean). Side benefit: the inline-temp pass can now
  collapse `let tN = syscall(...); ... let locK = tN;` into a single
  `let locK = syscall(...);` because the trailing comment is no longer
  blocking the inliner. Unknown-syscall annotations (`syscall(0x…); //
  unknown syscall`) are kept in every mode — they are the only signal
  that a raw hash in the output is intentional.
- **Drop the redundant trailing `return;` from C# void method bodies.**
  Every NEO method ends with `RET`, which the high-level emitter lifts
  as `return;`. C# void methods auto-emit a return at the end of the
  body, so the explicit one was pure clutter — `public static void X()
  { ...; return; }` instead of the natural `public static void X()
  { ... }`. The C# body emitter now strips the final bare `return;`
  when `returns_void` is set; non-void methods keep their `return X;`,
  and any non-trailing `return;` (e.g. inside an `if`) is left intact.
- **Eliminate dead temps whose RHS is a pure arithmetic / comparison /
  logical expression.** `eliminate_dead_temps` previously only removed
  unused `let tN = <literal-or-ident>;` lines, so an unused temp such as
  `let t1 = loc1 * 3;` survived all the way to the C# emitter — appearing
  as `var t1 = loc1 * 3; return;` in the body of a void method. The
  pass now considers any expression without `(`, `/`, `%`, or `[` to be
  observably pure (NEO `ADD`/`SUB`/`MUL`/`AND`/`OR`/`XOR`/`SHL`/`SHR`/
  comparison ops never throw and have no side effects), while still
  retaining temps for `DIV`/`MOD`/`PICKITEM` and any function/syscall
  call so divide-by-zero or out-of-bounds exceptions are not silently
  hidden.
- **Strip dead `leave label_X;` transfers when the target is the next
  line.** `eliminate_fallthrough_gotos` already collapsed `goto label_X;
  label_X:` pairs but did not recognise the high-level `leave` form
  (lifted from `ENDTRY`). Try / catch / finally lifts that resumed at the
  block-immediately-following instruction therefore left dead transfer
  plumbing — `leave label_0x0006; label_0x0006:` — visible in both
  high-level and C# output. The pass now also matches `leave`, and the
  orphaned-label pass strips the resulting unreferenced anchors.
- **Rust C# emitter: produce compilable C# for `loop`, `else if`, and
  `switch` constructs**. The high-level lift produces pseudocode tokens
  (`loop {`, `else if cond {`, `switch X { case Y { ... } default { ... } }`)
  that previously passed through `csharpize_statement` unchanged, leaving
  invalid C# in the `--format csharp` output. The emitter now rewrites:
  - `loop {` → `while (true) {`
  - `else if cond {` and `} else if cond {` → parenthesised C# form
  - `switch X { ... case Y { ... } default { ... } }` →
    `switch (X) { ... case Y: { ...; break; } default: { ...; break; } }`,
    with an explicit `break;` synthesised before each case-body close
    (skipped when the case already ends in `return`/`throw`/`goto`/
    `break`/`continue`).

### Removed

- Dropped the orphaned `src/decompiler/tests/golden/` snapshot directory
  (`csharp_for_loop.cs`, `csharp_try_finally.cs`,
  `high_level_for_loop.txt`, `high_level_try_finally.txt`) — no test
  consumed these fixtures, and they froze long-fixed indentation /
  scoping bugs into a misleading "expected" reference.

## [0.6.3] - 2026-04-26 (Rust) / [1.3.0] - 2026-04-26 (JS)

This release closes the last known behavior asymmetry between the Rust
and JavaScript implementations and bundles a comprehensive performance
and correctness pass on both sides. The two implementations are now
verified behavior-equivalent across 967 JS unit tests, 316 Rust lib
tests, and 4030 cross-implementation differential and corpus-replay
cases.

### Fixed

- **Rust: orphaned-label removal pass**: New `remove_orphaned_labels`
  postprocess pass strips `label_0xXXXX:` lines whose only `goto` /
  `leave` / `if {goto}` references were optimised away by the
  fallthrough-goto pass. Previously these labels remained as visible
  noise; the JS port already had this pass.
- **JS: ~17 silent manifest validation divergences**: `parseManifest`
  now rejects the same set of malformed manifests that Rust's serde
  rejects, instead of silently defaulting fields, coercing types, or
  storing raw values. Required-field violations throw
  `ManifestParseError` with `code: "MissingField"`; type violations
  use `code: "InvalidType"`. Specifically:
  - Required: top-level `name`; `abi.methods[i].name`, `returntype`;
    `parameters[j].name`, `type`; `abi.events[i].name`,
    `parameters[j].name`, `type`; `groups[i].pubkey`, `signature`;
    `permissions[i].contract`.
  - Type-strict: `abi` must be an object; `features.storage` /
    `payable` must be boolean (was `Boolean(...)` coercion);
    `supportedstandards` must be an array;
    `groups`/`permissions`/`abi.methods`/`abi.events`/`parameters`
    must be arrays when present; `method.offset` must be a number;
    `method.safe` must be a boolean; permission entries must be
    objects.
  - Removed the JS-specific `parameter.kind` fallback shim. Inputs
    using `kind` instead of the spec-required `type` now fail
    consistently in both implementations.

### Added

- **JS: `parseManifest` strict mode**: Optional second argument
  `parseManifest(json, { strict: true })` validates canonical wildcard
  values (`"*"`) in `permissions` and `trusts`, matching Rust's
  `from_json_str_strict` semantics.
- **JS: manifest size limit enforcement**: When input is a string,
  `parseManifest` now enforces `MAX_MANIFEST_SIZE = 0xFFFF`, matching
  Rust's `from_bytes` behaviour.
- **Differential fuzzing harness**: New `js/test/differential-fuzz.mjs`
  generates random NEFs covering all 15 operand encoding kinds and
  runs 21 hand-crafted edge-case probes (empty PUSHDATA,
  PUSHINT128/256, TRY variants, deep nesting, etc.) against both
  implementations. 200/200 random + 21/21 edge cases agree.
- **Corpus-replay harnesses**: New `js/test/corpus-replay.mjs` and
  `js/test/manifest-corpus-replay.mjs` feed every input from the saved
  fuzz corpora through both implementations. 414/414 NEF/decompile
  inputs and 3416/3416 manifest inputs produce matching outcomes.

### Changed (performance)

- **JS: comprehensive hot-path optimisations** (averaged over 3 runs):
  - Disassembly 10KB: `0.222ms → 0.135ms` (-39%)
  - Full-analysis pipeline: `1.74ms → 1.20ms` (-31%)
  - Syscall-heavy contract: `0.116ms → 0.062ms` (-47%)
  - 10KB contract end-to-end: `3.27ms → 2.60ms` (-21%)
  - 10000-iteration stress: `154ms → 116ms` (-25%)

  Highlights: replaced per-call `DataView` allocations with bit-op
  reads; cached PUSH-immediate operand singletons; replaced
  `Int8Array(...)[0]` with bit-math sign extension; switched
  `renderPseudocode` from `+=` cons-string accumulation to
  array-push + `join("\n")`; added `hex8`/`hex16`/`hex32` helpers
  using a 256-entry lookup table and applied them at every hot
  `toString(16).padStart(...)` site; converted chained
  `if (mnemonic === ...)` cascades to `switch` in
  `tryControlStatement`, `tryStackShapeOperation`,
  `tryUnaryExpression`, `tryBinaryExpression`, and `inferTypes`;
  replaced regex-based `slotIndexFromMnemonic` with prefix-check +
  char-code digit parse (called 6×/instruction); fixed
  `tryLiftSimpleSwitch` O(n²) per-case scan via offset→index Map;
  eliminated 4 `slice().findIndex()` patterns in
  `tryLiftSimpleTryBlock`; dropped redundant recursion in postprocess
  `rewriteExpr` (O(N²) → O(N)); single-pass temp scan in
  `collectInlineCandidates` (O(K·N) → O(N)); lazy
  `state.programIndexByOffset` cache in `inferUnpackElementCount`;
  in-place `REVERSE3`/`REVERSE4`/`REVERSEN` swaps; eliminated
  `scriptHashLE` slice-then-reverse copy.
- **Rust: O(log n) lookups in `MethodTable`**: New `largest_le`
  helper using `partition_point` replaces `filter().max()` linear
  scans in the CallA fixpoint loop and
  `resolve_argument_target_for_method`. `resolve_internal_target`
  switched from linear `iter().find()` to `binary_search_by_key`.
  `callers_by_target` switched from `BTreeMap<usize, Vec<usize>>`
  to `BTreeMap<usize, BTreeSet<usize>>`, eliminating the redundant
  `Vec::contains` check on each fixpoint iteration. For a contract
  with K method starts and N CallA instructions, fixpoint complexity
  drops from O(K·N) to O(N·log K) per iteration.
- **Rust: postprocess scan-loop clean-up**: Replaced
  `iter().enumerate().skip(start)` with direct `(start..len)` range
  loops in `next_code_line`, `find_matching_brace`, and
  `find_matching_close`. The `.skip()` adapter on slice iterators is
  not always specialised to O(1), so each call previously paid an
  O(start) startup tax.

## [1.2.1] - 2026-04-08 (JS only)

### Changed

- **JS: O(n²) → O(n) in `eliminateIdentityTemps` and `collapseTempIntoStore`**: Pre-scan temp usage counts/first-occurrence indices to replace per-temp forward scans with O(1) lookups. 31-42x faster on temp-heavy contracts.
- **JS: eliminate O(n²) `.trim()` in `rewriteForLoops`**: Pre-trim statements once instead of calling `.trim()` on every line for every `while`-scan in `findMatchingClose`. 1.7x faster on 50KB contracts (441ms → 260ms).
- **JS: regex cache for identifier helpers**: `containsIdentifier`, `countIdentifier`, and `replaceIdentifier` now cache compiled regexps per identifier, avoiding recompilation on every call.
- **JS: single-pass method partition**: `buildMethodGroups` now partitions instructions in a single walk instead of O(groups × instructions) filter calls.
- **JS: O(n) blank-line removal**: Final cleanup uses write-pointer compaction instead of O(n²) splice-in-loop.

## [0.6.2] - 2026-04-07

### Fixed

- **JS: PUSHA signed offset**: PUSHA operand is now correctly interpreted as signed I32 for backward pointer resolution, fixing wrong output for contracts using indirect backward calls via `CALLA`.
- **JS: disassembler bounds checks**: All operand reads (I8, I16, I32, I64, Jump8, Jump32, U16, U32, Syscall) now throw a proper `DisassemblyError` with code `UnexpectedEof` on truncated bytecode instead of an unhelpful `RangeError`.
- **JS: inline use-count**: `collectInlineCandidates` now counts identifier occurrences instead of using a boolean check, preventing incorrect inlining of temps used multiple times in a single expression.
- **JS: `negateCondition` compound logic**: Compound conditions with `&&`/`||` are now wrapped with `!(...)` instead of incorrectly negating only the first operator.
- **JS: compiler field null handling**: NEF compiler field parsing now stops at the first null byte (matching Rust behavior) instead of stripping only trailing nulls.

### Added

- **JS: `inlineSingleUseTemps` option**: New opt-in postprocess pass that inlines single-use temporaries into their use sites, producing cleaner output. Enable via `decompileHighLevelBytes(bytes, { inlineSingleUseTemps: true })`.
- **Rust: C# `[SupportedStandards]` attribute**: C# output now emits a proper `[SupportedStandards(...)]` attribute instead of a comment.
- **Native contract methods**: Added missing `Decimals` and `Symbol` methods to GasToken and NeoToken definitions.

### Changed

- **Rust: manifest extra rendering**: Both C# header and high-level summary now emit all string-valued manifest extra fields instead of only `author` and `email`. Keys are now matched case-sensitively (matching the JSON exactly).

## [0.6.1] - 2026-03-27

### Fixed

- **WASM `initPanicHook` binding**: Added missing `js_name = initPanicHook` attribute to `web.rs`, fixing a runtime `TypeError` when calling `init()` from the TypeScript wrapper.
- **Test panics on missing devpack fixtures**: Two unit tests (`csharp_trims_initslot_boundaries`, `high_level_trims_initslot_boundaries`) now gracefully skip when the optional `TestingArtifacts/devpack/` directory is absent instead of panicking.
- **Incorrect `MAX_NEF_FILE_SIZE` doc comment**: Corrected "10 MiB" to "1 MiB" in the re-export doc comment (`src/decompiler.rs`), matching the actual constant `0x10_0000`.
- **Disassembler split doc comment**: Consolidated the doc comment above the `#[derive]` attribute so the full description appears in generated docs.
- **JS security tests calling wrong function**: Three tests in `security.test.mjs` now correctly call `decompileHighLevelBytesWithManifest` instead of passing a manifest as options to `decompileHighLevelBytes`.
- **JS unused code cleanup**: Removed dead `stack` variable and unused `jumpTarget`/`wrapExpression` imports in `high-level.js`. Removed dead 4th argument from all `slice()` call sites in `nef.js`.

### Changed

- **JS package repository URLs**: Fixed `package.json` URLs from `neo-ngd` to `r3e-network` to match the actual repository.
- **JS README browser claim**: Corrected feature list to state "Node.js 18+, Deno, Bun" instead of claiming browser support, since `util.js` uses `node:crypto`.
- **JS shared utilities**: Extracted duplicated `scanSlotCounts`, `scanStaticSlotCount`, and `slotIndex` functions from `types.js` and `xrefs.js` into shared `util.js`.
- **SECURITY.md**: Corrected branch reference from `main` to `master`.
- **README roadmap ordering**: Swapped v0.5.x and v0.6.x sections to chronological order.
- **Plan documents**: Added completion status markers to all six plan files.
- **RELEASING.md**: Added step for JS package version consideration during releases.

### Added

- **CI JS package job**: Added `js-package` job to `ci.yml` to run JS tests on every push/PR.
- **Justfile JS/web tasks**: Added `js-test` and `web-test` recipes; included `js-test` in the `ci` recipe.

## [0.6.0] - 2026-02-16

### Added

- **Overflow collapse pass**: Automatically collapses verbose Neo C# compiler int32/int64 overflow-check patterns into clean expressions.
- **While-loop recovery**: Backward `JMP` instructions are now recognized as `while` loops with proper condition extraction.
- **Goto-to-while conversion**: Postprocess pass converts forward goto patterns into structured `while` loops.
- **Switch-break goto elimination**: Removes residual `goto` statements inside recovered `switch` blocks.
- **Continue-in-try detection**: `ENDTRY` instructions targeting loop conditions are now emitted as `continue` statements.
- **Empty-if-else inversion**: Empty `if` bodies with non-empty `else` are inverted to remove the dead branch.
- **Identity temp elimination**: Redundant `let tN = locM; locM = tN;` patterns are collapsed.
- **Temp-into-store collapsing**: `let tN = expr; locM = tN;` is simplified to `let locM = expr;`.
- **Temp-into-return collapsing**: `let tN = expr; return tN;` is simplified to `return expr;`.
- **Stack comment stripping**: Verbose `// rotate top three stack values` comments are removed from output.
- **`if true { }` collapse**: Constant-true conditionals are unwrapped to their body.
- **CALL/CALL_L method boundary detection**: Internal call targets are now recognized as method entry points even without `INITSLOT`.
- **PUSHA function pointer rendering**: `PUSHA` targets render as function pointer references instead of raw integers.
- **Implicit else for noreturn branches**: Branches ending in `abort`/`throw` now correctly suppress else emission.
- **PUSHINT128 decimal display**: 128-bit push immediates render in decimal for readability.

### Fixed

- **Try/catch nesting**: Catch/finally blocks no longer nest incorrectly inside else branches; processing order corrected in `advance_to()`.
- **Try-exit stack restoration**: `ENDTRY` properly restores the try-exit stack outside the pending-closers gate.
- **Nested try/catch brace balancing**: Deeply nested try/catch structures now produce correctly balanced braces.
- **PACK/UNPACK element ordering**: Array pack/unpack operations emit elements in correct stack order.
- **CALLA resolution**: `CALLA` (indirect call) now resolves to the correct target method.
- **Break-in-try**: `break` statements inside try blocks are correctly detected.
- **Tail-call JMP recognition**: JMP instructions at method boundaries are recognized as tail calls rather than control flow.
- **Self-referencing ENDTRY**: ENDTRY instructions targeting themselves no longer cause infinite loops.
- **If-condition inlining**: Condition temporaries are properly inlined into if-statement headers.

### Changed

- **Switch detection threshold**: Lowered minimum guarded-goto cases from 6 to 2 for better small-switch recovery.
- **Edge lookup optimization**: `collect_post_ret_method_offsets` now uses HashMap indices for O(1) lookups instead of O(n×m) linear scans.
- Comprehensive audit and validation against 101 Neo N3 devpack contracts.

## [0.5.2] - 2026-02-10

### Added

- New global CLI flag `--strict-manifest` to enforce strict manifest validation in commands that load manifests (for example `info` and `decompile`).
- New strict parsing helpers in the public API:
  - `ContractManifest::from_json_str_strict(...)`
  - `ContractManifest::from_file_strict(...)`

### Changed

- `disasm` now executes a direct bytecode disassembly path without running the full decompilation analysis pipeline.
- Added a dedicated manifest validation error path (`manifest validation error: ...`) for strict-mode failures so malformed wildcard-like values are clearly reported.

### Fixed

- Entry-point rendering now always starts at the real script entry offset; when manifest method offsets do not align, high-level and C# outputs emit a synthetic script-entry method instead of dropping entry bytecode.

## [0.5.1] - 2026-02-07

### Added

- High-level control-flow lifting now covers `CALLA`/`CALLT`/`CALL`/`CALL_L`, comparison branches, and `JMP`/`JMPIF`/`ENDTRY` long forms with label-based transfer placeholders.
- Added regression tests for native-contract metadata completeness, including latest upstream contracts and legacy token contracts used by existing fixtures.

### Changed

- Native-contract metadata generation now merges local `neo_csharp` sources with upstream Neo sources, preventing stale snapshots from dropping current contracts.
- Syscall metadata generation now uses the same local+upstream merge strategy to keep bundled syscall catalogs current.
- Updated metadata coverage documentation and release snippets in the README.

## [0.5.0] - 2025-01-30

### Added

- **SSA Transformation**: Complete Static Single Assignment form implementation
  - φ (phi) node insertion at dominance frontiers
  - Variable versioning with subscript notation
  - Proper handling of all Neo VM stack operations
- **Dominance Analysis**: Full dominance tree construction
  - Immediate dominator computation (Cooper-Harvey-Kennedy algorithm)
  - Dominance frontier calculation for φ placement
  - Dominator tree visualization support
- **SSA Rendering**: Human-readable SSA output
  - Statistics display (blocks, φ nodes, variables)
  - Clean variable naming with version subscripts

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
