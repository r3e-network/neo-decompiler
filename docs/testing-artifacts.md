# Testing Artifacts Guide

This guide explains how to add contract artifacts and validate them with the
same checks that run in CI.

## What the sweep checks

Run:

```bash
just artifact-sweep
```

Equivalent direct command:

```bash
bash tools/ci/artifact_sweep.sh
```

The sweep script:

1. Runs `decompile_artifacts` twice and checks output determinism.
2. Discovers artifacts under `TestingArtifacts/` and generated embedded samples
   under `TestingArtifacts/decompiled/embedded/`.
3. Runs `info`, `disasm`, `decompile`, and `tokens` JSON output for each
   discovered artifact.
4. Validates each JSON output against the embedded CLI schemas.
5. Enforces the expected-failure registries:
   - `known_unsupported.txt` contains valid artifacts whose `decompile` command
     must fail while `info`, `disasm`, and `tokens` continue to pass;
   - `expected_invalid.txt` contains malformed artifacts that every parser-backed
     command must reject;
   - optional expected substrings must appear in each expected error;
   - stale entries and unexpected successes fail the sweep.

## Supported artifact inputs

The artifact loader supports these layouts recursively:

- `Example.nef` + `Example.manifest.json`
- compiler-style `*.cs` source containing both:
  - `ContractManifest.Parse(@"...")`
  - `Convert.FromBase64String(@"...")`

## Adding a new artifact

1. Copy the contract into `TestingArtifacts/` using one of the supported
   layouts.
2. Run `just artifact-sweep`.
3. Check generated outputs in `TestingArtifacts/decompiled/`.
4. If a valid contract exposes a known decompiler limitation, add it to
   `TestingArtifacts/known_unsupported.txt`. If the artifact is intentionally
   malformed, add it to `TestingArtifacts/expected_invalid.txt` instead.

Both registry files use the same format:

```text
path/or/name
path/or/name:expected error substring
```

## CI integration

CI runs the same check in the `artifact-sweep` job via
`tools/ci/artifact_sweep.sh`. A separate 30-minute job checks out
`neo-devpack-dotnet` v3.10.0 at its pinned commit, strictly extracts all 103
NEF/manifest pairs, and runs the same corpus checks. Reproduce the extraction
locally with:

```bash
python3 tools/extract_devpack_artifacts.py \
  --devpack-root /path/to/neo-devpack-dotnet \
  --output-dir TestingArtifacts/devpack \
  --expected-count 103 --strict --clean
```

The extractor writes stable `provenance.json` metadata containing the source
commit, exact tags, counts, and sorted artifact names.
