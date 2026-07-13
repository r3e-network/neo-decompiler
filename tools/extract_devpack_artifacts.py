#!/usr/bin/env python3
"""Extract reproducible NEF/manifest pairs from neo-devpack-dotnet artifacts."""

from __future__ import annotations

import argparse
import base64
import binascii
import json
import re
import shutil
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Sequence

REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_DEVPACK_ROOT = REPO_ROOT.parent / "neo-devpack-dotnet"
DEFAULT_OUTPUT_DIR = REPO_ROOT / "TestingArtifacts" / "devpack"
DEFAULT_EXPECTED_COUNT = 103
UPSTREAM_REPOSITORY = "https://github.com/neo-project/neo-devpack-dotnet"

NEF_PATTERN = re.compile(
    r'Convert\.FromBase64String\(@"([A-Za-z0-9+/=]+)"\)',
)
MANIFEST_PATTERN = re.compile(
    r'ContractManifest\.Parse\(@"(.+?)"\);',
    re.DOTALL,
)


class ArtifactExtractionError(ValueError):
    """An artifact source did not contain a valid NEF/manifest pair."""


@dataclass(frozen=True)
class ExtractedArtifact:
    name: str
    nef_bytes: bytes
    manifest_json: str
    original_source: Path | None


def unescape_csharp_verbatim(value: str) -> str:
    """Unescape the quote escaping used by a C# verbatim string."""
    return value.replace('""', '"')


def extract_artifact(cs_path: Path, contracts_src: Path) -> ExtractedArtifact:
    """Extract and validate one compiler artifact source."""
    try:
        source = cs_path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as error:
        raise ArtifactExtractionError(f"cannot read UTF-8 source: {error}") from error

    nef_match = NEF_PATTERN.search(source)
    if not nef_match:
        raise ArtifactExtractionError("embedded NEF was not found")

    manifest_match = MANIFEST_PATTERN.search(source)
    if not manifest_match:
        raise ArtifactExtractionError("embedded manifest was not found")

    try:
        nef_bytes = base64.b64decode(nef_match.group(1), validate=True)
    except (ValueError, binascii.Error) as error:
        raise ArtifactExtractionError(f"embedded NEF is not valid base64: {error}") from error

    manifest_raw = unescape_csharp_verbatim(manifest_match.group(1))
    try:
        manifest = json.loads(manifest_raw)
    except json.JSONDecodeError as error:
        raise ArtifactExtractionError(f"manifest is not valid JSON: {error}") from error

    name = cs_path.stem
    original_source = contracts_src / f"{name}.cs"
    return ExtractedArtifact(
        name=name,
        nef_bytes=nef_bytes,
        manifest_json=json.dumps(
            manifest,
            indent=2,
            sort_keys=True,
            ensure_ascii=False,
        )
        + "\n",
        original_source=original_source if original_source.is_file() else None,
    )


def git_output(repo: Path, *args: str) -> str | None:
    """Return a stable git query result when ``repo`` is a checkout."""
    try:
        result = subprocess.run(
            ["git", "-C", str(repo), *args],
            check=False,
            capture_output=True,
            text=True,
        )
    except OSError:
        return None
    if result.returncode != 0:
        return None
    value = result.stdout.strip()
    return value or None


def discover_git_provenance(devpack_root: Path) -> tuple[str | None, list[str]]:
    commit = git_output(devpack_root, "rev-parse", "HEAD")
    tags_output = git_output(devpack_root, "tag", "--points-at", "HEAD")
    tags = sorted(tags_output.splitlines()) if tags_output else []
    return commit, tags


def clean_output_dir(output_dir: Path, devpack_root: Path) -> None:
    resolved = output_dir.resolve()
    protected = {Path("/").resolve(), Path.home().resolve(), REPO_ROOT.resolve()}
    protected.add(devpack_root.resolve())
    if resolved in protected:
        raise ValueError(f"refusing to clean protected directory: {resolved}")
    if output_dir.is_symlink():
        raise ValueError(f"refusing to clean symlinked output directory: {output_dir}")
    if output_dir.exists():
        shutil.rmtree(output_dir)


def existing_generated_names(output_dir: Path) -> tuple[set[str], set[str], set[str]]:
    nef_names = {path.stem for path in output_dir.glob("*.nef")}
    manifest_names = {
        path.name.removesuffix(".manifest.json")
        for path in output_dir.glob("*.manifest.json")
    }
    source_names = {
        path.stem for path in (output_dir / "sources").glob("*.cs")
    }
    return nef_names, manifest_names, source_names


def write_artifacts(
    artifacts: Sequence[ExtractedArtifact],
    output_dir: Path,
    *,
    expected_count: int | None,
    discovered_count: int,
    commit: str | None,
    tags: Sequence[str],
) -> None:
    sources_dir = output_dir / "sources"
    sources_dir.mkdir(parents=True, exist_ok=True)

    copied_sources: list[str] = []
    for artifact in artifacts:
        (output_dir / f"{artifact.name}.nef").write_bytes(artifact.nef_bytes)
        (output_dir / f"{artifact.name}.manifest.json").write_text(
            artifact.manifest_json,
            encoding="utf-8",
        )
        if artifact.original_source is not None:
            shutil.copyfile(
                artifact.original_source,
                sources_dir / f"{artifact.name}.cs",
            )
            copied_sources.append(artifact.name)

    provenance = {
        "schema_version": 1,
        "source": {
            "repository": UPSTREAM_REPOSITORY,
            "commit": commit,
            "tags": list(tags),
        },
        "artifacts": {
            "discovered": discovered_count,
            "extracted": len(artifacts),
            "expected": expected_count,
            "names": [artifact.name for artifact in artifacts],
            "original_sources": copied_sources,
        },
    }
    (output_dir / "provenance.json").write_text(
        json.dumps(provenance, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--devpack-root",
        type=Path,
        default=DEFAULT_DEVPACK_ROOT,
        help=f"neo-devpack-dotnet checkout (default: {DEFAULT_DEVPACK_ROOT})",
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=DEFAULT_OUTPUT_DIR,
        help=f"artifact output directory (default: {DEFAULT_OUTPUT_DIR})",
    )
    parser.add_argument(
        "--clean",
        action="store_true",
        help="remove the output directory before writing the validated corpus",
    )
    parser.add_argument(
        "--strict",
        action="store_true",
        help="fail on skipped sources, count drift, or stale generated outputs",
    )
    parser.add_argument(
        "--expected-count",
        type=int,
        default=DEFAULT_EXPECTED_COUNT,
        help=(
            "expected extracted pair count "
            f"(default: {DEFAULT_EXPECTED_COUNT}; use 0 to disable)"
        ),
    )
    return parser.parse_args(argv)


def run(args: argparse.Namespace) -> int:
    if args.expected_count < 0:
        print("ERROR: --expected-count must be >= 0", file=sys.stderr)
        return 2
    expected_count = args.expected_count or None

    artifacts_src = (
        args.devpack_root
        / "tests"
        / "Neo.Compiler.CSharp.UnitTests"
        / "TestingArtifacts"
    )
    contracts_src = args.devpack_root / "tests" / "Neo.Compiler.CSharp.TestContracts"
    output_dir = args.output_dir

    if not artifacts_src.is_dir():
        print(f"ERROR: devpack artifacts not found at {artifacts_src}", file=sys.stderr)
        print(
            "Clone neo-devpack-dotnet or pass its location with --devpack-root.",
            file=sys.stderr,
        )
        return 1

    cs_files = sorted(
        artifacts_src.rglob("*.cs"),
        key=lambda path: path.relative_to(artifacts_src).as_posix(),
    )
    print(f"Found {len(cs_files)} artifact files in {artifacts_src}")

    extracted: list[ExtractedArtifact] = []
    failures: list[str] = []
    seen_names: set[str] = set()
    for cs_path in cs_files:
        source_id = cs_path.relative_to(artifacts_src).as_posix()
        try:
            artifact = extract_artifact(cs_path, contracts_src)
        except ArtifactExtractionError as error:
            failures.append(f"{source_id}: {error}")
            continue
        if artifact.name in seen_names:
            failures.append(f"{source_id}: duplicate artifact name {artifact.name!r}")
            continue
        seen_names.add(artifact.name)
        extracted.append(artifact)

    extracted.sort(key=lambda artifact: artifact.name)
    if expected_count is not None and len(extracted) != expected_count:
        failures.append(
            f"extracted count mismatch: expected {expected_count}, got {len(extracted)}"
        )

    expected_names = {artifact.name for artifact in extracted}
    if args.strict and not args.clean and output_dir.exists():
        existing_nefs, existing_manifests, existing_sources = existing_generated_names(
            output_dir
        )
        stale = sorted((existing_nefs | existing_manifests) - expected_names)
        if stale:
            failures.append(f"stale generated artifact(s): {', '.join(stale)}")
        expected_sources = {
            artifact.name
            for artifact in extracted
            if artifact.original_source is not None
        }
        stale_sources = sorted(existing_sources - expected_sources)
        if stale_sources:
            failures.append(f"stale generated source(s): {', '.join(stale_sources)}")

    for failure in failures:
        print(f"  {'ERROR' if args.strict else 'WARN'}: {failure}", file=sys.stderr)
    if args.strict and failures:
        print(
            f"ERROR: strict extraction rejected {len(failures)} problem(s); no files written",
            file=sys.stderr,
        )
        return 1

    try:
        if args.clean:
            clean_output_dir(output_dir, args.devpack_root)
        commit, tags = discover_git_provenance(args.devpack_root)
        write_artifacts(
            extracted,
            output_dir,
            expected_count=expected_count,
            discovered_count=len(cs_files),
            commit=commit,
            tags=tags,
        )
    except (OSError, ValueError) as error:
        print(f"ERROR: failed to write extracted corpus: {error}", file=sys.stderr)
        return 1

    print(f"Extracted {len(extracted)} contracts, skipped {len(cs_files) - len(extracted)}")
    print(f"Output: {output_dir}")
    if commit:
        tag_suffix = f" ({', '.join(tags)})" if tags else ""
        print(f"Source revision: {commit}{tag_suffix}")
    return 0


def main(argv: Sequence[str] | None = None) -> None:
    raise SystemExit(run(parse_args(argv)))


if __name__ == "__main__":
    main()
