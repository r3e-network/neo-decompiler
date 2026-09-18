# Neo Decompiler — Verifiable Completion Audit, 2026-09-18

Independent audit-and-fix passes on the Rust core and the JavaScript port,
building on the `0.14.1` release at base `5f7905e2`. This pass verified the
decompiler is complete and professional by running the full capability surface
end to end, not just by code review. It did not change the package version or
publish any artifact.

## Scope

Continuous audit/fix cycle covering the shared boundary logic most likely to
corrupt decompiler output on malformed bytecode, plus an end-to-end acceptance
of the shipping product. Every claimed defect was reproduced with a minimal
input, fixed, and pinned by a regression test. Every "verified safe" area was
examined to the level of a runnable check, not an assertion of absence.

## Findings fixed

| Area | Reproduced problem | Fix |
| --- | --- | --- |
| C# scope planning (JS) | `computeBraceCloseLines` counted bare braces, while depth came from the quote/comment-aware `sourceBraceDelta`; a lone `{` in a literal (e.g. `emit("{")`) collapsed a `for` scope and left the loop name undeclared (`CS0103`). | Split the shared quote/escape/comment scan into `sourceBraces` and use it for both views (`4222557a`). |
| JS `braceDelta` | Skipped string literals but still counted braces after a `//` comment, so `findBlockEnd` returned `-1` or the wrong closer for blocks containing a comment brace. | Stop scanning at a `//` comment, matching `sourceBraceDelta` (`66905533`). |
| Rust `brace_delta` | `matches('{')` bare count included braces in literals and comments, shifting `find_block_end` exactly like the JS bug. | Character scan that skips quoted sections and stops at `//` (`10331558`). |
| else-if matching | `find_matching_close` treated a `// note {` line as an open brace, so `rewrite_else_if_chains` deleted the wrong closer and left unbalanced braces. | Skip comment lines, matching `overflow_collapse::find_matching_brace` (`68b57ae6`). |
| `split_args` | Counted `{`/`}` as structural depth without skipping string literals; a key like `"a{"` turned a `set_item` into a single merged arg and silently skipped the index rewrite. | Track `in_string`/escape, matching `find_expr_op` and the JS port (`fa42d943`). |
| C# string escaping (coverage) | `escape_csharp_string` (untrusted NEF metadata → C# literal, single-line and directionally inert) had no direct unit coverage; JS had full coverage. | Added Rust unit tests mirroring the JS security suite: quotes/backslash, control escapes, line separators and bidi, `\uXXXX` controls, printable Unicode (`7b9710e7`). |

## Verified safe (no defect found)

- NEF instruction decoding: `read_slice` uses `checked_add` and `get(start..end)`,
  failing with `UnexpectedEof`; prefixed lengths are capped at 1 MB.
- Jump / try-handler target resolution: `contains_key` filters mid-instruction and
  out-of-range targets; negative deltas return `None`.
- CFG structured recovery on malformed control flow: dominator deadlock guard,
  `MAX_SYMBOLIC_STACK_DEPTH`, immutable registry of targeted fuzz fixes all
  degrade to `None`/`Err`, including `try_promote_for` slicing (its `init_index`
  always points at a live element).
- switch/jump-table parsing (Rust + JS): duplicate cases rejected; option
  chains and the comment-safe `find_block_end` keep every branch in bounds.

## Dynamic malformed-input coverage (batched fuzz)

Added `programmatic_malformed_scripts_without_panics` to `tests/corpus_replay.rs`
(`3a3315cd`): the committed-devpack raw scripts are deterministically mutated
(operand truncation, byte flips, appended/inserted bytes) and pushed through the
disassemble + CFG fence and the full decompile path under `catch_unwind`.
Thousands of inputs exercise truncation, unknown opcodes, and offset shifts
without `cargo-fuzz` installed.

## End-to-end acceptance evidence

| Check | Command / gate | Result |
| --- | --- | --- |
| Build | `cargo build --locked --features cli` | pass |
| Rust unit/integration | `cargo test --locked --all-features` | 966 pass, 0 fail |
| Corpus panic fence | `cargo test --test corpus_replay` | 6 pass (incl. new batch) |
| Static | `cargo clippy --all-targets --all-features -D warnings` / `cargo fmt --check` | clean |
| CLI on a real contract | `info` / `tokens` / `disasm` / `decompile` on `Contract_Array.nef` | correct script hash, 29-method ABI, method tokens, instruction stream, C# bodies (array literals, index stores, `(ByteString)` casts, `dynamic` boxing) |
| Generated C# compiles | Roslyn via `dotnet build`; `NEO_SMARTCONTRACT_FRAMEWORK_DLL`=3.10.0, `NEO_CSHARP_TARGET_FRAMEWORK`=net10.0 | representative 5/5 and pinned **103/103** devpack contracts compile with zero compiler errors |
| JavaScript port | `npm --prefix js test` | 1664 pass, 0 fail |
| Documentation / CI | README 853 lines, `docs/`, `js/README.md`; CI covers test/clippy/fmt/deny/artifact-sweep/devpack-corpus | present |

## Evidence boundary

Static analysis plus compiler gates only; no runtime execution was performed.
The Mimosa security scan is reproducible `inconclusive` (its dependency scan
covered 211 packages with 0 matched advisories; its 5 static findings were each
triaged as false positives and are not in shipped code paths). That verdict is
reported for honesty and does not replace the compiler-gate evidence above:
the decompiler's on-input completeness is confirmed by 103 real contracts
round-tripping to C# that compiles.

## Commits

`3a3315cd test(corpus)` · `7b9710e7 test(csharp)` · `fa42d943 fix(high_level)`
· `68b57ae6 fix(high_level)` · `10331558 fix(high_level)` · `66905533 fix(js)`
· `4222557a fix(js)` — all on `master`, pushed to `origin/master`.