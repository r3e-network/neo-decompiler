#!/usr/bin/env python3
"""
Extract NEF binary and manifest JSON from neo-devpack-dotnet TestingArtifacts.

Each artifact .cs file contains:
  - A base64-encoded NEF:  Convert.FromBase64String(@"...").AsSerializable<...NefFile>();
  - A JSON manifest:       ContractManifest.Parse(@"{...}");

This script extracts both and writes them as .nef / .manifest.json files.
"""

import base64
import json
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
DEVPACK_ROOT = REPO_ROOT.parent / "neo-devpack-dotnet"
ARTIFACTS_SRC = DEVPACK_ROOT / "tests" / "Neo.Compiler.CSharp.UnitTests" / "TestingArtifacts"
CONTRACTS_SRC = DEVPACK_ROOT / "tests" / "Neo.Compiler.CSharp.TestContracts"
OUTPUT_DIR = REPO_ROOT / "TestingArtifacts" / "devpack"

NEF_PATTERN = re.compile(
    r'Convert\.FromBase64String\(@"([A-Za-z0-9+/=]+)"\)',
)
MANIFEST_PATTERN = re.compile(
    r'ContractManifest\.Parse\(@"(.+?)"\);',
    re.DOTALL,
)


def unescape_csharp_verbatim(s: str) -> str:
    """Unescape C# verbatim string (only "" -> " needed)."""
    return s.replace('""', '"')


def extract_artifact(cs_path: Path) -> tuple[str, bytes, str] | None:
    """Extract contract name, NEF bytes, and manifest JSON from a .cs artifact."""
    text = cs_path.read_text(encoding="utf-8")

    nef_match = NEF_PATTERN.search(text)
    if not nef_match:
        return None

    manifest_match = MANIFEST_PATTERN.search(text)
    if not manifest_match:
        return None

    name = cs_path.stem
    nef_bytes = base64.b64decode(nef_match.group(1))
    manifest_raw = unescape_csharp_verbatim(manifest_match.group(1))

    # Validate JSON
    try:
        manifest_obj = json.loads(manifest_raw)
    except json.JSONDecodeError as e:
        print(f"  WARN: {name}: invalid manifest JSON: {e}", file=sys.stderr)
        return None

    manifest_json = json.dumps(manifest_obj, indent=2)
    return name, nef_bytes, manifest_json


def find_original_source(name: str) -> Path | None:
    """Try to find the original C# source contract for comparison."""
    # TestContracts use the same name as the artifact
    candidate = CONTRACTS_SRC / f"{name}.cs"
    if candidate.exists():
        return candidate
    return None


def main() -> None:
    if not ARTIFACTS_SRC.exists():
        print(f"ERROR: devpack artifacts not found at {ARTIFACTS_SRC}", file=sys.stderr)
        print("Make sure neo-devpack-dotnet is cloned alongside neo-decompiler.", file=sys.stderr)
        sys.exit(1)

    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    sources_dir = OUTPUT_DIR / "sources"
    sources_dir.mkdir(exist_ok=True)

    cs_files = sorted(ARTIFACTS_SRC.glob("*.cs"))
    print(f"Found {len(cs_files)} artifact files in {ARTIFACTS_SRC}")

    extracted = 0
    skipped = 0
    for cs_path in cs_files:
        result = extract_artifact(cs_path)
        if result is None:
            print(f"  SKIP: {cs_path.name} (no NEF or manifest found)")
            skipped += 1
            continue

        name, nef_bytes, manifest_json = result
        nef_path = OUTPUT_DIR / f"{name}.nef"
        manifest_path = OUTPUT_DIR / f"{name}.manifest.json"

        nef_path.write_bytes(nef_bytes)
        manifest_path.write_text(manifest_json, encoding="utf-8")

        # Copy original source if available
        original = find_original_source(name)
        if original:
            (sources_dir / f"{name}.cs").write_text(
                original.read_text(encoding="utf-8"), encoding="utf-8"
            )

        extracted += 1

    print(f"\nExtracted {extracted} contracts, skipped {skipped}")
    print(f"Output: {OUTPUT_DIR}")


if __name__ == "__main__":
    main()
