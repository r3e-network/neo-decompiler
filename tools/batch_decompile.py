#!/usr/bin/env python3
"""
Batch-decompile all extracted devpack NEF+manifest pairs and collect results.

Outputs high-level, csharp, and pseudocode for each contract, plus a summary
JSON and optional error report.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import os
import subprocess
import sys
import threading
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[1]
DEVPACK_DIR = REPO_ROOT / "TestingArtifacts" / "devpack"
OUTPUT_DIR = DEVPACK_DIR / "decompiled"
BINARY = REPO_ROOT / "target" / "release" / "neo-decompiler"

FORMATS: tuple[tuple[str, str], ...] = (
    ("high-level", ".hl.txt"),
    ("csharp", ".cs"),
    ("pseudocode", ".pseudo.txt"),
)
DEFAULT_TIMEOUT_SECONDS = 30
DEFAULT_JOBS = min(8, os.cpu_count() or 1)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--jobs",
        type=int,
        default=DEFAULT_JOBS,
        help=f"Maximum concurrent contracts to process (default: {DEFAULT_JOBS})",
    )
    parser.add_argument(
        "--timeout",
        type=int,
        default=DEFAULT_TIMEOUT_SECONDS,
        help=(
            "Per-format subprocess timeout in seconds "
            f"(default: {DEFAULT_TIMEOUT_SECONDS})"
        ),
    )
    parser.add_argument(
        "--binary",
        type=Path,
        default=BINARY,
        help=f"Path to neo-decompiler binary (default: {BINARY})",
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=OUTPUT_DIR,
        help=f"Output directory (default: {OUTPUT_DIR})",
    )
    return parser.parse_args()


def atomic_write_text(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp_name = f".{path.name}.{os.getpid()}.{threading.get_ident()}.tmp"
    tmp_path = path.with_name(tmp_name)
    tmp_path.write_text(content, encoding="utf-8")
    tmp_path.replace(path)


def decompile_one(
    name: str,
    nef_path: Path,
    manifest_path: Path,
    *,
    binary: Path,
    output_dir: Path,
    timeout_seconds: int,
) -> dict[str, Any]:
    """Decompile a single contract and return a result record."""
    result: dict[str, Any] = {"name": name, "errors": [], "formats": {}}

    for fmt, out_ext in FORMATS:
        cmd = [
            str(binary),
            "decompile",
            str(nef_path),
            "--manifest",
            str(manifest_path),
            "--format",
            fmt,
        ]
        try:
            proc = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                timeout=timeout_seconds,
            )
            if proc.returncode == 0:
                out_path = output_dir / f"{name}{out_ext}"
                atomic_write_text(out_path, proc.stdout)
                result["formats"][fmt] = True
            else:
                result["formats"][fmt] = False
                stderr = proc.stderr.strip().replace("\n", " ")
                result["errors"].append(f"{fmt}: {stderr[:200]}")
        except subprocess.TimeoutExpired:
            result["formats"][fmt] = False
            result["errors"].append(f"{fmt}: TIMEOUT")
        except Exception as exc:  # pragma: no cover - defensive for batch scripts
            result["formats"][fmt] = False
            result["errors"].append(f"{fmt}: {exc}")

    return result


def main() -> None:
    args = parse_args()
    if args.jobs < 1:
        print("ERROR: --jobs must be >= 1", file=sys.stderr)
        sys.exit(2)
    if args.timeout < 1:
        print("ERROR: --timeout must be >= 1", file=sys.stderr)
        sys.exit(2)

    binary = args.binary.resolve()
    output_dir = args.output_dir.resolve()
    if not binary.exists():
        print(f"ERROR: binary not found at {binary}", file=sys.stderr)
        sys.exit(1)

    output_dir.mkdir(parents=True, exist_ok=True)
    nef_files = sorted(DEVPACK_DIR.glob("*.nef"))
    print(f"Decompiling {len(nef_files)} contracts with {args.jobs} worker(s)...")

    planned: list[tuple[str, Path, Path]] = []
    for nef_path in nef_files:
        name = nef_path.stem
        manifest_path = DEVPACK_DIR / f"{name}.manifest.json"
        if not manifest_path.exists():
            print(f"  SKIP: {name} (no manifest)")
            continue
        planned.append((name, nef_path, manifest_path))

    results: list[dict[str, Any]] = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as executor:
        future_to_name = {
            executor.submit(
                decompile_one,
                name,
                nef_path,
                manifest_path,
                binary=binary,
                output_dir=output_dir,
                timeout_seconds=args.timeout,
            ): name
            for name, nef_path, manifest_path in planned
        }
        for future in concurrent.futures.as_completed(future_to_name):
            name = future_to_name[future]
            try:
                result = future.result()
            except Exception as exc:  # pragma: no cover - defensive for batch scripts
                result = {
                    "name": name,
                    "errors": [f"worker failure: {exc}"],
                    "formats": {fmt: False for fmt, _ in FORMATS},
                }
            results.append(result)

    # Deterministic output order for stable diffs/CI artifacts.
    results.sort(key=lambda item: str(item["name"]))

    success = 0
    partial = 0
    failed = 0
    for result in results:
        all_ok = all(result["formats"].values())
        any_ok = any(result["formats"].values())
        if all_ok:
            success += 1
        elif any_ok:
            partial += 1
            print(f"  PARTIAL: {result['name']}: {result['errors']}")
        else:
            failed += 1
            print(f"  FAILED: {result['name']}: {result['errors']}")

    print(
        f"\nResults: {success} success, {partial} partial, {failed} "
        f"failed out of {len(results)}"
    )

    summary_path = output_dir / "summary.json"
    atomic_write_text(summary_path, json.dumps(results, indent=2))
    print(f"Summary: {summary_path}")

    errors = [result for result in results if result["errors"]]
    if errors:
        report_path = output_dir / "errors.txt"
        lines: list[str] = []
        for result in errors:
            lines.append(f"=== {result['name']} ===")
            for error in result["errors"]:
                lines.append(f"  {error}")
            lines.append("")
        atomic_write_text(report_path, "\n".join(lines))
        print(f"Error report: {report_path}")


if __name__ == "__main__":
    main()
