# Releasing neo-decompiler

1. Ensure the working tree is clean and the CI pipeline is green.
2. Update `CHANGELOG.md` with the release date and contents.
3. Bump the crate version in `Cargo.toml` and `Cargo.lock` (if present).
4. Run the full verification suite:
   ```bash
   cargo fmt
   cargo clippy --all-targets --all-features -- -D warnings
   cargo test
   cargo package --allow-dirty --no-verify
   ```
5. Create a signed git tag: `git tag -s vX.Y.Z -m "neo-decompiler vX.Y.Z"`.
6. Push commits and tags: `git push && git push --tags`.
7. Publish to crates.io: `cargo publish`.
8. Draft a GitHub release referencing the tag and summarising key changes.
