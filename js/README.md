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
- **C#-style rendering** — manifest-aware signatures and readable C# source-oriented output
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
const withManifest = decompileHighLevelBytesWithManifest(nefBytes, manifest);
console.log(withManifest.highLevel);

// Full analysis: call graph, method contracts, xrefs, types
const analysis = analyzeBytes(nefBytes, manifestJson);
console.log(analysis.callGraph);
console.log(analysis.methodContracts);
console.log(analysis.xrefs);
console.log(analysis.types);
console.log(analysis.patterns);

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

### `decompileHighLevelBytes(bytes, options?) → { ..., highLevel, csharp, methodContracts, patterns }`

Full decompilation to structured pseudocode (if/else, loops, etc.).

`options`:
- `clean: true` — convenience shorthand for the maximum-readability mode.
  Inlines single-use temporaries and strips informational comments
  (`// declare N locals, M arguments`, etc.). Recommended when consuming
  the high-level output as source code.
- `inlineSingleUseTemps: true` — replace every single-use `tN` with its
  RHS at the use site. Implied by `clean: true`.
- `typedDeclarations` — use conservative inferred C# declaration types (on by
  default) for obvious literals, arithmetic, and collection helpers. Set it to
  `false` for compatibility-oriented `var` declarations. Unresolved or
  conflicting values remain `dynamic`; catalog-known syscall results use their
  VM-normalized C# types, while unknown syscall hashes remain `dynamic`.
- `failOnUnknownOpcodes: true` — error rather than emitting `UNKNOWN_0xNN`
  for opcodes the disassembler does not recognise.

### `decompileHighLevelBytesWithManifest(bytes, manifest, options?) → { ..., highLevel, csharp, methodContracts, patterns }`

Same as above but with manifest-driven method signatures. Accepts the
same `options` object.

`csharp` is a readable C#-style view of the lifted body. VM-specific
expressions remain visible when they do not have a direct C# translation; the
field is source-oriented and is not a guarantee of framework compilation.
When produced by the high-level APIs, its contract header also records the
inferred standards, behavior patterns, supported C# target hint, and confidence
from `patterns`. Direct `renderCSharpContract` callers can pass that
`PatternInfo` as its optional fourth argument.
The JavaScript port deliberately targets C# only; pseudocode and high-level
text remain intermediate analysis views rather than alternate language output.
Runtime-variable stack rearrangements such as `PICK` and `ROLL` lower to
`default(dynamic)` with an inline explanation, and transfers to labels that
are missing or outside the current method become comments. Valid labels remain
ordinary C# `goto` targets.

### `analyzeBytes(bytes, manifest?) → { ..., callGraph, methodContracts, xrefs, types, patterns, methodGroups }`

Full analysis with call graph, deterministic method stack-call contracts,
cross-references, and type inference. Each method contract reports
`argumentCount` and a tri-state `returnBehavior` (`value`, `void`, or
`unknown`); unknown methods remain conservatively value-producing while
lifting calls.

`patterns` reports declared or inferred standards, behavior patterns such as
`storage`, `storage_reads`, `storage_writes`, `storage_deletes`,
`storage_iteration`, `iterator_usage`, `runtime_context`, `account_creation`,
`gas_management`, `cryptography`, `serialization`, `string_operations`,
`memory_operations`, `blockchain_queries`, `native_token_calls`,
`notifications`, `events`, `ownership`, `royalties`, and native
contract calls (including `oracle`, `governance`, `role_management`,
`policy_management`, `token_management`, `ledger`, `notary`, `treasury`,
`contract_management`, and `upgradeable`),
the supported C# compiler/source metadata hint,
an aggregate confidence, and the evidence signals behind each result. Generated
source output is intentionally focused on readable Neo C# contracts; the target
hint is secondary analysis only. Manifest standards are high confidence; bytecode-only hints remain conservative. ABI
method signatures can infer NEP-17, NEP-11, and NEP-24 (`royaltyInfo`) when
the manifest does not declare a standard.

When the pinned v3.10.0 corpus and Neo framework assembly are available, the
optional Roslyn gate verifies every generated JS contract:

```bash
NEO_CSHARP_CORPUS_DIR=/path/to/devpack-artifacts-v3.10.0 \
NEO_SMARTCONTRACT_FRAMEWORK_DLL=/path/to/Neo.SmartContract.Framework.dll \
NEO_CSHARP_TARGET_FRAMEWORK=net10.0 \
npm test -- --test-name-pattern "pinned JS-generated C# corpus"
```

Without those environment variables the gate is skipped; the ordinary JS
suite remains self-contained.

### `parseManifest(json) → { name, abi, ... }`

Parse a Neo N3 contract manifest JSON.

## Output Example

```
contract MyContract {
    fn transfer(from: hash160, to: hash160, amount: int) -> bool {
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
| 1.6.0             | 0.11.0                |
| 1.5.4             | 0.10.2                |
| 1.5.3             | 0.10.1                |
| 1.5.2             | 0.8.2                 |
| 1.5.1             | 0.8.1                 |
| 1.5.0             | 0.8.0                 |
| 1.4.0             | 0.7.0                 |
| 1.3.0             | 0.6.3                 |
| 1.2.1             | 0.6.2                 |
| 1.2.0             | 0.6.2                 |
| 1.1.1             | 0.6.1                 |
| 1.1.0             | 0.6.1                 |
| 1.0.x             | 0.6.0                 |

## License

MIT OR Apache-2.0
