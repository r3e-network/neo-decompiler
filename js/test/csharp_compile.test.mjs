import assert from "node:assert/strict";
import {
  readFileSync,
  readdirSync,
  mkdtempSync,
  writeFileSync,
  rmSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";

import { decompileHighLevelBytesWithManifest } from "../src/index.js";

const PINNED_DEVPACK_COMMIT = "5b0b63880b6201ae3f974cc845e93a90462d8043";
const corpus = process.env.NEO_CSHARP_CORPUS_DIR;
const framework = process.env.NEO_SMARTCONTRACT_FRAMEWORK_DLL;
const skipReason = !corpus
  ? "requires NEO_CSHARP_CORPUS_DIR"
  : !framework
    ? "requires NEO_SMARTCONTRACT_FRAMEWORK_DLL"
    : false;

test("pinned JS-generated C# corpus compiles with Roslyn", { skip: skipReason }, () => {
  const targetFramework = process.env.NEO_CSHARP_TARGET_FRAMEWORK ?? "net8.0";
  assert.match(
    targetFramework,
    /^net[0-9A-Za-z.]+$/,
    "NEO_CSHARP_TARGET_FRAMEWORK must be a target moniker such as net8.0",
  );
  assert.ok(framework && framework.endsWith(".dll"), "framework assembly path must be a DLL");

  const provenance = JSON.parse(readFileSync(join(corpus, "provenance.json"), "utf8"));
  assert.equal(
    provenance?.source?.commit,
    PINNED_DEVPACK_COMMIT,
    "JS C# corpus must use the pinned neo-devpack-dotnet revision",
  );

  const nefFiles = readdirSync(corpus)
    .filter((name) => name.endsWith(".nef"))
    .sort();
  assert.equal(nefFiles.length, 103, "pinned JS C# corpus count drift");

  const project = mkdtempSync(join(tmpdir(), "neo-js-csharp-corpus-"));
  try {
    writeFileSync(
      join(project, "Generated.csproj"),
      `<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>${targetFramework}</TargetFramework><Nullable>disable</Nullable><ImplicitUsings>disable</ImplicitUsings></PropertyGroup><ItemGroup><Reference Include="Neo.SmartContract.Framework"><HintPath>${escapeXml(framework)}</HintPath></Reference></ItemGroup></Project>`,
    );
    const restore = runDotnet(project, ["restore", "--nologo", "--verbosity", "quiet"]);
    assert.equal(restore.status, 0, `Roslyn restore failed:\n${outputText(restore)}`);

    const failures = [];
    const tryFallbacks = [];
    for (const nefName of nefFiles) {
      const manifestName = nefName.replace(/\.nef$/, ".manifest.json");
      const manifest = JSON.parse(readFileSync(join(corpus, manifestName), "utf8"));
      const decompiled = decompileHighLevelBytesWithManifest(
        readFileSync(join(corpus, nefName)),
        manifest,
      );
      const source = decompiled.csharp;
      const hazardWarnings = decompiled.warnings.filter((warning) =>
        /TRY(?:_L)? \(not yet translated\)/u.test(warning),
      );
      if (hazardWarnings.length > 0) {
        tryFallbacks.push({
          nefName,
          warnings: hazardWarnings,
        });
      }
      writeFileSync(join(project, "Generated.cs"), source);
      const build = runDotnet(project, [
        "build",
        "--no-restore",
        "--no-incremental",
        "--nologo",
        "--verbosity",
        "quiet",
      ]);
      if (build.status !== 0) {
        failures.push({
          nefName,
          diagnostics: outputText(build)
            .split(/\r?\n/)
            .filter((line) => /error CS/.test(line))
            .slice(0, 8),
        });
      }
    }
    assert.deepEqual(failures, [], `Roslyn rejected JS-generated contracts: ${JSON.stringify(failures)}`);
    assert.deepEqual(
      tryFallbacks,
      [],
      `JS high-level corpus still contains untranslated TRY regions: ${JSON.stringify(tryFallbacks)}`,
    );
  } finally {
    rmSync(project, { recursive: true, force: true });
  }
});

function runDotnet(cwd, args) {
  return spawnSync("dotnet", args, { cwd, encoding: "utf8" });
}

function outputText(result) {
  return `${result.stdout ?? ""}\n${result.stderr ?? ""}`;
}

function escapeXml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll('"', "&quot;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;");
}
