# Examples

This directory collects small contracts that you can compile with the official
Neo C# compiler (`nccs`) and then feed into the `neo-decompiler` CLI. The
examples are intentionally minimal so you can inspect the resulting NEF payloads
and manifests without needing a full build system.

## Prerequisites

Install the C# compiler as a .NET global tool (requires the .NET SDK 6.0+):

```bash
dotnet tool install -g Neo.Compiler.CSharp
```

The command installs the `nccs` executable and makes it available on your
`PATH`. Verify the installation by running `nccs --help`.

## hello_world

`hello_world/HelloWorld.cs` contains a small contract that returns a string and
exposes a `Notify` method for demonstration purposes.

```bash
# Compile the contract to NEF and manifest files
nccs compile \
  examples/hello_world/HelloWorld.cs \
  --nef build/HelloWorld.nef \
  --manifest build/HelloWorld.manifest.json

# Inspect the output with the decompiler (auto-detects the manifest)
neo-decompiler decompile build/HelloWorld.nef

# Fail fast on unknown opcodes (default is tolerant)
neo-decompiler decompile --fail-on-unknown-opcodes build/HelloWorld.nef
```

The `decompile` command prints both the reconstructed high-level contract view
and the instruction listing. You can swap `decompile` for `info`, `disasm`, or
`tokens` if you want to target a specific portion of the tooling.

Feel free to copy the example contract, add additional methods, and re-run the
steps above to explore how the bytecode evolves.
