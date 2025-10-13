# Neo N3 NEF Inspector

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

Issues and pull requests are welcome if they keep the project lean and focused.

## License
MIT OR Apache-2.0
