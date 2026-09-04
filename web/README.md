# Neo Decompiler Web Package

This folder contains the publishable npm package for the Rust crate compiled to
WebAssembly. It adds a thin TypeScript wrapper over the wasm bindings so browser
code gets a stable, typed API without duplicating the decompiler logic.

## Build

From this directory:

```bash
npm install
npm run build:wasm
```

That runs `wasm-pack build .. --mode no-install --target web --out-dir web/dist/pkg --features web --no-default-features --locked`
followed by `scripts/prepare-wasm-package.mjs`, generating the wasm glue under
`web/dist/pkg/`.

Then build the TypeScript wrapper:

```bash
npm run build:ts
```

For a full package build:

```bash
npm run build:package
npm run test:wasm
```

Install `wasm-pack` 0.13.1 and the `wasm-bindgen-cli` version in the root
`Cargo.lock` before building. The CLI's locked dependencies need Rust 1.88 or
newer even though the library builds with Rust 1.86. `test:wasm` requires the
built artifacts and runs all report APIs against the actual WebAssembly module;
`npm test` only needs the TypeScript compiler and runs the ordinary unit suite.

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
  typedDeclarations: true,
  outputFormat: "csharp",
});
```

The decompile report defaults to the generated C# contract. Pass `"all"` when
the intermediate high-level and pseudocode views are also needed. Set
`typedDeclarations: false` to retain compatibility-oriented dynamic/var
declarations.

`nefBytes` should be a `Uint8Array`. `manifestJson` should be a UTF-8 JSON string.

The wrapper accepts camelCase JS options and translates them into the snake_case
ABI expected by the wasm bindings. The published npm tarball includes the
compiled TypeScript wrapper plus the wasm artifacts under `dist/pkg/`.

Report lengths and offsets are JavaScript numbers. Manifest `features` and
`extra` are ordinary objects, including nested records; metadata keys such as
`__proto__` remain own data properties without changing object prototypes.

Wide `I64` operand values are returned as JavaScript `bigint`. When displaying a
report as JSON, convert those values to decimal strings in a replacer; plain
`JSON.stringify` cannot serialize them. The included browser demo does this
without losing precision.
