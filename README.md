# Neo N3 NEF Inspector

<p align="center">
  <img src="docs/logo.svg" alt="Neo Decompiler logo" width="180" />
</p>

[![CI](https://github.com/r3e-network/neo-decompiler/actions/workflows/ci.yml/badge.svg)](https://github.com/r3e-network/neo-decompiler/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/neo-decompiler.svg)](https://crates.io/crates/neo-decompiler)
[![docs.rs](https://img.shields.io/docsrs/neo-decompiler)](https://docs.rs/neo-decompiler)
[![License](https://img.shields.io/badge/license-MIT%20or%20Apache--2.0-blue.svg)](#license)
[![GitHub release](https://img.shields.io/github/v/release/r3e-network/neo-decompiler)](https://github.com/r3e-network/neo-decompiler/releases/latest)

This project provides a small, well-tested Rust crate and CLI for inspecting
Neo N3 NEF bytecode packages. It focuses on the essential pieces that are easy
to run locally: parsing the NEF container (header, method tokens, checksum),
loading the companion contract manifest, decoding a useful slice of Neo VM
opcodes, and rendering both pseudocode and a high-level contract skeleton.

## Supported Features

### Binary Format Analysis

| Feature                 | Status | Description                                                        |
| ----------------------- | ------ | ------------------------------------------------------------------ |
| NEF Container Parsing   | ✅     | Magic bytes, compiler metadata, source hash, checksum verification |
| Script Hash Calculation | ✅     | Hash160 (RIPEMD160(SHA256)) in little-endian and canonical forms   |
| Method Token Decoding   | ✅     | Variable-length encoding with call flag validation                 |
| Checksum Verification   | ✅     | Double SHA-256 integrity check per Neo specification               |
| Manifest Parsing        | ✅     | Full ABI, permissions, trusts, groups, features extraction         |

### Disassembly Engine

| Feature                  | Status | Description                                                         |
| ------------------------ | ------ | ------------------------------------------------------------------- |
| Linear Sweep Disassembly | ✅     | Sequential instruction decoding with operand extraction             |
| Opcode Coverage          | ✅     | 160+ Neo VM opcodes from upstream `OpCode.cs`                       |
| Operand Decoding         | ✅     | I8/I16/I32/I64, variable-length bytes, jump targets, syscall hashes |
| Unknown Opcode Handling  | ✅     | Tolerant mode (comments) or strict mode (fail-fast)                 |
| Syscall Resolution       | ✅     | All published syscalls with handler names, prices, call flags      |
| Native Contract Binding  | ✅     | Legacy + latest native contracts (GasToken/Governance/etc.)        |

### Decompilation Pipeline

| Feature                       | Status | Description                                         |
| ----------------------------- | ------ | --------------------------------------------------- |
| Stack Simulation              | ✅     | Abstract interpretation of stack operations         |
| Expression Building           | ✅     | Stack values lifted to infix expressions            |
| Temporary Variable Generation | ✅     | Automatic naming for intermediate values            |
| Constant Propagation          | ✅     | Literal tracking through stack operations           |
| Void Syscall Detection        | ✅     | Suppress phantom return values for known void calls |

### Control Flow Recovery

| Feature              | Status | Description                                       |
| -------------------- | ------ | ------------------------------------------------- |
| Conditional Branches | ✅     | `if`/`else` block reconstruction                  |
| Pre-test Loops       | ✅     | `while` loop pattern detection                    |
| Post-test Loops      | ✅     | `do { } while` loop pattern detection             |
| Counting Loops       | ✅     | `for` loop reconstruction with iterator detection |
| Loop Exit Statements | ✅     | `break`/`continue` emission at correct scope      |
| Exception Handling   | ✅     | `try`/`catch`/`finally` block reconstruction      |
| Jump Target Analysis | ✅     | Forward/backward branch classification            |

### Slot & Variable Analysis

| Feature                  | Status | Description                                         |
| ------------------------ | ------ | --------------------------------------------------- |
| Local Slot Tracking      | ✅     | `STLOC`/`LDLOC` mapped to `local_N` identifiers     |
| Argument Slot Tracking   | ✅     | `STARG`/`LDARG` mapped to `arg_N` or manifest names |
| Static Slot Tracking     | ✅     | `STSFLD`/`LDSFLD` mapped to `static_N` identifiers  |
| Manifest Parameter Names | ✅     | ABI parameter names applied to argument slots       |
| Initialization Detection | ✅     | First-write tracking for declaration placement      |

### Output Formats

| Feature         | Status | Description                                                |
| --------------- | ------ | ---------------------------------------------------------- |
| Pseudocode      | ✅     | Linear instruction listing with resolved operands          |
| High-Level View | ✅     | Structured code with control flow and expressions          |
| C# Skeleton     | ✅     | Compilable stub with attributes, events, method signatures |
| JSON Reports    | ✅     | Machine-readable output with JSON Schema validation        |
| Text Reports    | ✅     | Human-readable formatted output                            |

### Security & Robustness

| Feature                     | Status | Description                                                 |
| --------------------------- | ------ | ----------------------------------------------------------- |
| Input Size Limits           | ✅     | 10 MiB NEF limit, 1 MiB manifest limit, 1 MiB operand limit |
| Integer Overflow Protection | ✅     | Checked arithmetic in slice operations                      |
| Malformed Input Handling    | ✅     | Graceful error reporting, no panics                         |
| Fuzz Testing                | ✅     | cargo-fuzz targets for parser and disassembler              |

---

## Roadmap

### Shipped Features (v0.3.x)

| Feature                   | Status | Description                                                                 |
| ------------------------- | ------ | --------------------------------------------------------------------------- |
| Control Flow Graph (CFG)  | ✅     | Explicit basic block graph with edges + DOT export for visualization        |
| Else-If Chain Detection   | ✅     | Collapse nested `if`/`else` into `else if` chains                           |
| Dead Code Detection       | ✅     | Identify unreachable basic blocks via CFG reachability analysis             |
| Expression Simplification | ✅     | Algebraic simplification helpers (e.g., `x + 0` → `x`) in `decompiler::ir`  |
| Inline Expansion          | ✅     | Conservative temp inlining for loop conditions/increments (opt-in for more) |

### Shipped Features (v0.4.x)

| Feature                   | Status | Description                                                                          |
| ------------------------- | ------ | ------------------------------------------------------------------------------------ |
| Type Inference            | ✅     | Best-effort primitive/collection type recovery for locals/args/statics               |
| Array/Map Type Recovery   | ✅     | Detect collection kinds from `PACK`/`NEWARRAY`/`NEWMAP` and emit bracket indexing    |
| Call Graph Construction   | ✅     | Extract inter-procedural relationships (`CALL*`, `CALLT`, `SYSCALL`)                 |
| Cross-Reference Analysis  | ✅     | Track local/argument/static slot reads and writes by bytecode offset                 |
| Switch Statement Recovery | ✅     | Rewrite equality-based `if`/`else` chains into `switch`/`case` blocks (conservative) |

### Shipped Features (v0.5.x)

| Feature                    | Status | Description                                                                 |
| -------------------------- | ------ | --------------------------------------------------------------------------- |
| SSA Transformation         | ✅     | Static Single Assignment form with φ nodes and variable versions            |
| Dominance Analysis         | ✅     | Immediate dominators, dominator tree, dominance frontiers                   |
| SSA Rendering              | ✅     | Human-readable SSA output with statistics (blocks, φ nodes, vars)           |
| Strict Manifest Validation | ✅     | Global `--strict-manifest` flag plus strict manifest parser APIs            |
| Entry-Offset Safety        | ✅     | Synthetic script-entry emission when ABI method offsets don't match entry    |
| Disassembly Fast Path      | ✅     | `disasm` command decodes instruction streams without full decompile analysis |

### Planned Features (v0.6.x+)

| Feature               | Priority | Description                                           |
| --------------------- | -------- | ----------------------------------------------------- |
| Data Flow Analysis    | Medium   | Reaching definitions, live variable analysis          |
| SSA Optimizations     | Medium   | Constant propagation, dead code elimination using SSA |
| Struct/Class Recovery | Low      | Infer composite types from field access patterns      |
| Deobfuscation Passes  | Low      | Detect and simplify common obfuscation patterns       |
| Interactive Mode      | Low      | REPL for exploratory analysis                         |
| Plugin Architecture   | Low      | User-defined analysis passes                          |

### Not Planned (Out of Scope)

| Feature                         | Reason                                                         |
| ------------------------------- | -------------------------------------------------------------- |
| Full Type System Reconstruction | Requires source-level type information not present in bytecode |
| Automatic Variable Naming       | Semantic naming requires ML/heuristics beyond current scope    |
| Source-Level Debugging          | Would require debug symbol format specification                |
| Contract Modification/Patching  | Tool is read-only by design                                    |

---

## What you get

- NEF header parsing (magic, compiler, version, script length, checksum)
- Script hash calculation (Hash160) exposed in both little-endian and canonical forms
- Method token decoding using the official variable-length encoding
- Opcode metadata generated from the upstream Neo VM source (unknown mnemonics
  fall back to informative comments, and new opcodes can be added via
  `tools/generate_opcodes.py`)
- Manifest parsing (`.manifest.json`) with ABI, feature, group, permission, and trust details that surface in both text and JSON outputs, with optional strict wildcard canonical checks via `--strict-manifest`
- Disassembly for common opcodes such as `PUSH*`, arithmetic operations, jumps,
  calls, and `SYSCALL` (tolerant by default; optional fail-fast flag for unknown
  opcodes), with `disasm` taking a direct decode path that skips full decompilation analysis
- Syscall metadata resolution with human-readable names, call flags, and return
  arity (void syscalls avoid phantom temporaries in the high-level view)
- Native contract lookup so method tokens can be paired with contract names
- High-level contract view that surfaces manifest ABI data, names locals/args
  via slot instructions (including manifest parameter names), and lifts stack
  operations into readable statements with structured `if`/`else`, `for`,
  `while`, `do { } while`, and `try`/`catch`/`finally` blocks plus emitted `break`/`continue`
  statements and manifest-derived signatures with consistent indentation and small readability passes
  like compound assignments and optional single-use temp inlining. When ABI offsets are present,
  each manifest method is decompiled within its own offset range; methods
  without offsets are still emitted as stubs for completeness, and script
  entry bytecode is preserved via a synthetic `script_entry`/`ScriptEntry`
  method when ABI offsets do not align with the actual script entry.
- Control Flow Graph (CFG) construction with DOT export (`Decompilation::cfg_to_dot`) and
  reachability helpers for dead-code detection (`Cfg::unreachable_blocks`)
- SSA (Static Single Assignment) transformation via `cfg.to_ssa()` or `Decompilation::compute_ssa()`:
  - Dominance analysis (immediate dominators, dominator tree, dominance frontiers)
  - φ node placement at control flow merge points
  - SSA form rendering with variable versions and statistics
- Best-effort analysis output in both the library and JSON decompile report:
  call graph (`CALL*`, `CALLT`, `SYSCALL`), slot cross-references, and inferred primitive/collection types
- Syscall lifting that resolves human-readable names and suppresses phantom
  temporaries for known void syscalls (e.g., Runtime.Notify, Storage.Put)
- A simple pseudocode view mirroring the decoded instruction stream
- A C# contract skeleton view (`--format csharp`) that mirrors the manifest
  entry point, emits stubs for additional ABI methods, declares ABI events,
  and adds `[DisplayName]`/`[Safe]` attributes when available
- Label-based control-transfer placeholders use `label_0xNNNN` targets in both high-level and C# views (`goto label_...`; high-level may emit `leave label_...` for exceptional flow)
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

# Fail fast on unknown opcodes (default is tolerant)
./target/release/neo-decompiler disasm --fail-on-unknown-opcodes path/to/contract.nef

# Machine-readable disassembly (tolerant by default)
./target/release/neo-decompiler disasm --format json path/to/contract.nef

# Export the control flow graph as Graphviz DOT
./target/release/neo-decompiler cfg path/to/contract.nef > cfg.dot

# Emit the high-level contract view (auto-detects `*.manifest.json` if present)
./target/release/neo-decompiler decompile path/to/contract.nef

# Enable experimental inlining of single-use temporaries in the high-level view
./target/release/neo-decompiler decompile --inline-single-use-temps path/to/contract.nef

# Fail fast on unknown opcodes during high-level reconstruction
./target/release/neo-decompiler decompile --fail-on-unknown-opcodes path/to/contract.nef

# Emit the legacy pseudocode listing
./target/release/neo-decompiler decompile --format pseudocode path/to/contract.nef

# Emit a C# contract skeleton (includes manifest extras like Author/Email when present)
./target/release/neo-decompiler decompile --format csharp path/to/contract.nef

# Machine-readable decompilation (high-level, pseudocode, manifest path, metadata)
./target/release/neo-decompiler decompile --format json path/to/contract.nef

# Inspect method tokens
./target/release/neo-decompiler tokens path/to/contract.nef

# Machine-readable tokens listing
./target/release/neo-decompiler tokens --format json path/to/contract.nef

# Use --json-compact alongside any JSON format to minimise whitespace
./target/release/neo-decompiler info --format json --json-compact path/to/contract.nef

# Enforce strict manifest validation (reject non-canonical wildcard-like values)
./target/release/neo-decompiler --strict-manifest info --manifest path/to/contract.manifest.json path/to/contract.nef

# Strict mode applies to decompilation too
./target/release/neo-decompiler --strict-manifest decompile --manifest path/to/contract.manifest.json path/to/contract.nef
```

### Strict manifest validation

By default, manifest parsing is permissive to maximize compatibility with
real-world manifests. If you want stricter checks, use `--strict-manifest`
(global flag) to reject non-canonical wildcard-like values.

Current strict checks include:

- `permissions[*].contract` wildcard strings must be exactly `"*"`
- `permissions[*].methods` wildcard strings must be exactly `"*"`
- `trusts` wildcard string must be exactly `"*"`

Library callers can opt into the same behavior via:

- `ContractManifest::from_json_str_strict(...)`
- `ContractManifest::from_file_strict(...)`

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

### From crates.io (recommended)

```bash
cargo install neo-decompiler --locked
```

### From GitHub releases

Download pre-built binaries from the [releases page](https://github.com/r3e-network/neo-decompiler/releases/latest).

### From source

```bash
# Install from a tagged release (replace the tag as needed)
cargo install --git https://github.com/r3e-network/neo-decompiler --tag v0.5.2 --locked

# Or install the latest development version
cargo install --git https://github.com/r3e-network/neo-decompiler --locked

# Or install locally from a checkout
cargo install --path . --locked
```

## Library example

The crate ships with the CLI enabled by default. If you only need the library
APIs and want to avoid pulling in CLI-only dependencies, disable default
features in your `Cargo.toml`:

```toml
neo-decompiler = { version = "0.5.2", default-features = false }
```

```rust
use neo_decompiler::{Decompiler, OutputFormat};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let decompiler = Decompiler::new();
    let result = decompiler.decompile_file_with_manifest(
        "contract.nef",
        Some("contract.manifest.json"),
        OutputFormat::All,
    )?;

    println!("{} instructions", result.instructions.len());
    println!("{} call edges", result.call_graph.edges.len());
    println!("{} methods with xrefs", result.xrefs.methods.len());
    println!("{} inferred static slots", result.types.statics.len());
    if let Some(ref hl) = result.high_level {
        println!("{}", hl);
    }

    // SSA transformation (lazy computation)
    result.compute_ssa();
    if let Some(ssa) = result.ssa() {
        println!("SSA Stats: {}", ssa.stats());
        println!("{}", ssa.render());
    }

    Ok(())
}
```

### Analysis output (library + JSON)

The decompiler also produces best-effort analysis results that can be consumed
programmatically (via the library API) or via `neo-decompiler decompile --format json`
under the top-level `analysis` key:

```json
{
  "analysis": {
    "call_graph": { "methods": [], "edges": [] },
    "xrefs": { "methods": [] },
    "types": { "methods": [], "statics": [] }
  }
}
```

`contract.type` is `Hash` for explicit script hashes, `Group` for public-key groups,
and `Wildcard` when `*` is specified. `methods.type` mirrors the same wildcard vs list
semantics (e.g., `Methods` with `value: ["symbol"]`).

### Built-in metadata coverage

All published Neo N3 opcodes, syscalls, and native contracts are bundled with the
crate so there is no network or tooling dependency at runtime:

- `src/opcodes_generated.rs` is produced by `tools/generate_opcodes.py`, which
  reads `tools/OpCode.cs` or falls back to `neo_csharp/vm/src/Neo.VM/OpCode.cs`
  to emit every mnemonic alongside its byte value and operand encoding.
- `src/syscalls_generated.rs` is produced by `tools/scrape_syscalls.py`, which
  reads the `ApplicationEngine.*.cs` sources (local `neo_csharp` if present,
  otherwise the upstream repo) and writes `tools/data/syscalls.json` alongside
  the Rust table. `crate::syscalls::lookup` wires this into the disassembler and
  high-level view so every `SYSCALL` shows human-readable context.
- `src/native_contracts_generated.rs` is produced by
  `tools/scrape_native_contracts.py`, which reads native contract sources from
  the local `neo_csharp` snapshot and supplements them with upstream sources
  when available, then writes `tools/data/native_contracts.json` alongside
  the Rust table. It enumerates every detected native contract hash plus its
  publicly-exposed methods, ensuring method tokens are annotated with canonical
  names across legacy and latest contract sets when possible.

Re-run the scripts in `tools/` whenever Neo introduces new entries. Each script
overwrites the corresponding generated Rust file (and refreshes the JSON
sidecar), so `git status` immediately highlights the delta and the expanded
coverage is propagated to the CLI and library APIs. With `neo_csharp` available,
the scripts run without network access.

Use the CLI to browse these tables directly:

```
# List all syscalls with hashes, handlers, and call flags
neo-decompiler catalog syscalls

# Machine-readable native-contract catalog
neo-decompiler catalog native-contracts --format json

# Enumerate every opcode and operand encoding
neo-decompiler catalog opcodes
```

## Testing artifacts

Detailed contributor instructions (including CI sweep semantics and
known-unsupported rules) live in `docs/testing-artifacts.md`.

- Drop real contracts anywhere under `TestingArtifacts/` to extend coverage:
  - C# source with embedded manifest/NEF blobs (`*.cs`) are parsed and rewritten into `TestingArtifacts/decompiled/<relative>/`.
  - Paired files (`Example.nef` + `Example.manifest.json`) are also picked up automatically (recursively).
- Known limitations can be listed in `TestingArtifacts/known_unsupported.txt` (one name per line, `#` for comments, optional `path:expected substring` to assert the error text); matching artifacts are allowed to fail and are copied to `*.error.txt`.
- Outputs mirror the artifact layout under `TestingArtifacts/decompiled/`, which is git-ignored by default. Known-unsupported entries are still processed and must emit a non-empty `*.error.txt` to document the failure reason.
- Current samples ship under `TestingArtifacts/edgecases/` (loop lifting, method tokens, manifest metadata, permissions/trusts, call-flag failure, events) and `TestingArtifacts/embedded/` (compiler-style C# with embedded manifest/NEF).

### Extending opcode coverage

The high-level view prints informative comments for opcodes that are not yet lifted
into structured statements (`// XXXX: <MNEMONIC> (not yet translated)`).

- If Neo adds new opcodes, regenerate `src/opcodes_generated.rs` via
  `tools/generate_opcodes.py` (uses `tools/OpCode.cs` or the local
  `neo_csharp/vm/src/Neo.VM/OpCode.cs`) and update the disassembler as needed.
- If you want to improve high-level lifting for existing opcodes, add handling
  in `src/decompiler/high_level/emitter/dispatch.rs` (and related helpers under
  `src/decompiler/high_level/emitter/`), then extend the unit tests under
  `src/decompiler/tests/`.

## Scope and limitations

- NEF checksums are verified using the same double-SHA256 calculation employed
  by the official toolchain. Files with mismatching checksums are rejected.
- The disassembler covers the opcodes exercised by our tests (including the
  various `PUSH*` forms, short/long jumps, calls, and `SYSCALL`). Unrecognised
  opcodes still produce informative comments so you can decide how to extend the
  decoder.
- The high-level contract view (and the derived C# skeleton) performs
  lightweight stack lifting (constants, arithmetic, simple returns, syscalls)
  and recognises structured control flow such as `if`/`else`, `for`, `while`,
  and `do { } while` loops (including `break`/`continue` branches). Full
  source-level reconstruction is intentionally out of scope; CFG and type
  inference are best-effort analysis outputs and may be incomplete.

## Troubleshooting

- **"manifest not provided" in JSON/text output** – ensure the `.manifest.json`
  file sits next to the NEF or pass it explicitly via `--manifest path/to/file`.
- **Manifest path missing in text/JSON output** – both views show the detected
  path (look for `Manifest path:` in text, or `manifest_path` in JSON). If it is
  absent/`null`, pass `--manifest path/to/contract.manifest.json` explicitly.
- **"manifest validation error"** – when using `--strict-manifest`, one or more
  wildcard-like fields were non-canonical (for example `"all"` instead of `"*"`).
  Remove `--strict-manifest` for permissive parsing, or normalize those values.
- **Checksum mismatch errors** – the CLI re-computes the NEF hash; re-build the
  contract to regenerate the NEF or verify you are pointing at the correct file.
- **Unsupported opcode warnings** – the disassembler prints comments for
  unrecognised instructions; add tests and extend `opcodes_generated.rs` if you
  observe new opcodes.
- **Warnings in JSON output** – the `warnings` array can include unknown opcodes
  and high-level translation skips; use `--fail-on-unknown-opcodes` to halt
  instead of continuing with potentially desynchronized output.
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
- `warnings`: Array of human-readable warnings (method token mismatches,
  disassembly unknown opcodes, and high-level translation skips).
- Command-specific payloads:
  - `info`: checksum, script hashes, `method_tokens` (with native annotations)
    and `manifest` summaries (methods, events, permissions, trusts).
  - `disasm`: `instructions` array with `offset`, `opcode`, `operand_kind`, and
    structured `operand_value`.
  - `decompile`: combines the disassembly, `high_level` text, `csharp` view,
    `pseudocode`, and `method_tokens` into one report (C# view carries
    manifest extras such as Author/Email when provided), plus an `analysis`
    object containing the call graph, slot cross-references, and inferred types.
  - `tokens`: standalone `method_tokens` array for quick inspection.

Example (excerpt from `info --format json`):

```json
{
  "file": "path/to/contract.nef",
  "manifest_path": "path/to/contract.manifest.json",
  "manifest": {
    "groups": [
      {
        "pubkey": "03ab...ef",
        "signature": "00ff..."
      }
    ],
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
      "native_contract": {
        "contract": "GasToken",
        "label": "GasToken::Transfer"
      }
    }
  ],
  "warnings": []
}
```

Formal schema files live under [`docs/schema`](docs/schema) for every JSON command
(`info.schema.json`, `disasm.schema.json`, `decompile.schema.json`, `tokens.schema.json`).
See [`docs/schema/README.md`](docs/schema/README.md) for versioning guarantees,
validation instructions, and per-command details. Use
`neo-decompiler schema --list` to discover the available schemas (with version
and description; use `--list-json` for paths), and
`neo-decompiler schema <info|disasm|decompile|tokens>` (optionally with
`--json-compact`, `--output schema.json`, or `--quiet`) to print or persist
them without cloning the repository.

To validate an existing JSON report:

```bash
neo-decompiler info --format json contract.nef > info.json
neo-decompiler schema info --validate info.json
# or pipe via stdin (suppress schema body with --quiet / --no-print)
neo-decompiler schema info --validate - --quiet < info.json
```

## Development

```bash
cargo fmt
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo test --no-default-features
```

If you use [`just`](https://github.com/casey/just), the repository ships with a
`Justfile` providing shortcuts for the common workflows above.
Run `just ci` for the full lint/test/doc matrix used in CI.

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

The MSRV is Rust `1.83`. CI runs the test suite on the MSRV plus stable/beta/nightly
to catch regressions early.

## License

Dual licensed under MIT or Apache-2.0.
See `LICENSE-MIT` and `LICENSE-APACHE` for details.
