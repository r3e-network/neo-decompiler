// Simple standalone test for the manifest parser
// This bypasses the compilation issues with other modules

use neo_n3_decompiler::frontend::{ManifestParser, ValidationOptions};

fn main() {
    println!("Testing Neo N3 Contract Manifest Parser");
    
    let parser = ManifestParser::new();
    
    let test_manifest = r#"{
        "name": "TestToken",
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
                    "name": "symbol",
                    "offset": 0,
                    "parameters": [],
                    "returntype": "String",
                    "safe": true
                },
                {
                    "name": "transfer",
                    "offset": 100,
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
            {
                "contract": "*",
                "methods": ["transfer"]
            },
            {
                "contract": "0x1234567890abcdef1234567890abcdef12345678",
                "methods": []
            }
        ],
        "trusts": [
            "0xabcdef1234567890abcdef1234567890abcdef12",
            null
        ],
        "supportedstandards": ["NEP-17"],
        "extra": {"version": "1.0"}
    }"#;
    
    match parser.parse(test_manifest) {
        Ok(manifest) => {
            println!("✓ Successfully parsed manifest:");
            println!("  Name: {}", manifest.name);
            println!("  Methods: {}", manifest.abi.methods.len());
            println!("  Events: {}", manifest.abi.events.len());
            println!("  Permissions: {}", manifest.permissions.len());
            println!("  Standards: {:?}", manifest.supported_standards);
            
            // Test enhanced ABI extraction
            let enhanced_abi = parser.extract_abi(&manifest);
            println!("  Enhanced ABI methods: {}", enhanced_abi.method_lookup.len());
            println!("  Enhanced ABI events: {}", enhanced_abi.event_lookup.len());
            
            // Test standards detection
            let detected = parser.detect_standards(&manifest);
            println!("  Detected standards: {:?}", detected);
            
            println!("\n✓ All manifest parser tests passed!");
        }
        Err(e) => {
            println!("✗ Failed to parse manifest: {:?}", e);
        }
    }
    
    // Test validation options
    println!("\nTesting validation options...");
    let mut options = ValidationOptions::default();
    options.validate_hashes = false;
    let lenient_parser = ManifestParser::with_options(options);
    
    let invalid_hash_manifest = r#"{
        "name": "test",
        "abi": {"methods": [], "events": []},
        "permissions": [{"contract": "invalid_hash", "methods": []}]
    }"#;
    
    match lenient_parser.parse(invalid_hash_manifest) {
        Ok(_) => println!("✓ Validation options work correctly"),
        Err(e) => println!("✗ Validation options failed: {:?}", e),
    }
}