from __future__ import annotations

import contextlib
import hashlib
import importlib.util
import io
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT_PATH = Path(__file__).resolve().parents[1] / "extract_devpack_artifacts.py"
SPEC = importlib.util.spec_from_file_location("extract_devpack_artifacts", SCRIPT_PATH)
assert SPEC is not None and SPEC.loader is not None
extractor = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = extractor
SPEC.loader.exec_module(extractor)


def write_artifact(devpack_root: Path, name: str, *, valid: bool = True) -> None:
    artifacts = (
        devpack_root
        / "tests"
        / "Neo.Compiler.CSharp.UnitTests"
        / "TestingArtifacts"
    )
    contracts = devpack_root / "tests" / "Neo.Compiler.CSharp.TestContracts"
    artifacts.mkdir(parents=True, exist_ok=True)
    contracts.mkdir(parents=True, exist_ok=True)
    if valid:
        embedded = (
            'Convert.FromBase64String(@"AAE=");\n'
            f'ContractManifest.Parse(@"{{""name"":""{name}""}}");\n'
        )
    else:
        embedded = "// missing embedded compiler artifact\n"
    (artifacts / f"{name}.cs").write_text(embedded, encoding="utf-8")
    (contracts / f"{name}.cs").write_text(
        f"public class {name} {{}}\n",
        encoding="utf-8",
    )


def output_digest(output_dir: Path) -> str:
    digest = hashlib.sha256()
    for path in sorted(path for path in output_dir.rglob("*") if path.is_file()):
        digest.update(path.relative_to(output_dir).as_posix().encode())
        digest.update(path.read_bytes())
    return digest.hexdigest()


class ExtractDevpackArtifactsTests(unittest.TestCase):
    def run_extractor(self, *arguments: str) -> int:
        args = extractor.parse_args(arguments)
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(
            io.StringIO()
        ):
            return extractor.run(args)

    def test_strict_clean_output_is_reproducible_and_records_git_revision(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            devpack = root / "devpack"
            output = root / "output"
            write_artifact(devpack, "Beta")
            write_artifact(devpack, "Alpha")

            subprocess.run(["git", "init", "-q", str(devpack)], check=True)
            subprocess.run(
                ["git", "-C", str(devpack), "config", "user.email", "test@example.com"],
                check=True,
            )
            subprocess.run(
                ["git", "-C", str(devpack), "config", "user.name", "Extractor Test"],
                check=True,
            )
            subprocess.run(["git", "-C", str(devpack), "add", "."], check=True)
            subprocess.run(
                ["git", "-C", str(devpack), "commit", "-q", "-m", "fixture"],
                check=True,
            )
            subprocess.run(
                ["git", "-C", str(devpack), "tag", "v3.10.0"],
                check=True,
            )
            commit = subprocess.run(
                ["git", "-C", str(devpack), "rev-parse", "HEAD"],
                check=True,
                capture_output=True,
                text=True,
            ).stdout.strip()

            arguments = (
                "--devpack-root",
                str(devpack),
                "--output-dir",
                str(output),
                "--expected-count",
                "2",
                "--strict",
                "--clean",
            )
            self.assertEqual(self.run_extractor(*arguments), 0)
            first_digest = output_digest(output)
            self.assertEqual(self.run_extractor(*arguments), 0)
            self.assertEqual(output_digest(output), first_digest)

            provenance = json.loads((output / "provenance.json").read_text())
            self.assertEqual(provenance["source"]["commit"], commit)
            self.assertEqual(provenance["source"]["tags"], ["v3.10.0"])
            self.assertEqual(provenance["artifacts"]["names"], ["Alpha", "Beta"])
            self.assertEqual(provenance["artifacts"]["extracted"], 2)
            self.assertEqual((output / "Alpha.nef").read_bytes(), b"\x00\x01")
            self.assertTrue((output / "sources" / "Beta.cs").is_file())

    def test_strict_omission_fails_before_cleaning_existing_output(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            devpack = root / "devpack"
            output = root / "output"
            write_artifact(devpack, "Valid")
            write_artifact(devpack, "Broken", valid=False)
            output.mkdir()
            sentinel = output / "sentinel.txt"
            sentinel.write_text("keep", encoding="utf-8")

            result = self.run_extractor(
                "--devpack-root",
                str(devpack),
                "--output-dir",
                str(output),
                "--expected-count",
                "2",
                "--strict",
                "--clean",
            )

            self.assertEqual(result, 1)
            self.assertEqual(sentinel.read_text(encoding="utf-8"), "keep")
            self.assertFalse((output / "Valid.nef").exists())

    def test_strict_mode_rejects_stale_generated_pairs_without_clean(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            devpack = root / "devpack"
            output = root / "output"
            write_artifact(devpack, "Valid")
            output.mkdir()
            (output / "Stale.nef").write_bytes(b"stale")

            result = self.run_extractor(
                "--devpack-root",
                str(devpack),
                "--output-dir",
                str(output),
                "--expected-count",
                "1",
                "--strict",
            )

            self.assertEqual(result, 1)
            self.assertFalse((output / "Valid.nef").exists())

    def test_expected_count_defaults_to_pinned_corpus_size(self) -> None:
        self.assertEqual(extractor.parse_args([]).expected_count, 103)


if __name__ == "__main__":
    unittest.main()
