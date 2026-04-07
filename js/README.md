# neo-decompiler-js

Pure JavaScript Neo N3 smart contract decompiler. Parse NEF files, disassemble bytecode, and decompile to human-readable pseudocode. Zero dependencies.

## Install

```bash
npm install neo-decompiler-js
```

## Features

- **NEF parsing** — validate magic, checksum, method tokens
- **Disassembly** — full Neo VM opcode coverage
- **High-level decompilation** — structured pseudocode with if/else, loops, try/catch, switch
- **Post-processing** — 18 optimization passes (else-if chains, compound assignments, for-loops, indexing syntax, overflow collapse, and more)
- **Call graph** — internal calls, CALLT tokens, SYSCALL, indirect CALLA
- **Cross-references** — slot read/write tracking
- **Type inference** — basic collection and primitive type detection
- **Manifest support** — ABI method signatures, parameter names, return types
- **Zero dependencies** — pure ESM, works in Node.js 18+, Deno, Bun (uses `node:crypto` for checksum verification)

## Usage

```js
import {
  parseNef,
  disassembleScript,
  decompileBytes,
  decompileHighLevelBytes,
  decompileHighLevelBytesWithManifest,
  analyzeBytes,
  parseManifest,
} from "neo-decompiler-js";

// Basic: parse and decompile
const result = decompileHighLevelBytes(nefBytes);
console.log(result.highLevel);

// With manifest for better output
const manifest = parseManifest(manifestJson);
const result = decompileHighLevelBytesWithManifest(nefBytes, manifest);
console.log(result.highLevel);

// Full analysis: call graph, xrefs, types
const analysis = analyzeBytes(nefBytes, manifestJson);
console.log(analysis.callGraph);
console.log(analysis.xrefs);
console.log(analysis.types);

// Step by step
const nef = parseNef(nefBytes);
const disasm = disassembleScript(nef.script);
console.log(disasm.instructions);
```

## API

### `parseNef(bytes) → { script, header, methodTokens, scriptHash, scriptHashLE, ... }`

Parse a NEF container. Throws on invalid magic or checksum mismatch. Returns the script hash in both big-endian (`scriptHash`) and little-endian (`scriptHashLE`) hex.

### `disassembleScript(script) → { instructions, warnings }`

Disassemble a bytecode array into instruction objects.

### `decompileBytes(bytes) → { nef, instructions, warnings, pseudocode }`

Parse and disassemble. Returns simple pseudocode listing.

### `decompileBytesWithManifest(bytes, manifest) → { ..., methodGroups, groupedPseudocode }`

Parse, disassemble, and group methods using manifest ABI info. Returns grouped pseudocode.

### `decompileHighLevelBytes(bytes) → { ..., highLevel }`

Full decompilation to structured pseudocode (if/else, loops, etc.).

### `decompileHighLevelBytesWithManifest(bytes, manifest) → { ..., highLevel }`

Same as above but with manifest-driven method signatures.

### `analyzeBytes(bytes, manifest?) → { ..., callGraph, xrefs, types, methodGroups }`

Full analysis with call graph, cross-references, and type inference.

### `parseManifest(json) → { name, abi, ... }`

Parse a Neo N3 contract manifest JSON.

## Output Example

```
contract MyContract {
    fn transfer(from: Hash160, to: Hash160, amount: Integer) -> Boolean {
        if from != sender() {
            if !verify_signature(from) {
                return false;
            }
        }
        for (let i = 0; i < 3; i += 1) {
            balances[from] = balances[from] - amount;
        }
        return true;
    }
}
```

## Tests

```bash
npm test
```

## Version Mapping

| neo-decompiler-js | neo-decompiler (Rust) |
|-------------------|-----------------------|
| 1.2.0             | 0.6.2                 |
| 1.1.1             | 0.6.1                 |
| 1.1.0             | 0.6.1                 |
| 1.0.x             | 0.6.0                 |

## License

MIT OR Apache-2.0
