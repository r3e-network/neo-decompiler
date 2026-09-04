# Neo Decompiler — Second Review, 2026-09-05

This review builds on the uncommitted first-round changes at base commit
`33b5b8f967ca86c26d216e969069045fdfb1a0ff`. Independent reviews covered Rust
analysis, JavaScript rendering, Web/release behavior, and final integration.
The work focused on reproducible correctness problems and shared boundary
logic; it did not change the package version or publish any artifacts.

## Findings fixed

| Area | Reproduced problem | Result |
| --- | --- | --- |
| Dominators | A 1,500-block branch arm made the old 1,000-step ancestor cutoff report block 499 as the merge's immediate dominator instead of entry block 0. | Precomputed reverse-post-order ranks drive ancestor intersection. Each step decreases rank; repeated full-depth scans and the incorrect cutoff are removed. |
| Dominance frontiers | Entry self-loops lost their frontier; unreachable predecessors contributed false frontiers. | Reachability and root handling now match the definition, checked against an independent dominator-set oracle over 64 deterministic cyclic graphs. |
| JS call execution | Deferred CALL/CALLA/CALLT/SYSCALL expressions could run in reversed argument order or disappear after CLEAR/argument truncation. | Results are materialized when the VM executes each call. All 300 argument producers execute once even when only 256 arguments are rendered. |
| JS CALLT lowering | Nested token calls were skipped, and comment parentheses or labels could be treated as source syntax. | Rewrites replace call boundaries while retaining the original argument text and token-index identity. The shared scanner treats literals and comments as opaque. |
| JS expression splitting | Comparison and shift operators could be mistaken for generic argument delimiters. | Expression parsing distinguishes operators from generic suffixes; event type declarations opt into their separate context. |
| Manifest parsing | Standard `str::parse::<ContractManifest>()` bypassed the byte limit applied by explicit parser APIs. | All text entry points enforce the limit through `FromStr`; exact-limit and oversized valid JSON are covered. |
| File input | NEF and manifest loaders checked path metadata and then read without a bound, allowing file replacement/growth to defeat the memory cap. | Shared bounded I/O opens a file once, checks that handle, and reads at most the limit plus one detection byte. NEF library/CLI, manifest file/reader, and schema input reuse this logic with their existing error categories. |
| Rust test gates | Roslyn tests returned success without compilation when environment variables were missing. The local script used `--ignored` while the tests were not ignored. | Tests explicitly declare required external dependencies; CI/scripts opt in with `--ignored` and missing dependencies fail. Both representative and pinned compilation gates were actually run. |
| Corpus panic fence | Nested `catch_unwind` calls swallowed panics before the outer test could report them; parser smoke also ignored panics. | One labeled boundary propagates the original failure. An injected-panic regression proves that the harness fails with the target and input identity. |
| Corpus replay cost | The parse-only target performed a full decompilation, and generated mirrors were repeatedly treated as source inputs. | The parse target only parses; shared source discovery excludes `TestingArtifacts/decompiled` while retaining original fixtures and the pinned devpack corpus. |
| Windows packaging | Directly spawning `npm.cmd` failed, and URL pathname handling gave an incorrect Windows cleanup path. | The pack command and filesystem URL conversion work on Windows; a real package was built and verified. |
| Release toolchain | The pinned wasm-bindgen CLI's locked dependencies require Rust 1.88 although the library supports 1.86. | Tool compilation uses pinned Rust 1.88 separately; the real WASM library build remains on verified Rust 1.86. |
| Archive interpretation | High-bit ASCII aliases, ambiguous tar layouts, and metadata-only dangling/stacked extensions could disagree with the extractor. | Preflight rejects ambiguous headers before extraction and includes targeted archive fixtures. |
| Real WASM API | Lossless serde serialization returned small offsets/lengths as bigint despite TypeScript number declarations; manifest maps disappeared in JSON display. | The wrapper normalizes safe integers to number, preserves wide integers as bigint, and converts Maps with `Object.fromEntries`, retaining even `__proto__` as an own data property. The demo renders wide integers as exact decimal strings. Real binding smoke tests exercise all report APIs, maximum I64 values, nested manifest data, and unchanged object prototypes. |
| Benchmark fixtures | An SSA branch jumped to ADD without an operand, so the benchmark measured malformed recovery. | Corrected branch displacement and added fixture validation outside timed sections. |

## Verification and evidence

- Rust all-features suite: 1,068 passed, five explicitly ignored. The two
  ignored Roslyn gates were run separately with dependencies configured.
- Rust no-default-features suite: 1,021 passed, five explicitly ignored.
- The final corpus-harness correction was then rerun separately with all
  features: all five tests passed, including injected-panic propagation and
  source/mirror discovery. This rerun supersedes the older three-test corpus
  result included in the full-suite counts above.
- Real Rust Roslyn compilation: five representative contracts and 103/103
  pinned devpack contracts passed, zero compiler errors.
- JavaScript full suite: 1,541 passed, zero skipped or failed, including
  103/103 pinned Roslyn compilation and differential tests against the current
  debug Rust executable.
- Explicit Rust 1.86 compiler: `cargo check --locked --all-targets
  --all-features` passed in a separate target directory. Both `RUSTC` and
  `RUSTDOC` were resolved from that toolchain, because this Windows host's
  ambient PATH otherwise selects a different compiler. The host's GNU C
  compiler was explicitly selected for native build dependencies.
- Web: 28 unit tests and four real-WASM smoke tests passed. The library was
  compiled with Rust 1.86, wasm-pack 0.13.1, and matching wasm-bindgen 0.2.125.
  The exact ten-member npm archive was verified after construction: 791,767
  bytes, SHA-256
  `3a5bd71700b8cf6f2a8d8b5429e309a7f007cff92070530d443a1fe085f442d3`.
- Independent final review found no additional blocker in bounded I/O, the
  explicit compilation gates, or the JS scanner/call-effect changes.
- Both Clippy feature configurations passed with warnings denied, as did
  Rustdoc. Formatting, workflow actionlint, PowerShell syntax, Bash syntax,
  and diff whitespace checks passed.
- `cargo package --allow-dirty --locked` compiled the unpacked 487-file Rust
  crate successfully (640.7 KiB compressed). A clean consumer also installed and initialized the
  actual npm archive. No package was uploaded.

### Exploratory performance baseline

The corrected Criterion suite ran with 0.2-second warmup, 0.5-second
measurement, and ten samples per case. Selected central estimates on this
Windows host were:

| Case | Estimate |
| --- | --- |
| CFG construction, 1,000 instructions | 23.63 microseconds |
| SSA construction, 100 branch groups | 736.57 microseconds |
| Full decompilation, 512 push/drop pairs | 8.55 milliseconds |
| Disassembly, 8,192 push/drop pairs | 270.14 microseconds |

These are short baseline measurements taken alongside other validation work,
not a controlled before/after speedup claim. The corrected SSA fixture is not
directly comparable to the previous malformed fixture. Cargo also reports its
existing rlib/cdylib output-name collision warning for this benchmark build;
compilation and all benchmark cases complete successfully.

## Correction to the first report

The first report treated Rust test-runner success as evidence that the Roslyn
gates had compiled their fixtures. Their early-return behavior made that
inference invalid. This review records explicit external-tool execution and
fixes the gate so ordinary test output accurately marks it ignored. The first
report now links this correction instead of claiming those implicit passes.

## Remaining scope

Legacy high-level and structured C# rendering still coexist. Further migration
should preserve corpus parity and demonstrated VM evaluation order. Repository
changes cannot verify external npm trusted-publisher bindings or organization
tag-protection policy. No commit, tag, push, or publication was performed.
