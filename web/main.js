import {
  decompileReport,
  disasmReport,
  infoReport,
  init,
  initPanicHook,
} from "./dist/index.js";

const controls = document.querySelector("#controls");
const nefFileInput = document.querySelector("#nef-file");
const manifestFileInput = document.querySelector("#manifest-file");
const outputFormatInput = document.querySelector("#output-format");
const strictManifestInput = document.querySelector("#strict-manifest");
const failUnknownInput = document.querySelector("#fail-unknown");
const inlineTempsInput = document.querySelector("#inline-temps");
const runButton = document.querySelector("#run");
const status = document.querySelector("#status");
const infoSummary = document.querySelector("#info-summary");
const disasmSummary = document.querySelector("#disasm-summary");
const decompileSummary = document.querySelector("#decompile-summary");
const infoOutput = document.querySelector("#info-output");
const disasmOutput = document.querySelector("#disasm-output");
const decompileOutput = document.querySelector("#decompile-output");

let wasmReady = false;

boot();

async function boot() {
  try {
    await init();
    initPanicHook();
    wasmReady = true;
    status.textContent = "Wasm package loaded. Choose a .nef file to begin.";
  } catch (error) {
    wasmReady = false;
    status.innerHTML =
      "Could not load <code>./pkg/neo_decompiler.js</code>. Run <code>npm run build:wasm</code> first.";
    console.error(error);
  }
}

controls.addEventListener("submit", async (event) => {
  event.preventDefault();
  if (!wasmReady) {
    status.innerHTML =
      "The wasm package is not ready yet. Build it with <code>npm run build:wasm</code>.";
    return;
  }

  const nefFile = nefFileInput.files?.[0];
  if (!nefFile) {
    status.textContent = "Choose a .nef file first.";
    return;
  }

  runButton.disabled = true;
  status.textContent = "Loading files and running the Rust decompiler...";

  try {
    const nefBytes = new Uint8Array(await nefFile.arrayBuffer());
    const manifestJson = await readOptionalText(manifestFileInput.files?.[0]);

    const info = infoReport(nefBytes, {
      manifestJson,
      strictManifest: strictManifestInput.checked,
    });
    const disasm = disasmReport(nefBytes, {
      failOnUnknownOpcodes: failUnknownInput.checked,
    });
    const decompile = decompileReport(nefBytes, {
      manifestJson,
      strictManifest: strictManifestInput.checked,
      failOnUnknownOpcodes: failUnknownInput.checked,
      inlineSingleUseTemps: inlineTempsInput.checked,
      outputFormat: outputFormatInput.value,
    });

    infoSummary.textContent = `${info.script_hash_be} • ${info.script_length} bytes`;
    disasmSummary.textContent = `${disasm.instructions.length} instructions • ${disasm.warnings.length} warnings`;
    decompileSummary.textContent =
      `${decompile.analysis.call_graph.methods.length} methods • ${decompile.warnings.length} warnings`;

    infoOutput.textContent = JSON.stringify(info, null, 2);
    disasmOutput.textContent = JSON.stringify(disasm, null, 2);

    const rendered = [
      decompile.high_level && ["// High-level", decompile.high_level],
      decompile.pseudocode && ["// Pseudocode", decompile.pseudocode],
      decompile.csharp && ["// C#", decompile.csharp],
      decompile.warnings.length
        ? ["// Warnings", decompile.warnings.map((warning) => `- ${warning}`).join("\n")]
        : null,
    ]
      .filter(Boolean)
      .map(([title, body]) => `${title}\n${body}`)
      .join("\n\n");

    decompileOutput.textContent = rendered || JSON.stringify(decompile, null, 2);
    status.textContent = "Analysis complete.";
  } catch (error) {
    console.error(error);
    status.textContent = `Analysis failed: ${String(error)}`;
    decompileOutput.textContent = String(error);
  } finally {
    runButton.disabled = false;
  }
});

async function readOptionalText(file) {
  if (!file) {
    return undefined;
  }
  return await file.text();
}
