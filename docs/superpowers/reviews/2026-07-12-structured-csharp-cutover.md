# Structured C# Cutover Review

## Scope

The Rust C# renderer now lowers instruction-bearing method bodies through the
typed structured IR. The old string emitter and line-oriented C# conversion
path are retained only as test-only compatibility utilities and are not
reachable from production C# rendering.

## Evidence

- Focused C# suite: 72 tests passed.
- Full Rust all-features suite: passed, including integration and doc tests.
- Rust no-default-features suite: 623 tests passed.
- Structured IR pipeline suite: 42 tests passed.
- Typed declaration suite: 4 tests passed.
- Artifact parity suite: 24 tests passed.
- JavaScript suite: 1,064 tests passed.
- Clippy: `cargo clippy --all-targets --all-features -- -D warnings` passed.
- Formatting and whitespace: `cargo fmt --all -- --check` and `git diff --check` passed.
- Renderer source fence: no `HighLevelEmitter`, legacy backend, or line-oriented
  C# conversion symbols under `src/decompiler/csharp`.
- Artifact sweep: 7 successful artifacts, 1 documented expected unsupported
  artifact, 0 unexpected failures, and matching deterministic output hashes.
- Roslyn: one ignored test covering five representative contracts passed with
  `Neo.SmartContract.Framework` 3.7.4.1 (`net8.0`) and 3.10.0 (`net10.0`).

## Remaining Risk

The artifact corpus does not include the optional devpack contracts, so those
parity checks remain skipped. The known `CallFlagInvalid` artifact still
requires the documented unsupported-call-flags exception. Broader contracts
outside the repository corpus should be validated with the optional Roslyn
gate before release.
