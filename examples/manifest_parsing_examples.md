# Neo N3 Contract Manifest Parser Examples

The Neo N3 Contract Manifest Parser provides comprehensive parsing, validation, and analysis of Neo N3 smart contract manifests.

## Basic Usage

```rust
use neo_decompiler::frontend::{ManifestParser, ValidationOptions};

// Create parser with default validation
let parser = ManifestParser::new();

// Parse a manifest JSON string
let manifest_json = r#"{
    "name": "MyToken",
    "abi": {
        "methods": [
            {
                "name": "balanceOf",
                "offset": 10,
                "parameters": [{"name": "account", "type": "Hash160"}],
                "returntype": "Integer",
                "safe": true
            }
        ],
        "events": [
            {
                "name": "Transfer", 
                "parameters": [
                    {"name": "from", "type": "Hash160"},
                    {"name": "to", "type": "Hash160"},
                    {"name": "amount", "type": "Integer"}
                ]
            }
        ]
    },
    "permissions": [{"contract": "*", "methods": []}],
    "trusts": [],
    "supportedstandards": ["NEP-17"]
}"#;

let manifest = parser.parse(manifest_json)?;
println!("Contract name: {}", manifest.name);
```

## Enhanced ABI Analysis

```rust
// Extract enhanced ABI with type mapping and lookup tables
let enhanced_abi = parser.extract_abi(&manifest);

// Look up method by name with type information
if let Some(method) = enhanced_abi.method_lookup.get("balanceOf") {
    println!("Method: {}", method.base.name);
    println!("Parameter types: {:?}", method.parameter_types);
    println!("Return type: {:?}", method.return_type_mapped);
}

// Look up event by name
if let Some(event) = enhanced_abi.event_lookup.get("Transfer") {
    println!("Event: {}", event.base.name);
    println!("Parameter types: {:?}", event.parameter_types);
}
```

## Standards Detection and Validation

```rust
// Detect supported standards from ABI analysis
let detected_standards = parser.detect_standards(&manifest);
println!("Detected standards: {:?}", detected_standards);

// The parser automatically validates NEP-17 compliance
// if "NEP-17" is listed in supportedstandards
```

## Validation Options

```rust
// Create parser with custom validation options
let mut options = ValidationOptions::default();
options.validate_hashes = false;  // Skip hash format validation
options.check_standards = true;   // Enable standards compliance checking
options.validate_abi = true;      // Enable ABI consistency validation
options.allow_custom_types = true; // Allow non-standard type names

let parser = ManifestParser::with_options(options);
```

## NEP-17 Token Example

```rust
let nep17_manifest = r#"{
    "name": "MyNEP17Token",
    "features": {
        "storage": true,
        "payable": false
    },
    "abi": {
        "methods": [
            {
                "name": "symbol",
                "offset": 0,
                "parameters": [],
                "returntype": "String",
                "safe": true
            },
            {
                "name": "decimals", 
                "offset": 10,
                "parameters": [],
                "returntype": "Integer",
                "safe": true
            },
            {
                "name": "totalSupply",
                "offset": 20, 
                "parameters": [],
                "returntype": "Integer",
                "safe": true
            },
            {
                "name": "balanceOf",
                "offset": 30,
                "parameters": [{"name": "account", "type": "Hash160"}],
                "returntype": "Integer", 
                "safe": true
            },
            {
                "name": "transfer",
                "offset": 40,
                "parameters": [
                    {"name": "from", "type": "Hash160"},
                    {"name": "to", "type": "Hash160"},
                    {"name": "amount", "type": "Integer"},
                    {"name": "data", "type": "Any"}
                ],
                "returntype": "Boolean",
                "safe": false
            }
        ],
        "events": [
            {
                "name": "Transfer",
                "parameters": [
                    {"name": "from", "type": "Hash160"},
                    {"name": "to", "type": "Hash160"},
                    {"name": "amount", "type": "Integer"}
                ]
            }
        ]
    },
    "permissions": [
        {"contract": "*", "methods": ["onNEP17Payment"]}
    ],
    "trusts": [],
    "supportedstandards": ["NEP-17"]
}"#;

let manifest = parser.parse(nep17_manifest)?;

// The parser will automatically validate NEP-17 compliance
let detected = parser.detect_standards(&manifest);
assert!(detected.contains(&"NEP-17".to_string()));
```

## Multi-Signature Contract Example

```rust
let multisig_manifest = r#"{
    "name": "MultiSigWallet",
    "groups": [
        {
            "pubkey": "0279bedd5df3c6f2e8ccc0f8f2b8e5c1f5d8c2e1a5c7a4b2f2a3d2e1c4b5a6e7f8",
            "signature": "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef"
        }
    ],
    "features": {
        "storage": true,
        "payable": true
    },
    "abi": {
        "methods": [
            {
                "name": "addOwner",
                "offset": 0,
                "parameters": [{"name": "owner", "type": "Hash160"}],
                "returntype": "Boolean", 
                "safe": false
            },
            {
                "name": "removeOwner",
                "offset": 50,
                "parameters": [{"name": "owner", "type": "Hash160"}],
                "returntype": "Boolean",
                "safe": false  
            }
        ],
        "events": [
            {
                "name": "OwnerAdded",
                "parameters": [{"name": "owner", "type": "Hash160"}]
            }
        ]
    },
    "permissions": [
        {"contract": "*", "methods": []}
    ],
    "trusts": [null],  // Trust all contracts
    "supportedstandards": []
}"#;

let manifest = parser.parse(multisig_manifest)?;
println!("Groups: {}", manifest.groups.len());
println!("Trusts all: {}", matches!(manifest.trusts[0], Trust::Wildcard));
```

## Type System Integration

The parser maps Neo N3 type names to internal type representations for type inference:

```rust
use neo_decompiler::frontend::NeoType;

// Neo type mapping examples:
// "Hash160" -> NeoType::Hash160
// "Integer" -> NeoType::Integer  
// "String" -> NeoType::ByteString
// "Boolean" -> NeoType::Boolean
// "Array" -> NeoType::Array(Box::new(NeoType::Any))
// "Map" -> NeoType::Map(Box::new(NeoType::Any), Box::new(NeoType::Any))

let enhanced_abi = parser.extract_abi(&manifest);
for (name, method) in &enhanced_abi.method_lookup {
    println!("Method {}: {:?}", name, method.parameter_types);
}
```

## Error Handling

```rust
use neo_decompiler::common::errors::ManifestParseError;

match parser.parse(invalid_json) {
    Ok(manifest) => { /* use manifest */ }
    Err(ManifestParseError::Json(e)) => {
        println!("JSON parsing error: {}", e);
    }
    Err(ManifestParseError::MissingField { field }) => {
        println!("Missing required field: {}", field);
    }
    Err(ManifestParseError::InvalidABI) => {
        println!("Invalid ABI structure or type validation failed");
    }
    Err(ManifestParseError::InvalidPermission) => {
        println!("Invalid permission format or hash validation failed");
    }
    Err(ManifestParseError::InvalidGroup) => {
        println!("Invalid group format or signature validation failed");
    }
}
```

## Integration with Decompiler Pipeline

```rust
// In a typical decompiler workflow:
use neo_decompiler::frontend::{NEFParser, ManifestParser};

// Parse both NEF and manifest files
let nef_parser = NEFParser::new();
let manifest_parser = ManifestParser::new();

let nef_file = nef_parser.parse(&nef_bytes)?;
let manifest = manifest_parser.parse(&manifest_json)?;

// Extract enhanced ABI for type inference
let enhanced_abi = manifest_parser.extract_abi(&manifest);

// Use method information during decompilation
for instruction in &nef_file.bytecode {
    if let Some(method_offset) = get_method_at_offset(instruction.offset, &enhanced_abi) {
        // Apply type information during analysis
        println!("Found method: {}", method_offset.base.name);
    }
}
```

## Supported Features

### JSON Schema Validation
- Complete manifest structure validation
- Required field checking ("name", "abi")
- Optional field handling with defaults

### ABI Processing  
- Method definitions with parameters, return types, offsets, safety flags
- Event definitions with typed parameters
- Type validation for all parameters and return types
- Duplicate name detection

### Permission and Trust Handling
- Contract permission parsing with hash validation
- Wildcard permissions ("*")
- Trust relationship parsing
- Contract hash format validation (160-bit hex)

### Type System Integration
- Mapping from Neo N3 type names to internal representations
- Support for complex types (Array, Map, Struct)
- Custom type handling
- Enhanced ABI with lookup tables

### Standards Detection
- Built-in NEP-17 standard definition
- Automatic compliance checking
- Extensible standard definition system
- Standards detection from ABI analysis

### Validation and Error Handling
- Comprehensive validation with configurable options
- Hash format validation (public keys, signatures, contract hashes)
- ABI consistency checking
- Detailed error reporting with context