# Contributing

Thanks for taking the time to improve neo-decompiler! We aim to keep the
project focused, easy to reason about, and straightforward to run locally.

## Getting started
- Install a recent stable Rust toolchain (see `rust-version` in `Cargo.toml`).
- Clone the repository and ensure submodules (if any) are initialised.
- Install optional tooling: [`cargo-edit`](https://github.com/killercup/cargo-edit) for dependency bumps and [`just`](https://github.com/casey/just) for local recipes (if you use them).

## Development workflow
- Keep changes scoped and well documented.
- Run the full test suite before sending a pull request:
  - `cargo fmt`
  - `cargo clippy --all-targets --all-features`
  - `cargo test`
- For release-related changes, also verify the crate packages cleanly with
  `cargo package --allow-dirty --no-verify`.
- Include or update tests alongside behaviour changes. For CLI additions, add
  assertions in `tests/cli_smoke.rs` where possible.
- Maintain the high-level ergonomics of the public library API—avoid leaking
  internal implementation details.

## Commit guidelines
- Use clear commit messages describing *why* a change is needed.
- Avoid unrelated whitespace churn.
- Squash fix-up commits before merging when feasible.

## Code of conduct
Participation in this project is governed by the
[`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md). By contributing you agree to uphold
these standards.

## Licensing
This project is dual licensed under MIT or Apache-2.0. By submitting a patch
you agree to license your work under these terms.
