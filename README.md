# Neo N3 NEF Inspector

[![CI](https://github.com/r3e-network/neo-decompiler/actions/workflows/ci.yml/badge.svg)](https://github.com/r3e-network/neo-decompiler/actions/workflows/ci.yml)
[![docs.rs](https://img.shields.io/docsrs/neo-decompiler)](https://docs.rs/neo-decompiler)
[![License](https://img.shields.io/badge/license-MIT%20or%20Apache--2.0-blue.svg)](#license)

This project provides a small, well-tested Rust crate and CLI for inspecting
Neo N3 NEF bytecode packages. It focuses on the essential pieces that are easy
to run locally: parsing the NEF container (header, method tokens, checksum),
loading the companion contract manifest, decoding a useful slice of Neo VM
opcodes, and rendering both pseudocode and a high-level contract skeleton.

## What you get
- NEF header parsing (magic, compiler, version, script length, checksum)
- Script hash calculation (Hash160) exposed in both little-endian and canonical forms
- Method token decoding using the official variable-length encoding
- Opcode metadata generated from the upstream Neo VM source (unknown mnemonics
  fall back to informative comments, and new opcodes can be added via
  `tools/generate_opcodes.py`)
- Manifest parsing (`.manifest.json`) with ABI, feature, permission, and trust details that surface in both text and JSON outputs
- Disassembly for common opcodes such as `PUSH*`, arithmetic operations, jumps,
  calls, and `SYSCALL`
- Syscall metadata resolution with human-readable names and call flags
- Native contract lookup so method tokens can be paired with contract names
- High-level contract view that surfaces manifest ABI data and lifts stack
  operations into readable statements
- A simple pseudocode view mirroring the decoded instruction stream
- A single binary (`neo-decompiler`) and a reusable library (`neo_decompiler`)

## Quick start
```bash
# Build the binary
cargo build --release

# Print header information
./target/release/neo-decompiler info path/to/contract.nef

# Emit machine-readable header information (includes checksum, script hash, ABI, tokens, manifest path)
./target/release/neo-decompiler info --format json path/to/contract.nef

# Decode instructions
./target/release/neo-decompiler disasm path/to/contract.nef

# Machine-readable disassembly
./target/release/neo-decompiler disasm --format json path/to/contract.nef

# Emit the high-level contract view (auto-detects `*.manifest.json` if present)
./target/release/neo-decompiler decompile path/to/contract.nef

# Emit the legacy pseudocode listing
./target/release/neo-decompiler decompile --format pseudocode path/to/contract.nef

# Machine-readable decompilation (high-level, pseudocode, manifest path, metadata)
./target/release/neo-decompiler decompile --format json path/to/contract.nef

# Inspect method tokens
./target/release/neo-decompiler tokens path/to/contract.nef

# Machine-readable tokens listing
./target/release/neo-decompiler tokens --format json path/to/contract.nef

# Use --json-compact alongside any JSON format to minimise whitespace
./target/release/neo-decompiler info --format json --json-compact path/to/contract.nef
```

### Permissions example
Given a manifest snippet:

```json
{
  "permissions": [
    { "contract": { "hash": "0x0123..." }, "methods": ["symbol"] },
    { "contract": { "group": "03ABCD..." }, "methods": "*" }
  ],
  "trusts": "*"
}
```

The `info` command prints:

```
Permissions:
    - contract=hash:0x0123... methods=["symbol"]
    - contract=group:03ABCD... methods=*
Trusts: *
```

Corresponding JSON (truncated) mirrors the schema:

```json
"manifest": {
  "permissions": [
    {
      "contract": { "type": "Hash", "value": "0x0123..." },
      "methods": { "type": "Methods", "value": ["symbol"] }
    },
    {
      "contract": { "type": "Group", "value": "03ABCD..." },
      "methods": { "type": "Wildcard", "value": "*" }
    }
  ],
  "trusts": { "type": "Wildcard", "value": "*" }
}
```

## Worked example (nccs)

The repository ships with a minimal C# contract under
[`examples/hello_world`](examples/hello_world/HelloWorld.cs). You can compile it
with the official Neo C# compiler (`nccs`) and immediately feed the result into
the decompiler:

```bash
# Install the Neo compiler if you do not already have it
dotnet tool install -g Neo.Compiler.CSharp

# Compile the example contract
nccs compile \
  examples/hello_world/HelloWorld.cs \
  --nef build/HelloWorld.nef \
  --manifest build/HelloWorld.manifest.json

# Decompile (auto-detects the manifest sitting next to the NEF)
neo-decompiler decompile build/HelloWorld.nef
```

The `examples/README.md` file explains the walkthrough and can serve as a
starting point for your own experiments.

## Installation
```bash
# Install the latest commit from the main branch
cargo install --git https://github.com/r3e-network/neo-decompiler --locked

# Or install locally from a checkout
cargo install --path . --locked
```

## Library example
```rust
use neo_decompiler::Decompiler;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let decompiler = Decompiler::new();
    let result = decompiler.decompile_file_with_manifest(
        "contract.nef",
        Some("contract.manifest.json"),
    )?;

    println!("{} instructions", result.instructions.len());
    println!("{}", result.high_level);
    Ok(())
}
```

`contract.type` is `Hash` for explicit script hashes, `Group` for public-key groups,
and `Wildcard` when `*` is specified. `methods.type` mirrors the same wildcard vs list
semantics (e.g., `Methods` with `value: ["symbol"]`).

### Extending opcode coverage
The disassembler prints informative comments for opcodes that are not yet translated
(`// XXXX: <MNEMONIC> (not yet translated)`). To extend support, update
`tools/generate_opcodes.py` (which regenerates `src/opcodes_generated.rs`) and add
handling in `src/decompiler.rs`/`src/cli.rs` for any new instructions.

-## Scope and limitations
- NEF checksums are verified using the same double-SHA256 calculation employed
  by the official toolchain.  Files with mismatching checksums are rejected.
- The disassembler covers the opcodes exercised by our tests (including the
  various `PUSH*` forms, short/long jumps, calls, and `SYSCALL`). Unrecognised
  opcodes still produce informative comments so you can decide how to extend the
  decoder.
- The high-level contract view performs lightweight stack lifting (constants,
  arithmetic, simple returns, and syscalls) and annotates unsupported control
  flow. Complex reconstruction such as control-flow graphs or type inference is
  intentionally out of scope.

## Troubleshooting
- **"manifest not provided" in JSON/text output** – ensure the `.manifest.json`
  file sits next to the NEF or pass it explicitly via `--manifest path/to/file`.
- **Manifest path missing in text/JSON output** – both views show the detected
  path (look for `Manifest path:` in text, or `manifest_path` in JSON). If it is
  absent/`null`, pass `--manifest path/to/contract.manifest.json` explicitly.
- **Checksum mismatch errors** – the CLI re-computes the NEF hash; re-build the
  contract to regenerate the NEF or verify you are pointing at the correct file.
- **Unsupported opcode warnings** – the disassembler prints comments for
  unrecognised instructions; add tests and extend `opcodes_generated.rs` if you
  observe new opcodes.
- **Need structured data for scripting** – use the `--format json` variants
  (`info`, `disasm`, `tokens`, `decompile`) and add `--json-compact` when piping
  into tools that prefer minified payloads. If you need the manifest path or
  operand types, consume the structured fields exposed in the JSON report
  (`manifest_path`, `operand_kind`, `operand_value`).
- **Unexpected native-contract warnings** – the CLI resolves method tokens to
  native hashes; when you see `native contract ... does not expose method ...`,
  double-check the target contract name or regenerate the NEF to ensure the
  method token is valid. (These also appear in the JSON `warnings` array; other
  warning types may be added in the future.)

### JSON schema overview
Each `--format json` command emits a top-level object containing:
- `file`: Path to the NEF file being inspected.
- `manifest_path`: Optional path to the manifest file that was consumed.
- `warnings`: Array of human-readable warnings (currently populated when method
  tokens refer to unknown native methods).
- Command-specific payloads:
  - `info`: checksum, script hashes, `method_tokens` (with native annotations)
    and `manifest` summaries (methods, events, permissions, trusts).
  - `disasm`: `instructions` array with `offset`, `opcode`, `operand_kind`, and
    structured `operand_value`.
  - `decompile`: combines the disassembly, `high_level` text, `pseudocode`, and
    `method_tokens` into one report.
  - `tokens`: standalone `method_tokens` array for quick inspection.

Example (excerpt from `info --format json`):
```json
{
  "file": "path/to/contract.nef",
  "manifest_path": "path/to/contract.manifest.json",
  "manifest": {
    "permissions": [
      {
        "contract": { "type": "Hash", "value": "0x0123..." },
        "methods": { "type": "Methods", "value": ["symbol"] }
      }
    ],
    "trusts": { "type": "Contracts", "value": ["0x89ab..."] }
  },
  "method_tokens": [
    {
      "method": "Transfer",
      "native_contract": { "contract": "GasToken", "label": "GasToken::Transfer" }
    }
  ],
  "warnings": []
}
```

Formal schema files live under [`docs/schema`](docs/schema) for every JSON command
(`info.schema.json`, `disasm.schema.json`, `decompile.schema.json`, `tokens.schema.json`).
Schemas follow semantic versioning (breaking changes bump the major version); consumers
should pin to a known commit/tag when validating automation pipelines.

## Development
```bash
cargo fmt
cargo test
```

If you use [`just`](https://github.com/casey/just), the repository ships with a
`Justfile` providing shortcuts for the common workflows above.

Issues and pull requests are welcome if they keep the project lean and focused.

## Contributing
See [`CONTRIBUTING.md`](CONTRIBUTING.md) for development guidelines and
[`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md) for behavioural expectations.

## Support & Security
- Support channels are documented in [`SUPPORT.md`](SUPPORT.md).
- Responsible disclosure guidance lives in [`SECURITY.md`](SECURITY.md).

## Changelog
Recent project history is tracked in [`CHANGELOG.md`](CHANGELOG.md).

## Minimum supported Rust version
The crate is tested against Rust `1.70` and newer on CI. Older toolchains are
not guaranteed to work.

## License
Dual licensed under MIT or Apache-2.0.
See `LICENSE-MIT` and `LICENSE-APACHE` for details.
