# Neo Decompiler

A comprehensive Neo smart contract decompiler that transforms compiled NEF (Neo Executable Format) bytecode into human-readable pseudocode.

## Features

- **Complete Pipeline**: NEF parsing → Disassembly → IR lifting → Analysis → Pseudocode generation
- **Advanced Analysis**: Type inference, control flow analysis, effect tracking, and security analysis
- **Multiple Output Formats**: C-style, Python, Rust, and TypeScript pseudocode syntax
- **Plugin System**: Extensible architecture for custom analysis passes and output formats
- **Standards Support**: Built-in detection and analysis for NEP-17, NEP-11, and other standards
- **Performance Optimized**: Parallel processing and caching for large-scale analysis

## Quick Start

### Installation

```bash
git clone https://github.com/r3e-network/neo-decompiler
cd neo-decompiler
cargo build --release
```

### Basic Usage

```bash
# Decompile a NEF file
./target/release/neo-decompiler decompile contract.nef

# Include contract manifest for better analysis
./target/release/neo-decompiler decompile contract.nef -m contract.manifest.json

# Generate analysis reports
./target/release/neo-decompiler decompile contract.nef --reports --metrics

# Different output formats
./target/release/neo-decompiler decompile contract.nef -f json -o output.json
```

### Library Usage

```rust
use neo_decompiler::{Decompiler, DecompilerConfig};

let config = DecompilerConfig::default();
let decompiler = Decompiler::new(config);

let nef_data = std::fs::read("contract.nef")?;
let manifest = std::fs::read_to_string("contract.manifest.json")?;

let result = decompiler.decompile(&nef_data, Some(&manifest))?;
println!("{}", result.pseudocode);
```

## Architecture

### Modular Design

```text
┌─────────────────────────────────────────────────────────────┐
│                    Neo N3 Decompiler                        │
├─────────────────────────────────────────────────────────────┤
│  Frontend           │  Core Engine        │  Backend        │
│                     │                     │                 │
│  ┌─────────────────┐│ ┌─────────────────┐ │ ┌─────────────┐ │
│  │ NEF Parser      ││ │ Disassembler    │ │ │ IR Dumper   │ │
│  ├─────────────────┤│ ├─────────────────┤ │ ├─────────────┤ │
│  │ Manifest Parser ││ │ Lifter          │ │ │ Pseudocode  │ │
│  ├─────────────────┤│ ├─────────────────┤ │ │ Generator   │ │
│  │ Debug Symbols   ││ │ Decompiler      │ │ ├─────────────┤ │
│  └─────────────────┘│ └─────────────────┘ │ │ Reports     │ │
│                     │                     │ └─────────────┘ │
├─────────────────────┼─────────────────────┼─────────────────┤
│            Analysis Passes Framework                        │
│  ┌─────────────────┬─────────────────┬─────────────────┐   │
│  │ Control Flow    │ Type Inference  │ Optimizations   │   │
│  │ Graph Builder   │ Engine          │ & Transforms    │   │
│  └─────────────────┴─────────────────┴─────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

### Key Components

- **Frontend**: Parses NEF files, contract manifests, and debug symbols
- **Core Engine**: Disassembles bytecode, lifts to IR, and performs decompilation
- **Analysis Framework**: Type inference, control flow analysis, and effect tracking
- **Backend**: Generates pseudocode and analysis reports
- **Plugin System**: Extensible architecture for custom functionality

## Configuration

The decompiler uses TOML configuration files for customization:

```toml
# config/decompiler_config.toml
[analysis]
enable_type_inference = true
enable_effect_analysis = true
parallel_analysis = true

[output]
syntax_style = "CStyle"  # CStyle, Python, Rust, TypeScript
include_type_annotations = true
indent_size = 4

[plugins]
enabled_plugins = ["syscall_analyzer", "nep_detector"]

[performance]
parallel_processing = true
memory_limit_mb = 1024
```

## Plugin Development

Create custom analysis passes and output formats:

```rust
use neo_decompiler::plugins::{AnalysisPlugin, PluginMetadata};

struct MyAnalysisPlugin;

impl AnalysisPlugin for MyAnalysisPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "my_analyzer".to_string(),
            version: "1.0.0".to_string(),
            description: "Custom analysis plugin".to_string(),
        }
    }

    fn analyze_function(&self, function: &mut IRFunction) -> Result<AnalysisResult, PluginError> {
        // Custom analysis logic
        Ok(AnalysisResult::default())
    }
}
```

## Standards Support

Built-in support for Neo standards:

- **NEP-17**: Fungible tokens (automatic detection and specialized analysis)
- **NEP-11**: Non-fungible tokens
- **NEP-24**: Royalty standard
- **Custom Standards**: Configurable via TOML files

## Performance

Designed for high performance with large contracts:

- **Parallel Processing**: Multi-threaded analysis passes
- **Intelligent Caching**: Reuse analysis results across sessions  
- **Memory Efficient**: Streaming processing for large bytecode
- **Configurable Limits**: Memory and timeout controls

### Benchmarks

| Contract Size | Decompilation Time | Memory Usage |
|---------------|-------------------|--------------|
| < 1KB         | < 100ms          | < 10MB       |
| < 10KB        | < 5s             | < 50MB       |
| < 100KB       | < 30s            | < 200MB      |

## Advanced Features

### Type Inference

Sophisticated Hindley-Milner style type inference:

```rust
// Input bytecode
LDARG0    // Load argument 0
LDARG1    // Load argument 1  
ADD       // Add values
RET       // Return result

// Inferred types and generated pseudocode
function transfer(from: Hash160, amount: Integer) -> Integer {
    return from + amount;  // Type mismatch detected and reported
}
```

### Effect Analysis

Track side effects for security analysis:

```rust
// Detected effects
Effects: [
    StorageWrite { key_pattern: "balance_*" },
    EventEmit { event_name: "Transfer" },
    GasConsumption { amount: 1000000 }
]
```

### Control Flow Reconstruction

Advanced control flow analysis with loop detection:

```rust
// Reconstructed control structures
if (condition) {
    // true branch
} else {
    // false branch
}

while (iterator.next()) {
    // loop body
}
```

## Development

### Building from Source

```bash
git clone https://github.com/r3e-network/neo-decompiler
cd neo-decompiler
cargo build --release
```

### Running Tests

```bash
cargo test                    # Unit tests
cargo test --test integration # Integration tests
cargo bench                   # Performance benchmarks
```

### Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests for new functionality
5. Run the test suite
6. Submit a pull request

## Documentation

- [Technical Design Document](TECHNICAL_DESIGN.md)
- [Architecture Overview](docs/architecture.md)
- [Plugin Development Guide](docs/plugin_development.md)
- [API Documentation](docs/api/)

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- Neo Development Team
- Neo Community Contributors
- Rust Community for excellent tooling and libraries