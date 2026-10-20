# Release v0.14.4 — Publication Status

## ✅ COMPLETED — all four artifacts are live

| Artifact | Identifier | State |
|---|---|---|
| crates.io | `neo-decompiler 0.14.4` | ✅ published, `yanked: false` |
| npm (WebAssembly) | `neo-decompiler-web 0.14.4` | ✅ published |
| npm (pure JavaScript) | `neo-decompiler-js 2.1.4` | ✅ published |
| GitHub Release | `v0.14.4` | ✅ published with both archives attached |

### Verification

```
crates.io  0.14.4  created 2026-10-20T14:27:44Z  yanked=false
npm web    0.14.4  integrity sha512-SRHmWeyJgVVPxQwowCjqvUb72zthDW4lHR2AJfudQoOnVT52N8jlCHY0bqfX1aszD4BLE9Fbc+Ldnj2be5FhdQ==
npm js     2.1.4   integrity sha512-Bhkw+NE0OK4mFjzKlC4TjjI3yl1UgLG1KyVrXnKK9lw469oaN+YeI5nkFfUBeF2EvyPIza5OZr5ssXXiWlm5Hg==
GitHub     v0.14.4 published 2026-10-20T14:38:29Z
             assets: neo-decompiler-js-2.1.4.tgz (147,643 B)
                     neo-decompiler-web-0.14.4.tgz (802,187 B)
```

Install commands:

```bash
cargo install neo-decompiler --version 0.14.4
npm install neo-decompiler-web@0.14.4
npm install neo-decompiler-js@2.1.4
```

### End-to-end check of the published WebAssembly package

The published `neo-decompiler-web@0.14.4` tarball was installed from the npm
registry and exercised against a real contract from the pinned corpus:

```js
import { readFileSync } from "node:fs";
import { init, decompileReport } from "neo-decompiler-web";

await init(readFileSync("node_modules/neo-decompiler-web/dist/pkg/neo_decompiler_bg.wasm"));
const report = decompileReport(readFileSync("Contract1.nef"));
```

* report keys: `compiler, source, script_hash_le, script_hash_be, csharp,
  high_level, pseudocode, instructions, method_tokens, manifest, analysis, warnings`
* `warnings` is empty
* `csharp` is 2,557 characters with the full `using` set, script hashes, and
  compiler banner

### End-to-end check of the do-while fix in the published artifact

`Contract_BitOperations.nef` contains the bottom-tested loop that the fix
targets. The published package now emits the recovered `do`/`while` shape
instead of the degenerate infinite loop:

```csharp
do {
    t_7 = p4_1 + 1;
    t_10 = (BigInteger)(dynamic)(Runtime.LoadScript(...));
    p4_1 = t_7;
} while (t_10 > 0);
p6_0 = t_7 - 1;
```

The previous output was `while (@value >> (int)(t_7) > 0) { continue; }`, an
infinite loop with the state update hoisted out of the loop body.

---

## Git repository

* **Commits pushed**: 13 commits from `v0.14.3` to `b97b915f`
* **Tag pushed**: `v0.14.4` → `fe545e3f40f37b6de122c8730465a3ce0d1b1134`
  (annotated, points at `b40d86a1`)
* **Releases**: https://github.com/r3e-network/neo-decompiler/releases/tag/v0.14.4

---

## Verification matrix

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | ✅ |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | ✅ |
| `cargo clippy --locked --all-targets --no-default-features -- -D warnings` | ✅ |
| `cargo test --locked --all-features` | ✅ 973 passed, 0 failed, 1 ignored |
| `cargo test --locked --no-default-features` | ✅ 969 passed, 0 failed, 1 ignored |
| `cargo bench --locked` | ✅ |
| `cargo doc --locked --no-deps` (with `-D warnings`) | ✅ |
| `cargo package --locked` | ✅ |
| `cargo publish --locked --dry-run` | ✅ |
| `npm --prefix js test` | ✅ 1,664 passed, 0 failed, 1 skipped |
| `npm --prefix web ci --ignore-scripts` | ✅ |
| `npm --prefix web run version:check` | ✅ |
| `npm --prefix web test` | ✅ 29 passed, 0 failed |
| `npm --prefix web run verify:pack` | ✅ |
| `npm --prefix web run test:wasm` | ✅ 4 passed, 0 failed |
| `node tools/ci/check-release-metadata.mjs` | ✅ crate 0.14.4, web 0.14.4, js 2.1.4 |
| `actionlint` | ✅ |
| `bash -n scripts/release.sh` | ✅ |
| CI run `39435869732` (commit `c057c969`) | ✅ success |

CI run `39440031131` and `39440346583` also completed successfully. The
`Test (nightly)` job inside a CI run is configured with `continue-on-error`
and is an early-warning signal only; it does not gate a release.

---

## Defects found and fixed during this release

### 1. Single-block self-loop recovered as do-while (`3b204615`)

The bottom-tested loop of `log2UInt`/`log2ULong` compiles to one self-edge
block (`BB4 -> BB4`) where the body and the test share a block. The
do-while detection required the header's terminator not to be a `Branch`,
and the two-block latch search required
`strictly_dominates(header, latch)`, which a self-edge never satisfies. The
shape fell through to the generic `while` path, which hoisted the header's
own statements out of the loop: the condition kept its first value and the
body had nothing left to update it, producing an infinite loop.

The shape is now recovered explicitly: the header's statements become the
loop body, and the back-edge φ copy runs at the tail of the body. When the
inlined bottom test would read a variable the back-edge copy rewrites, the
test keeps its own variable so it is still evaluated before the copy.

The degenerate-loop count over the 110-contract corpus drops from 9 to 0, and
the literal-increment count from 2 to 0.

### 2. Postfix increment on a non-assignable operand (`3b204615`)

`3++` — emitted for a literal operand — is not a valid expression in any
emitted language. Such an increment is now spelled arithmetically.

### 3. Corpus-dependent test no longer panics in CI (`3b204615`)

`csharp_static_field_load_renders_the_field_not_runtime_load_script` read the
devpack corpus with `expect`, but that corpus is generated by CI and never
downloaded, so the Test/MSRV/Benchmark jobs panicked. It now skips like the
other corpus-dependent tests do.

### 4. Benchmark profile unwinds on panic (`c057c969`)

`[profile.bench]` inherits the release profile, which aborts on panic, but
the benchmark harness needs the unwinding strategy. Cargo used to force
every dependency to unwind while building a benchmark, which hid the
conflict; current toolchains no longer do, and the link failed with *"crate
X requires panic strategy `abort` which is incompatible with this crate's
strategy of `unwind`"*. The benchmark profile now states `panic = "unwind"`
explicitly, keeping the release optimizations. The published release
artifact is unchanged.

The root cause was found by reproducing the CI failure locally: CI uses the
floating `stable` toolchain (rustc 1.99.1) while the machine that reported
"green locally" had rustc 1.95.0. Installing 1.99.1 locally reproduced the
identical 95 errors, which confirmed the cause before the fix was written.

### 5. Web release packaging works on Windows (`b40d86a1`)

The archive verifier extracted the tarball with `tar -xzf - -C <temp>`, and
GNU tar reads a colon inside a path as the `host:path` separator of the
remote-archive syntax. On Windows the drive letter made it try to reach a
remote host and fail with *"Cannot connect to C: resolve failed"*, so every
packaging check failed locally even though the published artifact was
correct. The destination is now passed as the working directory, which tar
never interprets. `--force-local` would have fixed only GNU tar and broken
the macOS release path, which uses bsdtar.

### 6. JS mapping check compared the JS version with itself (`b97b915f`)

The JS job exports the JS package version as its `version` output, and the
mapping check read `steps.identity.outputs.version` into `CRATE_VERSION`.
That compared the JS version with the crate column of the mapping table, so
the check could only ever confirm that the JS version equalled itself: a
correct mapping failed, and a missing mapping passed. The tolerance added
for an already-published artifact hid the defect until this release added
its table row.

The crate version is now exported as its own output and the check uses it.
Verified in both directions — the real 2.1.4 → 0.14.4 mapping passes, and a
deliberately mismatched crate version is rejected.

### 7. Release identity is derived, not duplicated (`0a31e72b`)

The npm web version is now derived from the crate version in `Cargo.toml`,
and the workflow verifies that the release tag matches it before publishing.
Publication is idempotent: an already-published version is detected and
skipped instead of failing the run.

### 8. Release metadata drift is gated early (`0a31e72b`)

A new fail-closed check (`tools/ci/check-release-metadata.mjs`) verifies that
the crate, web, and js versions are mutually consistent and that the js
version-mapping table records the pair. It runs as a standalone fast CI job
and as the first step of both release scripts, so a metadata mismatch is
reported in seconds instead of after the long test suite.

### 9. GitHub release job checks out the repository (`7881fc47`)

The release job called `gh release view` without a checkout and failed with
*"not a git repository"*.

### 10. Benchmark job uses an isolated target directory (`3789675f`)

The benchmark job now builds into `target/bench` with its own cache key, so
release and benchmark artifacts cannot collide in a shared target directory.

---

## Publication run log

| Run | Trigger | Result | Note |
|---|---|---|---|
| `39435869732` | push `c057c969` | ✅ success | Benchmark job turned green |
| `39439472881` | push `v0.14.4` | ⚠️ failure | tag tree froze the pre-fix workflow; web published, js job failed on the mapping check |
| `39439579692` | dispatch `--ref master` | ⚠️ failure | same mapping-check defect, caught it in the open |
| `39440346583` | dispatch `--ref master` | ✅ success | JS published, GitHub Release created |
| `39440031131` | push `b97b915f` | ✅ success | CI on the fix commit |

A tag push runs the workflow **as it is frozen in the tag tree**, so a
workflow fix on `master` does not apply to a tag that was already pushed.
The remedy is to re-dispatch with `--ref master` and the release reference,
as recorded above.
