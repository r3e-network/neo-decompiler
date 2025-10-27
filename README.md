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
- Method token decoding using the official variable-length encoding
- Full opcode coverage generated from the upstream Neo VM source
- Manifest parsing (`.manifest.json`) with ABI, feature, and permission details
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

# Decode instructions
./target/release/neo-decompiler disasm path/to/contract.nef

# Emit the high-level contract view (auto-detects `*.manifest.json` if present)
./target/release/neo-decompiler decompile path/to/contract.nef

# Emit the legacy pseudocode listing
./target/release/neo-decompiler decompile --format pseudocode path/to/contract.nef

# Inspect method tokens
./target/release/neo-decompiler tokens path/to/contract.nef
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

## Scope and limitations
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
