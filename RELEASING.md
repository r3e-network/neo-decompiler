# Releasing neo-decompiler

The release scripts derive the version and signed tag from the root
`Cargo.toml`. They run only from a clean `master` checkout whose `HEAD` exactly
matches `origin/master`, and they stop on the first failed command.

## Prepare the release

1. Update `CHANGELOG.md` with the release date and contents.
2. Bump the crate version in `Cargo.toml` and refresh `Cargo.lock`.
3. Synchronize the Web package version:

   ```bash
   npm --prefix web install
   npm --prefix web run version:sync
   ```

4. Bump the independently versioned JS package in `js/package.json` and add its
   mapping to the Rust/Web version in `js/README.md`. The tag workflow publishes
   both npm packages, so choose an unpublished version for each package.
5. Commit and push the preparation to `master`, wait for CI to pass, and make
   sure the local checkout is clean and current.

## Automated release

Install Node.js, the Rust formatting/Clippy components, the
`wasm32-unknown-unknown` target, `wasm-pack` 0.13.1, and the `wasm-bindgen-cli`
version from `Cargo.lock`. The publisher builds the library on Rust 1.86.0,
but compiles `wasm-bindgen-cli` with Rust 1.88.0 because that tool's locked
dependencies require a newer compiler. The library's MSRV stays at 1.86.

Linux and macOS:

```bash
bash scripts/release.sh
```

Windows PowerShell:

```powershell
.\scripts\release.ps1
```

Both scripts run the full release matrix, ask before publishing the crate, and
create and push the derived signed `v<version>` tag only after `cargo publish`
succeeds. Pushing that tag triggers `publish-web.yml` (Publish npm Packages).
Its Web job requires the tag, Cargo version, and Web npm version to agree. Its
independent JS job additionally checks the version-mapping table and the
zero-dependency package contract. Both jobs build or pack without publishing
credentials, verify their archive contents and SHA-256, and publish the exact
verified archive from separate minimal jobs with no source checkout.

Configure npm trusted publishing for both `neo-decompiler-web` and
`neo-decompiler-js`, referencing `publish-web.yml`, or provide the repository's
`NPM_TOKEN` secret with publish access to both packages. The token is exposed
only to each final publish step. The JS archive is created with
`npm pack --ignore-scripts`, checked against its source-file allowlist, and
installed into a clean consumer for a public-entry-point smoke test.

## Release gates

The scripts run these gates fail-closed:

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo clippy --locked --all-targets --no-default-features -- -D warnings
cargo test --locked --all-features
cargo test --locked --no-default-features
cargo bench --locked
cargo build --locked --release
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps
npm --prefix js test
npm --prefix web ci --ignore-scripts
npm --prefix web run version:check
npm --prefix web test
npm --prefix web run verify:pack
npm --prefix web run test:wasm
cargo package --locked
cargo publish --locked --dry-run
```

The WebAssembly smoke gate loads the generated `.wasm` and exercises the real
info, disassembly, and decompilation APIs, including wide integers and recovery
after malformed input, plus nested manifest data and prototype-like keys.
The ordinary Web unit suite uses injected bindings and
does not replace this gate. On PowerShell, the release script sets
`RUSTDOCFLAGS` around the documentation command and restores its previous value.

CI also requires the pinned 103-contract devpack corpus and the Neo C# framework
assembly. With `NEO_CSHARP_CORPUS_DIR`, `NEO_SMARTCONTRACT_FRAMEWORK_DLL`, and
`NEO_CSHARP_TARGET_FRAMEWORK` configured, explicitly run the external compiler
tests using `cargo test --locked --test csharp_compile -- --ignored --nocapture`.
An ordinary `cargo test` leaves those tests ignored rather than reporting a
passing compiler check without its prerequisites.

The JS publication job runs standalone tests. Rust/JS differential parity and
external Roslyn compiler checks require the main CI workflow's Rust binary,
pinned corpus, and framework assembly; their absence in the packaging job does
not replace the requirement that main CI passes on the tagged commit.

## Manual publication

If automation cannot be used, reproduce every gate above, then publish and tag
in this order, substituting the version from `Cargo.toml`:

```bash
cargo publish --locked
git tag -s v<version> -m "neo-decompiler v<version>"
git push origin v<version>
```

Do not create or push the tag if crate publication fails. Do not run `npm
publish` from the working tree: the tag-triggered workflow builds once, checks
the archive allowlist and SHA-256, and publishes those exact bytes. Before
retrying a partial release, inspect the crates.io version, both npm package
versions, and both local and remote tag state. If one npm publication succeeds
and the other fails, rerun only the failed jobs after fixing the cause; do not
republish the completed package or move the release tag.

The scripts recheck that the working tree and `HEAD` have not changed after
verification and bind the signed tag to the verified commit.

## GitHub release and verification

After the tag-triggered npm workflow succeeds, create a GitHub release for the
same tag using the matching changelog section as its notes. Verify:

- the crate at <https://crates.io/crates/neo-decompiler>;
- documentation at `https://docs.rs/neo-decompiler/<version>`;
- the Web package at <https://www.npmjs.com/package/neo-decompiler-web>;
- the independent JS package at <https://www.npmjs.com/package/neo-decompiler-js>;
- the GitHub release at
  <https://github.com/r3e-network/neo-decompiler/releases>.
