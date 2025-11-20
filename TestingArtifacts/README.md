# TestingArtifacts

This folder is for real-world Neo contracts that should be exercised by the artifact decompilation test.

Supported layouts (recursively discovered):
- C# source containing both `ContractManifest.Parse(@...)` and `Convert.FromBase64String(@...)` snippets (matching the official compiler output). The test reads these blobs, writes the NEF/manifest into `TestingArtifacts/decompiled/<relative-path>/`, and verifies the decompilation.
- Paired files named `<contract>.nef` and `<contract>.manifest.json` living side-by-side. These are picked up directly and decompiled in place, preserving the relative directory structure under `decompiled/`.

Known limitations:
- List any contracts that are expected to fail in `TestingArtifacts/known_unsupported.txt` (one name per line; `#` for comments). You can optionally specify an expected error substring after a colon, e.g. `edgecases/callflags/CallFlagInvalid:unsupported bits`. Failures for those entries are recorded as `<contract>.error.txt` in `TestingArtifacts/decompiled/` and must be non-empty; when an expected substring is provided, the error text is validated to contain it.

Outputs:
- All generated files mirror the source layout under `TestingArtifacts/decompiled/`, which is already git-ignored.
