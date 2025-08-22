# Neo N3 Decompiler Project

A comprehensive decompiler project for Neo N3 smart contracts, providing analysis, decompilation, and security assessment capabilities.

## Quick Start

### Installation

```bash
# Install from crates.io (when published)
cargo install neo-n3-decompiler

# Or build from source
git clone https://github.com/neo-project/neo-n3-decompiler
cd neo-n3-decompiler
cargo build --release
```

### Basic Usage

```bash
# Initialize a new decompiler project
neo-decompile init

# Disassemble a NEF file
neo-decompile disasm contract.nef

# Full decompilation with manifest
neo-decompile decompile -m contract.manifest.json -o output.pseudo contract.nef

# Generate control flow graph
neo-decompile cfg --format dot contract.nef > contract.dot

# Security and compliance analysis
neo-decompile analyze --security --nep-compliance contract.nef

# Extract contract information
neo-decompile info --metadata --methods contract.nef
```

## Features

### ✨ Multiple Output Formats
- **Pseudocode**: Generic readable format
- **Python**: Python-like syntax
- **C**: C-like syntax with type annotations
- **Rust**: Rust-like syntax
- **TypeScript**: JavaScript/TypeScript syntax
- **JSON**: Structured data output
- **HTML**: Syntax-highlighted web format

### 🔍 Advanced Analysis
- **Security Analysis**: Vulnerability detection and threat modeling
- **NEP Compliance**: NEP-17, NEP-11, and other standard compliance checking
- **Performance Analysis**: Gas cost analysis and optimization opportunities
- **Code Quality**: Maintainability and best practices assessment

### 📊 Visualization
- **Control Flow Graphs**: GraphViz DOT format for visualization
- **Call Graphs**: Function dependency analysis
- **Data Flow**: Variable usage and dependency tracking

### 🛠️ Developer Tools
- **CLI Interface**: Comprehensive command-line tools
- **Library API**: Embeddable Rust library
- **Plugin System**: Extensible analysis capabilities
- **Configuration**: Flexible configuration system

## CLI Commands

### `disasm` - Disassembly
Pretty disassembly with offsets, operands, and optional comments.

```bash
# Basic disassembly
neo-decompile disasm contract.nef

# With detailed information
neo-decompile disasm --bytes --comments --stats contract.nef

# Save to file
neo-decompile disasm -o contract.asm contract.nef
```

### `cfg` - Control Flow Graph
Generate control flow graphs in various formats.

```bash
# GraphViz DOT format
neo-decompile cfg contract.nef > contract.dot

# JSON format for programmatic use
neo-decompile cfg -f json -o graph.json contract.nef

# With detailed analysis
neo-decompile cfg --show-instructions --analysis contract.nef
```

### `decompile` - Full Decompilation
Complete decompilation to human-readable pseudocode.

```bash
# Basic decompilation
neo-decompile decompile contract.nef

# With manifest and Python output
neo-decompile decompile -m contract.manifest.json -f python -o contract.py contract.nef

# Multiple formats
neo-decompile decompile --multi-format -o contract.pseudo contract.nef

# With performance metrics
neo-decompile decompile --metrics --reports contract.nef
```

### `analyze` - Security and Compliance
Comprehensive analysis for security and standard compliance.

```bash
# Security analysis
neo-decompile analyze --security contract.nef

# NEP compliance checking
neo-decompile analyze --nep-compliance -m contract.manifest.json contract.nef

# Complete analysis
neo-decompile analyze --all --format html -o report.html contract.nef

# Performance analysis
neo-decompile analyze --performance --threshold high contract.nef
```

### `info` - Contract Information
Extract metadata and contract information.

```bash
# Basic information
neo-decompile info contract.nef

# With manifest details
neo-decompile info -m contract.manifest.json --methods --dependencies contract.nef

# JSON output for automation
neo-decompile info -f json --stats --compiler contract.nef
```

## Configuration

### Default Configuration
```toml
# decompiler.toml
[decompiler]
optimization_level = 1
type_inference = true
generate_comments = true
max_iterations = 100

[analysis]
security_checks = true
performance_analysis = false
quality_checks = true

[output]
syntax_highlighting = true
line_numbers = true
include_metadata = false

[logging]
level = "info"
format = "compact"
```

### Configuration Commands
```bash
# Show current configuration
neo-decompile config show

# Generate default configuration
neo-decompile config generate -o decompiler.toml

# Validate configuration
neo-decompile config validate decompiler.toml
```

## Library Usage

### Basic Decompilation
```rust
use neo_decompiler::{Decompiler, DecompilerConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = DecompilerConfig::default();
    let mut decompiler = Decompiler::new(config);
    
    let nef_data = std::fs::read("contract.nef")?;
    let manifest = std::fs::read_to_string("contract.manifest.json")?;
    
    let result = decompiler.decompile(&nef_data, Some(&manifest))?;
    
    println!("Pseudocode:");
    println!("{}", result.pseudocode);
    
    println!("\nContract: {}", result.manifest.as_ref().unwrap().name);
    println!("Instructions: {}", result.instructions.len());
    
    Ok(())
}
```

### Custom Configuration
```rust
use neo_decompiler::{Decompiler, DecompilerConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = DecompilerConfig::default();
    config.optimization_level = 3;
    config.type_inference = true;
    
    let mut decompiler = Decompiler::new(config);
    
    // Decompile with custom settings
    let nef_data = std::fs::read("contract.nef")?;
    let result = decompiler.decompile(&nef_data, None)?;
    
    // Generate different output formats
    println!("Instructions: {}", result.instructions.len());
    println!("Basic blocks: {}", result.ir_function.basic_blocks.len());
    
    Ok(())
}
```

### Analysis Integration
```rust
use neo_decompiler::{Decompiler, DecompilerConfig};

fn analyze_contract(nef_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let config = DecompilerConfig::default();
    let mut decompiler = Decompiler::new(config);
    
    let nef_data = std::fs::read(nef_path)?;
    let result = decompiler.decompile(&nef_data, None)?;
    
    // Analyze the results
    println!("Contract Analysis:");
    println!("- Instructions: {}", result.instructions.len());
    println!("- Basic blocks: {}", result.ir_function.basic_blocks.len());
    println!("- Variables: {}", result.ir_function.variables.len());
    
    // Check for common patterns
    let has_loops = result.ir_function.basic_blocks.len() > 2;
    let has_conditionals = result.pseudocode.contains("if") || result.pseudocode.contains("while");
    
    println!("- Has loops: {}", has_loops);
    println!("- Has conditionals: {}", has_conditionals);
    
    Ok(())
}
```

## Examples

### NEP-17 Token Analysis
```bash
# Download sample NEP-17 contract
wget https://example.com/sample-token.nef
wget https://example.com/sample-token.manifest.json

# Full analysis
neo-decompile analyze --all -m sample-token.manifest.json sample-token.nef

# Generate Python-style decompilation
neo-decompile decompile -f python -m sample-token.manifest.json sample-token.nef
```

### Security Audit
```bash
# Comprehensive security analysis
neo-decompile analyze --security --format sarif -o security-report.sarif contract.nef

# Check NEP compliance
neo-decompile analyze --nep-compliance -m manifest.json contract.nef

# Performance analysis
neo-decompile analyze --performance --threshold medium contract.nef
```

### Development Workflow
```bash
# Initialize project
neo-decompile init ./analysis-project

# Generate configuration
neo-decompile config generate -o ./analysis-project/decompiler.toml

# Batch analysis
for nef in *.nef; do
    echo "Analyzing $nef..."
    neo-decompile analyze --all "$nef" > "${nef%.nef}-analysis.json"
done
```

## Performance

### Benchmarks
Run comprehensive benchmarks:
```bash
cargo bench
```

Typical performance characteristics:
- **NEF Parsing**: ~10μs for typical contracts
- **Disassembly**: ~100μs for 1KB bytecode
- **Complete Decompilation**: ~1-10ms for typical contracts
- **Memory Usage**: ~1-10MB for large contracts

### Optimization Tips
1. **Use appropriate optimization levels** (0-3)
2. **Enable type inference** for better pseudocode
3. **Disable unnecessary analysis** for faster processing
4. **Use streaming mode** for large batch processing

## Contributing

### Development Setup
```bash
git clone https://github.com/neo-project/neo-n3-decompiler
cd neo-n3-decompiler
cargo build
cargo test
```

### Running Tests
```bash
# Unit tests
cargo test --lib

# Integration tests
cargo test --test integration_tests

# CLI tests
cargo test --test cli_tests

# Property-based tests
cargo test --test property_tests

# Benchmarks
cargo bench
```

### Code Quality
```bash
# Format code
cargo fmt

# Lint
cargo clippy

# Check documentation
cargo doc --open
```

## License

Licensed under either of:
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

at your option.

## Acknowledgments

- Neo Development Team
- Neo N3 Virtual Machine specification
- Community contributors and testers

---

For more information, visit the [Neo Developer Documentation](https://docs.neo.org/).