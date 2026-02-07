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
5. Enforces `TestingArtifacts/known_unsupported.txt`:
   - listed artifacts must still fail decompile;
   - optional expected substrings must appear in the decompile error;
   - stale entries fail the sweep.

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
4. If this is a known unsupported contract, add it to
   `TestingArtifacts/known_unsupported.txt`.

Known unsupported format:

```text
path/or/name
path/or/name:expected error substring
```

## CI integration

CI runs the same check in the `artifact-sweep` job via
`tools/ci/artifact_sweep.sh`.

