# Neo N3 NEF Inspector

This project provides a small, well-tested Rust crate and CLI for inspecting
Neo N3 NEF bytecode.  It focuses on the essential pieces that are easy to run
locally: parsing the NEF container (header, method tokens, checksum), decoding a
useful slice of Neo VM opcodes, and printing a readable listing of the bytecode.

## What you get
- NEF header parsing (magic, compiler, version, script length, checksum)
- Method token decoding using the official variable-length encoding
- Disassembly for common opcodes such as `PUSH*`, arithmetic operations, jumps,
  calls, and `SYSCALL`
- A simple pseudocode view that mirrors the decoded instruction stream
- A single binary (`neo-decompiler`) and a reusable library (`neo_decompiler`)

## Quick start
```bash
# Build the binary
cargo build --release

# Print header information
./target/release/neo-decompiler info path/to/contract.nef

# Decode instructions
./target/release/neo-decompiler disasm path/to/contract.nef

# Emit the compact pseudocode view
./target/release/neo-decompiler decompile path/to/contract.nef

# Inspect method tokens
./target/release/neo-decompiler tokens path/to/contract.nef
```

## Library example
```rust
use neo_decompiler::Decompiler;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let decompiler = Decompiler::new();
    let result = decompiler.decompile_file("contract.nef")?;

    println!("{} instructions", result.instructions.len());
    println!("{}", result.pseudocode);
    Ok(())
}
```

## Scope and limitations
- NEF checksums are verified using the same double-SHA256 calculation employed
  by the official toolchain.  Files with mismatching checksums are rejected.
- The disassembler covers the opcodes exercised by our tests (including the
  various `PUSH*` forms, short/long jumps, calls, and `SYSCALL`).  Unrecognised
  opcodes still produce informative errors so you can decide how to extend the
  decoder.
- Manifest files and higher-level analyses (control-flow graphs, type
  reconstruction, etc.) are intentionally out of scope.

## Development
```bash
cargo fmt
cargo test
```

Issues and pull requests are welcome if they keep the project lean and focused.

## License
MIT OR Apache-2.0
