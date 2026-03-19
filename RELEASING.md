# Releasing neo-decompiler

1. Ensure the working tree is clean and the CI pipeline is green.
2. Update `CHANGELOG.md` with the release date and contents.
3. Bump the crate version in `Cargo.toml` and `Cargo.lock` (if present).
4. Sync the web package version with the crate version:
   ```bash
   cd web
   npm install
   npm run version:sync
   cd ..
   ```
5. Run the full verification suite:
   ```bash
   just ci
   cd web && npm test && npm run verify:pack
   cargo package
   cargo publish --dry-run
   ```
6. Create a signed git tag: `git tag -s vX.Y.Z -m "neo-decompiler vX.Y.Z"`.
7. Push commits and tags: `git push && git push --tags`.
8. Publish to crates.io: `cargo publish`.
9. Publish the web package:
   ```bash
   cd web
   npm publish
   ```
   Prefer GitHub Actions trusted publishing. If trusted publishing is not configured on npm yet, provide an `NPM_TOKEN` secret for the `Publish Web Package` workflow instead.
10. Draft a GitHub release referencing the tag and summarising key changes, including the matching npm package version.
