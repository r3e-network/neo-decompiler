# Neo Decompiler Web Package

This folder contains the publishable npm package for the Rust crate compiled to
WebAssembly. It adds a thin TypeScript wrapper over the wasm bindings so browser
code gets a stable, typed API without duplicating the decompiler logic.

## Build

From the repository root:

```bash
wasm-pack build --target web --out-dir web/pkg --features web --no-default-features
```

That generates the wasm glue under `web/dist/pkg/`.

Then build the TypeScript wrapper:

```bash
npm install
npm run build:ts
```

For a full package build:

```bash
npm run build:package
```

## Serve

From this directory:

```bash
python3 -m http.server 4173
```

Then open `http://localhost:4173`.

## JS API

```js
import {
  init,
  initPanicHook,
  infoReport,
  disasmReport,
  decompileReport,
} from "neo-decompiler-web";

await init();
initPanicHook();

const info = infoReport(nefBytes, {
  manifestJson,
  strictManifest: false,
});

const disasm = disasmReport(nefBytes, {
  failOnUnknownOpcodes: false,
});

const decompile = decompileReport(nefBytes, {
  manifestJson,
  strictManifest: false,
  failOnUnknownOpcodes: false,
  inlineSingleUseTemps: true,
  outputFormat: "all",
});
```

`nefBytes` should be a `Uint8Array`. `manifestJson` should be a UTF-8 JSON string.

The wrapper accepts camelCase JS options and translates them into the snake_case
ABI expected by the wasm bindings. The published npm tarball includes the
compiled TypeScript wrapper plus the wasm artifacts under `dist/pkg/`.
