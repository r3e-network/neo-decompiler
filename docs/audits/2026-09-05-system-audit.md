# Neo Decompiler System Audit — 2026-09-05

## Executive summary

The repository was synchronized with `origin/master` at
`33b5b8f967ca86c26d216e969069045fdfb1a0ff` before review. The local and remote
revisions were identical (`0` ahead, `0` behind).

The audit covered the Rust library and CLI, the independent JavaScript port,
the WebAssembly/browser package, generated high-level and C# source, parser and
analysis resource limits, schemas, benchmarks, dependencies, CI, and release
automation. The standard scan identified eight validated security findings;
an independent integration review found one additional generated-code issue.
The final total is nine: three high, four medium, and two low. All nine have
repository-level remediations in this work tree, with focused regression tests
at their trust boundaries.

## Method

- Used the repository's CodeGraph index first for symbol and call-path
  discovery, then verified the relevant source and tests directly.
- Performed a standard whole-repository security scan with independent source
  validation. Scan ID: `c93c1cda-7407-4d72-b5f5-21efbbe1f20d`.
- Traced attacker-controlled NEF/manifest/report data from parser entry points
  through CFG/SSA analysis and every generated-source, terminal, JSON, and DOM
  sink.
- Reviewed GitHub Actions and local release automation as separate credential
  and artifact-integrity trust zones.
- Ran lockfile advisory, license, duplicate-version, and source-policy checks
  with `cargo deny`.
- Added deterministic performance baselines instead of asserting unmeasured
  speedups.

## Validated findings and disposition

| Severity | Finding | Remediation |
| --- | --- | --- |
| High | Compact CALLT parameter metadata amplified allocations, analysis, and rendering at every call site. | Rust and JS reject arity above the Neo VM stack limit (2,048); default call metadata is sparse; malformed underflow processes only present values; per-method SSA argument work has a proportional cumulative budget; source rendering emits only present arguments plus one explicit omission marker. |
| High | Mutable build inputs ran in the same job as npm OIDC/token authority. | Package construction is an unprivileged job. A separate minimal publisher has only OIDC authority, no checkout, and immutable action references; the fallback token exists only on the final publish step. |
| High | An arbitrary method-token name became executable syntax in JS-generated C#, and same-named tokens from different contracts collapsed to the last hash. | Unknown and restricted CALLT targets use deterministic, index-derived inert labels. C# lowering maps each label back to that exact token and places the original method only in a hardened string literal; native framework calls retain their qualified labels. |
| Medium | The inspected npm dry-run was not the exact archive later published. | The package is built once, packed once with lifecycle scripts disabled, allowlist-checked, hashed, transferred as an artifact, re-hashed, and published by exact filename. |
| Medium | Metadata line/control characters escaped generated C# comment boundaries. | Shared Rust/JS visible-text encoders cover C0/C1, line separators, ESC, and bidi controls at source-rendering boundaries; C# literal encoders were hardened too. Structured JSON retains original values. |
| Medium | Schema validation accepted unbounded file/stdin input and accumulated every diagnostic. | Input is capped at 128 MiB, diagnostics at 1 MiB, and rendered validation errors at 100, with explicit truncation notices. |
| Medium | PowerShell continued after failed native release commands. | Both platform scripts are fail-closed, derive the version/tag, require a clean synchronized `master`, and gate publication/tagging on every command's exit status. |
| Low | Human CLI output emitted active terminal control and bidi sequences. | All untrusted fields and warning text are visibly encoded only at human-text boundaries; JSON regression tests preserve raw structured data. |
| Low | The browser buffered oversized NEF/manifest files before Rust limits ran. | Browser admission checks `File.size` before `arrayBuffer()`/`text()` and tests prove rejected files are never read. |

## Engineering improvements

### Performance and regression measurement

The new Criterion suite uses deterministic, in-memory NEF fixtures. It measures
CFG construction, SSA construction, disassembly, and end-to-end decompilation
without depending on the optional `TestingArtifacts` checkout. Criterion is
pinned to the current MSRV-compatible 0.8 line with only the required benchmark
feature enabled, reducing benchmark-only dependency and compile overhead.

### Release and package integrity

- CI and release actions are pinned to immutable revisions, CI defaults to
  read-only repository permissions, and `wasm-pack` is pinned consistently.
- Tag, Cargo, and npm versions must agree before packaging.
- The npm archive has an exact file allowlist, per-file and total size limits,
  no links or traversal paths, and a SHA-256 binding across jobs. Its gzip and
  tar headers are bounded and validated in memory before any extraction.
- Local release scripts preserve existing Rust, JS, Web, package, and
  documentation gates and create a signed tag only after publication succeeds.
- The crates.io allowlist retains the Rust sources, public tests, deterministic
  fixtures, examples, benchmark, embedded schemas, and legal files while
  excluding unrelated JS/Web packages, internal tooling, generated corpora,
  and planning material. The verified archive fell from 725 to 486 files and
  is 635.9 KiB compressed.

### Repository hygiene

- Added `.gitattributes` so text remains LF across platforms while `.nef` and
  `.bin` fixtures remain binary. This removes the Windows CRLF churn that had
  made hundreds of content-identical files appear modified.
- `just ci` now includes Web package tests as well as Rust and the independent
  JavaScript port.
- Removed stale unused-license allowances so dependency policy checks are
  warning-free.
- Rejected speculative refactor scaffolding that was not wired into the main
  decompiler and would have added public configuration, interning, worklist,
  emitter-state, and trait abstractions without changing runtime behavior.

## Architecture assessment

The core is in good condition: Rust forbids unsafe code, NEF/manifest raw sizes
are bounded, checksum/reserved/trailing-data validation is explicit, analysis
passes contain convergence or complexity fences, browser rendering uses
`textContent`, and there is no authentication, database, server, subprocess,
or runtime network boundary in normal decompilation.

The largest maintainability cost is the coexistence of the legacy high-level
emitter and structured-IR/C# pipeline. A broad emitter/trait rewrite was not
performed: it would be high-risk and is not justified by the current findings.
Future simplification should be driven by corpus parity and benchmark evidence,
then remove a path rather than introduce another abstraction layer.

## Residual and external risks

- npm trusted-publisher bindings, protected tags, default organization policy,
  and secret scope are external to this repository and must be verified by an
  administrator.
- Pinned action/tool versions remove mutable references but require deliberate
  reviewed upgrades for future security fixes.
- Corpus-wide Roslyn compilation depends on the external Neo framework assembly
  and pinned development corpus; local runs without those assets can only skip
  that environment-dependent gate. CI remains the authoritative full-corpus
  check.
- The schema cap intentionally allows large reports produced from maximum-size
  contracts. Callers embedding validation in stricter environments may impose
  a lower outer limit.

## Verification

| Gate | Result |
| --- | --- |
| `cargo test --locked --all-features` | 1,056 passed; 3 ignored; 0 failed. |
| `cargo test --locked --no-default-features` | 1,010 passed; 3 ignored; 0 failed. |
| Clippy, all targets, with and without default features | Passed with warnings denied. |
| Rustdoc, dependencies excluded | Passed with warnings denied. |
| Rust 1.86.0 MSRV check, all targets | Passed. The Windows audit host required an explicit GNU C compiler because its global `CC` selected an incompatible LLVM-MinGW compiler for the MSVC target; CI's Linux environment does not need that local override. |
| Independent JavaScript suite | 1,532 passed; 1 environment-dependent corpus compilation skipped; 0 failed. |
| Web package suite | 17 passed; 0 failed. |
| `cargo deny check` | Advisories, bans, licenses, and sources all passed. |
| Criterion benchmark compilation | Passed for the deterministic decompiler suite. |
| Workflow and release-script syntax | Both workflows passed `actionlint`; PowerShell, Node, and Bash syntax checks passed. |
| `cargo package --allow-dirty --locked` | Verified the unpacked 486-file, 635.9-KiB crate by compiling it successfully. |

Focused regressions cover every finding above, including Rust/JS arity parity,
sparse CALLT underflow, cumulative work exhaustion, bounded rendering,
token-index target identity, hostile generated-source and terminal controls,
single-layer event-name literal escaping, unchanged structured JSON, schema
input/error limits, browser pre-read rejection, and tar-header validation before
extraction.

**Second-review correction:** the original Rust C# compilation tests returned
success without compiling when the framework/corpus environment was unset.
The first-round counts above are test-runner counts, not evidence that Roslyn
ran; the original claim that only JavaScript skipped that gate was incorrect.
The second review makes these tests explicitly ignored by default and runs
them with the pinned corpus and framework configured. See the
[second-review report](2026-09-05-second-review.md) for the actual compilation
results.
